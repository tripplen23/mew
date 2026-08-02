//! Long-running agent harness. Owns the conversation state, drives
//! the tool-calling loop, and streams [`mewcode_protocol::StreamEvent`]s
//! back through an mpsc channel until the model stops emitting tool
//! calls or the user cancels.

mod completion;
mod turn_compaction;

pub use self::completion::last_user_text;
#[doc(hidden)]
pub use self::completion::user_text_with_file_context;
pub use self::turn_compaction::{
    CompactionBlocked, CompactionCheckpoint, CompactionMode, CompactionState, accept_summary,
    should_compact_history,
};
pub use crate::compaction::estimate_compacted_context;

use std::path::PathBuf;
use std::sync::Arc;

use mewcode_protocol::{Message, Mode, ModelId, Role, StreamEvent};
use tokio::sync::mpsc;
use tracing::Instrument;
use uuid::Uuid;

use crate::agent::{Agent, AgentActivity, Provider, build_system_prompt};
use crate::config::EngineConfig;
use crate::context::{HistoryStrategy, MemoryStore};
use crate::error::EngineError;
use crate::observability::langfuse;
use crate::observability::langfuse::FIELD_LANGFUSE_SESSION_ID;
use crate::skills::SkillRegistry;
use crate::tools::{ApprovalBroker, ToolRegistry};

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
    compaction: CompactionState,
    engine_config: Option<EngineConfig>,
    max_tokens: Option<u64>,
    max_turns: Option<usize>,
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

/// Turn-level error wrapper that tracks whether the agent emitted text or
/// tool activity before the error occurred — needed by the context-overflow
/// retry gate. Exposed for tests.
#[derive(Debug)]
pub struct AttemptError {
    pub error: EngineError,
    pub agent_activity: bool,
}

impl AttemptError {
    #[doc(hidden)]
    pub fn new(error: EngineError, agent_activity: bool) -> Self {
        Self {
            error,
            agent_activity,
        }
    }
}

/// Retry a context-overflow turn only when the first attempt produced no
/// activity (no text, no tool calls). Exposed for tests.
pub fn should_retry_after_compaction(attempt: &AttemptError) -> bool {
    attempt.error.is_context_overflow() && !attempt.agent_activity
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
            engine_config: None,
            max_tokens: None,
            max_turns: None,
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
        compacted_up_to_message_id: Option<Uuid>,
    ) -> Self {
        self.compaction.checkpoint =
            CompactionCheckpoint::new(summary, compacted_up_to, compacted_up_to_message_id);
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
    pub fn updated_compaction(&self) -> Option<(&str, usize, Uuid)> {
        self.compaction.pending_update.as_ref().map(|checkpoint| {
            (
                checkpoint.summary.as_str(),
                checkpoint.up_to,
                checkpoint.compacted_up_to_message_id,
            )
        })
    }

    /// Attach a memory store for durable facts. When set, the memory content
    /// is injected into the system prompt as a `<memory>` section.
    pub fn with_memory(mut self, memory: MemoryStore) -> Self {
        self.memory = Some(memory);
        self
    }

    /// Override the engine config used for provider resolution.
    /// When set, this is used instead of calling `EngineConfig::from_env()`.
    pub fn with_engine_config(mut self, cfg: EngineConfig) -> Self {
        self.engine_config = Some(cfg);
        self
    }

    /// Override the per-turn completion token cap.
    pub fn with_max_tokens(mut self, max_tokens: u64) -> Self {
        self.max_tokens = Some(max_tokens);
        self
    }

    /// Override the maximum number of model/tool turns.
    pub fn with_max_turns(mut self, max_turns: usize) -> Self {
        self.max_turns = Some(max_turns);
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
    /// Context overflow is retried once only when the first attempt emitted no
    /// text or tool activity; the retry always performs real summarization.
    pub async fn run_turn(
        &mut self,
        messages: &[Message],
        tx: mpsc::Sender<StreamEvent>,
    ) -> Result<(), EngineError> {
        self.compaction.pending_update = None;

        // Validate the request and credentials before Start so boundary
        // failures still produce only the caller-owned Error event.
        let user_text = if let Some(root) = self.project_root.as_deref() {
            completion::user_text_with_file_context(messages, root)
        } else {
            last_user_text(messages)
        }
        .ok_or_else(|| EngineError::Other("no user message in chat history".to_string()))?;
        let cfg = self
            .engine_config
            .clone()
            .map(Ok)
            .unwrap_or_else(EngineConfig::from_env)?;
        let current_user_pos = messages
            .iter()
            .enumerate()
            .rev()
            .find(|(_, message)| message.role == Role::User)
            .map_or(0, |(index, _)| index);
        let prior_messages = &messages[..current_user_pos];

        let span = langfuse::chat_turn_span(self.model, self.mode);
        if let Some(session_id) = self.session_id {
            span.record(FIELD_LANGFUSE_SESSION_ID, session_id.to_string());
        }

        let started = std::time::Instant::now();
        // Validate the credential before Start so boundary failures (missing
        // key, unsupported model) produce only the caller-owned Error event —
        // no Start precedes it.
        let provider = Provider::for_model(self.model, &cfg)?;
        tx.send(StreamEvent::Start {
            message_id: Uuid::new_v4(),
            mode: self.mode,
            model: self.model,
            pwd: self
                .project_root
                .as_deref()
                .and_then(|path| path.to_str())
                .map(str::to_string),
        })
        .await
        .map_err(|error| EngineError::Other(error.to_string()))?;

        let first = self
            .run_turn_attempt(
                &user_text,
                prior_messages,
                &cfg,
                &provider,
                &tx,
                false,
                started,
            )
            .instrument(span.clone())
            .await;
        match first {
            Ok(()) => Ok(()),
            Err(attempt) if should_retry_after_compaction(&attempt) => {
                tracing::warn!("context overflow before agent activity; forcing compaction");
                self.run_turn_attempt(
                    &user_text,
                    prior_messages,
                    &cfg,
                    &provider,
                    &tx,
                    true,
                    started,
                )
                .instrument(span)
                .await
                .map_err(|attempt| attempt.error)
            }
            Err(attempt) => Err(attempt.error),
        }
    }

    #[allow(clippy::too_many_arguments)]
    async fn run_turn_attempt(
        &mut self,
        user_text: &str,
        prior_messages: &[Message],
        cfg: &EngineConfig,
        provider: &Provider,
        tx: &mpsc::Sender<StreamEvent>,
        force_compaction: bool,
        started: std::time::Instant,
    ) -> Result<(), AttemptError> {
        let history = if force_compaction {
            // Report the real reason: "nothing left to compact" and "the
            // compaction call itself failed" are very different diagnoses.
            self.build_forced_turn_history(prior_messages, cfg, tx)
                .await
                .map_err(|blocked| {
                    let error = match blocked {
                        CompactionBlocked::NothingToCompact => EngineError::ContextOverflow(
                            "no compactable history remains after context overflow".into(),
                        ),
                        CompactionBlocked::SummaryUnavailable(error) => error,
                    };
                    AttemptError::new(error, false)
                })?
        } else {
            self.build_turn_history(prior_messages, cfg, tx).await
        };
        let system_prompt = self.compose_system_prompt();
        langfuse::record_turn_input(&tracing::Span::current(), &system_prompt, user_text);

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
        let mut agent = Agent::new(provider.clone(), self.model, system_prompt).with_tools(tools);
        if let Some(max_tokens) = self.max_tokens {
            agent = agent.with_max_tokens(max_tokens);
        }
        if let Some(max_turns) = self.max_turns {
            agent = agent.with_max_turns(max_turns);
        }
        if let Some(sink) = self.display_sink.clone() {
            agent = agent.with_display_sink(sink);
        }

        let activity = AgentActivity::default();
        let result = agent
            .run_turn(user_text.to_string(), history, tx, activity.clone())
            .await;
        let (reply, usage) =
            result.map_err(|error| AttemptError::new(error, activity.was_observed()))?;
        langfuse::record_turn_output(&tracing::Span::current(), &reply);

        if !usage.is_empty() {
            self.record_context_usage(usage.total());
        }

        tx.send(StreamEvent::Finish {
            duration_ms: started.elapsed().as_millis() as u64,
            input_tokens: (usage.input_tokens > 0).then_some(usage.input_tokens),
            output_tokens: (usage.output_tokens > 0).then_some(usage.output_tokens),
            session_tokens: Some(self.compaction.context_tokens),
            context_limit: (self.model.context_limit() > 0).then(|| self.model.context_limit()),
            cost_usd: usage
                .cost
                .or_else(|| crate::helpers::pricing::turn_cost_usd(self.model, usage)),
        })
        .await
        .map_err(|error| {
            AttemptError::new(
                EngineError::Other(error.to_string()),
                activity.was_observed(),
            )
        })?;

        Ok(())
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
            cost_usd: None,
        })
        .await
        .map_err(|e| EngineError::Other(e.to_string()))?;

        Ok(())
    }
}
