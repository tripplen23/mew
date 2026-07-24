//! Long-running agent harness. Owns the conversation state, drives
//! the tool-calling loop, and streams [`mewcode_protocol::StreamEvent`]s
//! back through an mpsc channel until the model stops emitting tool
//! calls or the user cancels.

mod compaction;
mod completion;
mod trace;

pub use self::compaction::{CompactionCheckpoint, CompactionState};
pub use self::compaction::{estimate_compacted_context, truncate_fallback};
pub use self::completion::last_user_text;
#[doc(hidden)]
pub use self::completion::user_text_with_file_context;
pub use self::trace::{chat_turn_span, record_turn_input, record_turn_output};

use std::path::PathBuf;
use std::sync::Arc;

use mewcode_protocol::{Message, Mode, ModelId, Role, StreamEvent};
use tokio::sync::mpsc;
use tracing::Instrument;
use uuid::Uuid;

use crate::agent::{Agent, build_system_prompt};
use crate::approval::ApprovalBroker;
use crate::config::EngineConfig;
use crate::error::EngineError;
use crate::history::{self, HistoryStrategy};
use crate::memory::MemoryStore;
use crate::provider::Provider;
use crate::skills::SkillRegistry;
use crate::tools::ToolRegistry;

/// The agent harness.
#[derive(Clone)]
pub struct Harness {
    model: ModelId,
    mode: Mode,
    skills: Arc<SkillRegistry>,
    tools: Arc<ToolRegistry>,
    session_id: Option<Uuid>,
    history_strategy: HistoryStrategy,
    memory: Option<MemoryStore>,
    display_sink: Option<crate::tools::DisplaySink>,
    project_root: Option<PathBuf>,
    approval_broker: Option<ApprovalBroker>,
    pub compaction: CompactionState,
}

impl std::fmt::Debug for Harness {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Harness")
            .field("model", &self.model)
            .field("mode", &self.mode)
            .field("tools", &self.tools.names())
            .field("skill_count", &self.skills.len())
            .finish()
    }
}

impl Harness {
    /// Build a new harness. `skills` is the catalog source for the
    /// system prompt; `tools` supplies the descriptors the model can call.
    pub fn new(
        model: ModelId,
        mode: Mode,
        skills: Arc<SkillRegistry>,
        tools: Arc<ToolRegistry>,
    ) -> Self {
        Self {
            model,
            mode,
            skills,
            tools,
            session_id: None,
            history_strategy: HistoryStrategy::default_raw(),
            memory: None,
            display_sink: None,
            project_root: None,
            approval_broker: None,
            compaction: CompactionState::default(),
        }
    }

    /// Set the project root used to resolve `@file` mentions.
    pub fn with_project_root(mut self, root: impl Into<PathBuf>) -> Self {
        self.project_root = Some(root.into());
        self
    }

    /// Attach the display sink so mutating tools' render-only data (diffs) is
    /// correlated to tool calls and streamed as `ToolDisplayAvailable`.
    pub fn with_display_sink(mut self, sink: crate::tools::DisplaySink) -> Self {
        self.display_sink = Some(sink);
        self
    }

    /// Attach the in-memory approval broker for interactive tool approvals.
    pub fn with_approval_broker(mut self, broker: ApprovalBroker) -> Self {
        self.approval_broker = Some(broker);
        self
    }

    /// Record the chat session id so reported turns are grouped by session in Langfuse.
    pub fn with_session(mut self, session_id: Uuid) -> Self {
        self.session_id = Some(session_id);
        self
    }

    /// Seed the estimated context size from the previous turn.
    pub fn with_session_tokens(mut self, tokens: u64) -> Self {
        self.compaction.context_tokens = tokens;
        self
    }

    /// Set a structurally valid compaction checkpoint from a previous turn.
    /// Invalid or incomplete pairs are ignored so history is never dropped
    /// without a summary that replaces it.
    pub fn with_compaction_summary(
        mut self,
        summary: Option<String>,
        compacted_up_to: usize,
    ) -> Self {
        self.compaction.checkpoint = CompactionCheckpoint::new(summary, compacted_up_to);
        self
    }

    /// Estimated token count for the context produced by the latest turn.
    pub fn session_tokens(&self) -> u64 {
        self.compaction.context_tokens
    }

    pub fn record_context_usage(&mut self, observed_tokens: u64) {
        self.compaction.context_tokens = observed_tokens;
    }

    /// Return a checkpoint created during the current turn, if any.
    pub fn updated_compaction(&self) -> Option<(&str, usize)> {
        self.compaction
            .pending_update
            .as_ref()
            .map(|checkpoint| (checkpoint.summary.as_str(), checkpoint.up_to))
    }

    /// Attach a memory store for durable facts. When set, the memory content
    /// is injected into the system prompt as a `<memory>` section.
    pub fn with_memory(mut self, memory: MemoryStore) -> Self {
        self.memory = Some(memory);
        self
    }

    /// The exact system prompt sent this turn: static sections plus, when
    /// present, the durable-memory section. Single source of truth so
    /// `run_turn_inner` always sends what this returns.
    fn compose_system_prompt(&self) -> String {
        let mut prompt = build_system_prompt(self.mode, &self.skills, &self.tools);
        if let Some(section) = self.memory.as_ref().and_then(|m| m.format()) {
            prompt.push_str("\n\n");
            prompt.push_str(&section);
        }
        prompt
    }

    /// Run one agent invocation, streaming events through the channel.
    /// The agent may make up to `MAX_AGENT_TURNS` sub-turns (tool calls
    /// → results → reply) before finishing. Returns `Err` on any failure
    /// and emits nothing on that path — the caller owns the `Error` event.
    ///
    /// If the provider returns a context-overflow error, compacts history
    /// and retries once.
    pub async fn run_turn(
        &mut self,
        messages: &[Message],
        tx: mpsc::Sender<StreamEvent>,
    ) -> Result<(), EngineError> {
        // A retry belongs to the same turn, so only clear an update left by a
        // previous completed turn here—not inside history preparation.
        self.compaction.pending_update = None;

        let span = trace::chat_turn_span(self.model, self.mode);
        if let Some(session_id) = self.session_id {
            span.record("langfuse.session.id", session_id.to_string());
        }

        match self
            .run_turn_inner(messages, &tx)
            .instrument(span.clone())
            .await
        {
            Ok(_) => Ok(()),
            Err(e) if e.is_context_overflow() => {
                tracing::warn!("context overflow detected, forcing compaction and retrying");
                // Force compaction by setting session_tokens to threshold
                let limit = self.model.context_limit();
                if limit > 0 {
                    self.compaction.context_tokens =
                        (limit as f64 * history::COMPACTION_THRESHOLD) as u64;
                }
                // Retry once with compacted history
                self.run_turn_inner(messages, &tx)
                    .instrument(span)
                    .await
                    .map(|_| ())
            }
            Err(e) => Err(e),
        }
    }

    /// The turn proper: resolve config, select the user message, build
    /// history from prior turns, optionally inject durable memory into
    /// the system prompt, then run one agent invocation streaming
    /// TextDelta events through the channel and emit the Finish event.
    /// Returns the assistant reply on success so the caller can both
    /// report it and discard it. The SSE emission is unchanged —
    /// nothing reaches the channel on failure, so the server route stays
    /// the single owner of the `Error` event.
    async fn run_turn_inner(
        &mut self,
        messages: &[Message],
        tx: &mpsc::Sender<StreamEvent>,
    ) -> Result<String, EngineError> {
        // The turn always answers the most recent user message. With no
        // user message there is nothing to send, so fail without a provider.
        let user_text = if let Some(root) = self.project_root.as_deref() {
            completion::user_text_with_file_context(messages, root)
        } else {
            last_user_text(messages)
        }
        .ok_or_else(|| EngineError::Other("no user message in chat history".to_string()))?;

        // Resolve the credential before any provider is constructed.
        let cfg = EngineConfig::from_env()?;

        // Build history from messages before the current user prompt, so
        // the prompt text is not duplicated when invoke_agent sends it
        // via `.prompt(user_text).with_history(history)`.
        let current_user_pos = messages
            .iter()
            .enumerate()
            .rev()
            .find(|(_, m)| m.role == Role::User)
            .map(|(i, _)| i)
            .unwrap_or(0);
        let prior_messages = &messages[..current_user_pos];

        let history = self.build_turn_history(prior_messages, &cfg, tx).await;

        // Build the system prompt, optionally injecting durable memory.
        let system_prompt = self.compose_system_prompt();

        let provider = Provider::for_model(self.model, &cfg)?;
        trace::record_turn_input(&tracing::Span::current(), &system_prompt, &user_text);

        // Emit Start before the first token so the client can prepare.
        let message_id = Uuid::new_v4();
        let started = std::time::Instant::now();
        let pwd = self
            .project_root
            .as_deref()
            .and_then(|p| p.to_str())
            .map(|s| s.to_string());
        tx.send(StreamEvent::Start {
            message_id,
            mode: self.mode,
            model: self.model,
            pwd,
        })
        .await
        .map_err(|e| EngineError::Other(e.to_string()))?;

        // Stream the reply through the agent layer. Token/turn caps are
        // owned by Agent's defaults; the harness doesn't override them.
        let approved_tools;
        let tools_registry = if self.mode.allows_writes() {
            match (self.session_id, self.approval_broker.clone()) {
                (Some(session_id), Some(broker)) => {
                    approved_tools = self.tools.with_approval(session_id, broker, tx.clone());
                    &approved_tools
                }
                _ => &self.tools,
            }
        } else {
            &self.tools
        };
        let tools = crate::tools::adapter::rig_tools(tools_registry);
        let mut agent = Agent::new(provider, self.model, system_prompt).with_tools(tools);
        if let Some(sink) = self.display_sink.clone() {
            agent = agent.with_display_sink(sink);
        }
        let (reply, usage) = agent.run_turn(user_text, history, tx).await?;
        trace::record_turn_output(&tracing::Span::current(), &reply);

        // Provider input usage already includes the history sent this turn,
        // so it is a context snapshot rather than a value to accumulate.
        if !usage.is_empty() {
            self.record_context_usage(usage.total());
        }

        // Emit Finish with actual token counts.
        tx.send(StreamEvent::Finish {
            duration_ms: started.elapsed().as_millis() as u64,
            input_tokens: if usage.input_tokens > 0 {
                Some(usage.input_tokens)
            } else {
                None
            },
            output_tokens: if usage.output_tokens > 0 {
                Some(usage.output_tokens)
            } else {
                None
            },
            session_tokens: Some(self.compaction.context_tokens),
            context_limit: if self.model.context_limit() > 0 {
                Some(self.model.context_limit())
            } else {
                None
            },
        })
        .await
        .map_err(|e| EngineError::Other(e.to_string()))?;

        Ok(reply)
    }

    /// Emit the success-path event sequence for one turn: exactly one `Start`
    /// carrying this turn's mode and model, then a single `TextDelta` (omitted
    /// when `reply` is empty), then exactly one `Finish`, with zero tool events.
    pub async fn emit_reply(
        &self,
        reply: &str,
        tx: &mpsc::Sender<StreamEvent>,
    ) -> Result<(), EngineError> {
        let started = std::time::Instant::now();
        let message_id = Uuid::new_v4();

        tx.send(StreamEvent::Start {
            message_id,
            mode: self.mode,
            model: self.model,
            pwd: self
                .project_root
                .as_deref()
                .and_then(|p| p.to_str())
                .map(|s| s.to_string()),
        })
        .await
        .map_err(|e| EngineError::Other(e.to_string()))?;

        if !reply.is_empty() {
            tx.send(StreamEvent::TextDelta {
                delta: reply.to_string(),
            })
            .await
            .map_err(|e| EngineError::Other(e.to_string()))?;
        }

        tx.send(StreamEvent::Finish {
            duration_ms: started.elapsed().as_millis() as u64,
            input_tokens: None,
            output_tokens: None,
            session_tokens: Some(self.compaction.context_tokens),
            context_limit: if self.model.context_limit() > 0 {
                Some(self.model.context_limit())
            } else {
                None
            },
        })
        .await
        .map_err(|e| EngineError::Other(e.to_string()))?;

        Ok(())
    }
}
