//! In-memory broker for tool approvals.
//!
//! The broker owns pending approval requests and session-scoped allow rules so
//! tool execution can wait for the TUI without storing approval data on disk.

use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use mewcode_protocol::event::{
    CHOICE_ALLOW_ONCE, CHOICE_ALLOW_SESSION, CHOICE_DENY, ChoiceOption, ChoiceRequest,
    ChoiceResponse,
};
use mewcode_protocol::{StreamEvent, ToolError};
use serde_json::Value;
use tokio::sync::{mpsc, oneshot};
use uuid::Uuid;

const APPROVAL_TIMEOUT_MS: u64 = 120_000;

/// Coordinates pending tool approvals and in-memory session allow rules.
#[derive(Clone, Default)]
pub struct ApprovalBroker {
    state: Arc<Mutex<ApprovalState>>,
}

#[derive(Default)]
struct ApprovalState {
    pending: HashMap<String, PendingApproval>,
    allowed: HashSet<ApprovalRule>,
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
            .map(|state| state.allowed.contains(&rule))
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

fn approval_scope(tool_name: &str, input: &Value) -> (String, u64) {
    let display = if tool_name == mewcode_protocol::tool::names::BASH {
        input.get("command").and_then(Value::as_str).unwrap_or("")
    } else {
        input.get("path").and_then(Value::as_str).unwrap_or("")
    };
    let label = if display.is_empty() {
        "this input".to_string()
    } else if tool_name == mewcode_protocol::tool::names::BASH {
        format!("command `{display}`")
    } else {
        format!("path `{display}`")
    };
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    tool_name.hash(&mut hasher);
    display.hash(&mut hasher);
    (label, hasher.finish())
}

fn rejected(tool_name: &str, message: &str) -> ToolError {
    ToolError::Rejected {
        message: format!("{tool_name} blocked: {message}"),
        hint: Some("Ask the user for approval before retrying this tool call.".into()),
    }
}
