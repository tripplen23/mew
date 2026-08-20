//! In-memory broker for tool approvals.
//!
//! The broker owns pending approval requests and session-scoped allow rules so
//! tool execution can wait for the TUI without storing approval data on disk.

use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use mewcode_protocol::event::{
    CHOICE_ALLOW_ONCE, CHOICE_ALLOW_SESSION, CHOICE_ALWAYS_ALLOW, CHOICE_DENY, ChoiceOption,
    ChoiceRequest, ChoiceResponse,
};
use mewcode_protocol::{StreamEvent, ToolError};
use serde_json::Value;
use tokio::sync::{mpsc, oneshot};
use uuid::Uuid;

const APPROVAL_TIMEOUT_MS: u64 = 120_000;

/// Host-side callback that persists an always-allow rule: `(tool, scope)`
/// with `None` granting the whole tool. Returns `Err` when persistence
/// fails, so the broker can refuse to grant the rule instead of granting a
/// phantom always-allow that vanishes on restart.
pub type PersistAlwaysAllow =
    Arc<dyn Fn(&'static str, Option<&str>) -> Result<(), String> + Send + Sync>;

/// Coordinates pending tool approvals, in-memory session allow rules, and
/// persistent (cross-session) "always allow" tool rules.
#[derive(Clone, Default)]
pub struct ApprovalBroker {
    state: Arc<Mutex<ApprovalState>>,
}

#[derive(Default)]
struct ApprovalState {
    pending: HashMap<String, PendingApproval>,
    allowed: HashSet<ApprovalRule>,
    /// Cross-session always-allow rules, preloaded from host settings.
    /// Scope semantics live on [`PersistentRule`].
    always_allowed: HashSet<PersistentRule>,
    /// Hook invoked when the user picks "always allow", so the host can
    /// persist the rule beyond this process.
    persist_always_allow: Option<PersistAlwaysAllow>,
}

/// One always-allow rule: `(tool, scope)`. `None` scope = whole tool;
/// `Some` = that one command or path only.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct PersistentRule {
    tool_name: &'static str,
    scope_key: Option<u64>,
}

struct PendingApproval {
    session_id: Uuid,
    tx: oneshot::Sender<ChoiceResponse>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct ApprovalRule {
    session_id: Uuid,
    tool_name: &'static str,
    scope_key: u64,
}

impl ApprovalBroker {
    /// Preload cross-session always-allow rules from host settings.
    /// Each seed is `(tool, scope)` — semantics on [`PersistentRule`].
    pub fn with_always_allowed(self, seeds: Vec<(&'static str, Option<&str>)>) -> Self {
        let rules = seeds
            .into_iter()
            .map(|(tool_name, scope)| PersistentRule {
                tool_name,
                scope_key: scope.map(|display| scope_key_of(tool_name, display)),
            })
            .collect::<Vec<_>>();
        if let Ok(mut state) = self.state.lock() {
            state.always_allowed.extend(rules);
        }
        self
    }

    /// Attach the hook that persists an always-allow rule (host-owned). The
    /// hook receives the tool name and the granted scope (`None` = firm tool).
    pub fn with_persist_always_allow(self, hook: PersistAlwaysAllow) -> Self {
        if let Ok(mut state) = self.state.lock() {
            state.persist_always_allow = Some(hook);
        }
        self
    }

    /// Ask the interactive client to approve a tool call before execution.
    pub async fn approve_tool(
        &self,
        session_id: Uuid,
        tool_name: &'static str,
        input: &Value,
        events: &mpsc::Sender<StreamEvent>,
    ) -> Result<(), ToolError> {
        let (scope_label, scope_key) = approval_scope(tool_name, input);
        let rule = ApprovalRule {
            session_id,
            tool_name,
            scope_key,
        };
        if self
            .state
            .lock()
            .map(|state| {
                state.allowed.contains(&rule)
                    || state.always_allowed.contains(&PersistentRule {
                        tool_name,
                        scope_key: None,
                    })
                    || state.always_allowed.contains(&PersistentRule {
                        tool_name,
                        scope_key: Some(scope_key),
                    })
            })
            .unwrap_or(false)
        {
            return Ok(());
        }

        let request_id = format!("tool-approval-{}", Uuid::new_v4());
        let (tx, rx) = oneshot::channel();
        if let Ok(mut state) = self.state.lock() {
            state
                .pending
                .insert(request_id.clone(), PendingApproval { session_id, tx });
        } else {
            return Err(rejected(tool_name, "approval state unavailable"));
        }

        let request = ChoiceRequest {
            request_id: request_id.clone(),
            title: format!("Approve {tool_name}?"),
            prompt: format!("Allow {tool_name} for {scope_label}?"),
            options: vec![
                ChoiceOption {
                    id: CHOICE_ALLOW_ONCE.into(),
                    label: "Allow once".into(),
                    description: Some("Run only this tool call.".into()),
                },
                ChoiceOption {
                    id: CHOICE_ALLOW_SESSION.into(),
                    label: "Allow this session".into(),
                    description: Some("Run matching calls in this chat session.".into()),
                },
                ChoiceOption {
                    id: CHOICE_ALWAYS_ALLOW.into(),
                    label: "Always allow".into(),
                    description: Some(
                        "Never ask for this specific command or path again, across sessions (saved to Mew settings)."
                            .into(),
                    ),
                },
                ChoiceOption {
                    id: CHOICE_DENY.into(),
                    label: "Deny".into(),
                    description: Some("Return a rejected tool result.".into()),
                },
            ],
            timeout_ms: APPROVAL_TIMEOUT_MS,
        };

        if events
            .send(StreamEvent::ChoiceRequest(request))
            .await
            .is_err()
        {
            self.remove_pending(&request_id);
            return Err(rejected(tool_name, "no interactive client available"));
        }

        let response =
            match tokio::time::timeout(Duration::from_millis(APPROVAL_TIMEOUT_MS), rx).await {
                Ok(Ok(response)) => response,
                _ => {
                    self.remove_pending(&request_id);
                    return Err(rejected(tool_name, "approval timed out"));
                }
            };

        match response {
            ChoiceResponse::Selected {
                request_id: id,
                option_id,
            } if id == request_id && option_id == CHOICE_ALLOW_ONCE => Ok(()),
            ChoiceResponse::Selected {
                request_id: id,
                option_id,
            } if id == request_id && option_id == CHOICE_ALLOW_SESSION => {
                if let Ok(mut state) = self.state.lock() {
                    state.allowed.insert(rule);
                }
                Ok(())
            }
            ChoiceResponse::Selected {
                request_id: id,
                option_id,
            } if id == request_id && option_id == CHOICE_ALWAYS_ALLOW => {
                let scope_display = approval_display(tool_name, input);
                let scope_display = (!scope_display.is_empty()).then_some(scope_display);
                let scope_key = scope_display.as_deref().map(|d| scope_key_of(tool_name, d));
                let hook = self
                    .state
                    .lock()
                    .ok()
                    .and_then(|state| state.persist_always_allow.clone());
                // Persist first: a failed write must not grant a phantom rule
                // that suppresses prompts now and vanishes on restart.
                if let Some(hook) = hook {
                    if let Err(message) = hook(tool_name, scope_display.as_deref()) {
                        return Err(rejected(tool_name, &message));
                    }
                }
                if let Ok(mut state) = self.state.lock() {
                    state.always_allowed.insert(PersistentRule {
                        tool_name,
                        scope_key,
                    });
                }
                Ok(())
            }
            _ => Err(rejected(tool_name, "approval denied")),
        }
    }

    /// Resolve a pending approval response for its owning session.
    pub fn answer(&self, session_id: Uuid, response: ChoiceResponse) -> bool {
        let request_id = match &response {
            ChoiceResponse::Selected { request_id, .. } => request_id,
            ChoiceResponse::Cancelled { request_id, .. } => request_id,
        };
        let pending = match self.state.lock() {
            Ok(mut state)
                if state
                    .pending
                    .get(request_id)
                    .is_some_and(|pending| pending.session_id == session_id) =>
            {
                state.pending.remove(request_id)
            }
            _ => None,
        };
        pending
            .map(|pending| pending.tx.send(response).is_ok())
            .unwrap_or(false)
    }

    fn remove_pending(&self, request_id: &str) {
        if let Ok(mut state) = self.state.lock() {
            state.pending.remove(request_id);
        }
    }
}

/// The scope string a tool call is granted against: the bash command, or the
/// file path for file tools. Empty when the input carries no scope.
fn approval_display(tool_name: &str, input: &Value) -> String {
    if tool_name == mewcode_protocol::tool::names::BASH {
        input
            .get("command")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string()
    } else {
        input
            .get("path")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string()
    }
}

/// Stable scope key for `(tool, scope)` used by session and persistent rules.
fn scope_key_of(tool_name: &str, display: &str) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    tool_name.hash(&mut hasher);
    display.hash(&mut hasher);
    hasher.finish()
}

fn approval_scope(tool_name: &str, input: &Value) -> (String, u64) {
    let display = approval_display(tool_name, input);
    let label = if display.is_empty() {
        "this input".to_string()
    } else if tool_name == mewcode_protocol::tool::names::BASH {
        format!("command `{display}`")
    } else {
        format!("path `{display}`")
    };
    (label, scope_key_of(tool_name, &display))
}

fn rejected(tool_name: &str, message: &str) -> ToolError {
    ToolError::Rejected {
        message: format!("{tool_name} blocked: {message}"),
        hint: Some("Ask the user for approval before retrying this tool call.".into()),
    }
}
