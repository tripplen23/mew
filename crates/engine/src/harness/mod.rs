//! Long-running agent harness. Owns the conversation state, drives
//! the tool-calling loop, and streams [`mewcode_protocol::StreamEvent`]s
//! back through an mpsc channel until the model stops emitting tool
//! calls or the user cancels.

mod completion;
mod trace;

pub use self::completion::last_user_text;
pub use self::trace::{chat_turn_span, record_turn_input, record_turn_output};

use std::sync::Arc;

use mewcode_protocol::{Message, Mode, ModelId, Role, StreamEvent};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tracing::Instrument;
use uuid::Uuid;

use crate::agent::build_system_prompt;
use crate::config::EngineConfig;
use crate::error::EngineError;
use crate::history::HistoryStrategy;
use crate::memory::MemoryStore;
use crate::provider::{AgentRequest, Provider};
use crate::skills::SkillRegistry;
use crate::tools::ToolRegistry;

/// The agent harness.
#[derive(Clone)]
pub struct Harness {
    model: ModelId,
    mode: Mode,
    cancel: CancellationToken,
    skills: Arc<SkillRegistry>,
    tools: Arc<ToolRegistry>,
    session_id: Option<Uuid>,
    history_strategy: HistoryStrategy,
    memory: Option<MemoryStore>,
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
            cancel: CancellationToken::new(),
            skills,
            tools,
            session_id: None,
            history_strategy: HistoryStrategy::default_raw(),
            memory: None,
        }
    }

    /// Record the chat session id so reported turns are grouped by session in Langfuse.
    pub fn with_session(mut self, session_id: Uuid) -> Self {
        self.session_id = Some(session_id);
        self
    }

    /// Attach a memory store for durable facts. When set, the memory content
    /// is injected into the system prompt as a `# Memory` section.
    pub fn with_memory(mut self, memory: MemoryStore) -> Self {
        self.memory = Some(memory);
        self
    }

    /// Cancel the in-flight stream, if any.
    pub fn cancel(&self) {
        self.cancel.cancel();
    }

    /// The system prompt for the model's first turn.
    pub fn system_prompt(&self) -> String {
        build_system_prompt(self.mode, &self.skills, &self.tools)
    }

    /// Number of skills currently available.
    pub fn skill_count(&self) -> usize {
        self.skills.len()
    }

    /// Tool names currently registered.
    pub fn tool_names(&self) -> Vec<&'static str> {
        self.tools.names()
    }

    /// Invoke the configured Rig agent once and stream the reply as
    /// `Start` → `TextDelta`* → `Finish`. Returns `Err` on any failure and
    /// emits nothing on that path — the caller owns the `Error` event.
    pub async fn run_turn(
        &self,
        messages: &[Message],
        tx: mpsc::Sender<StreamEvent>,
    ) -> Result<(), EngineError> {
        let span = trace::chat_turn_span(self.model, self.mode);
        if let Some(session_id) = self.session_id {
            span.record("langfuse.session.id", session_id.to_string());
        }

        self.run_turn_inner(messages, &tx)
            .instrument(span)
            .await
            .map(|_| ())
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
        &self,
        messages: &[Message],
        tx: &mpsc::Sender<StreamEvent>,
    ) -> Result<String, EngineError> {
        // Resolve the credential first: a missing key must fail before any
        // provider is constructed or any request is built.
        let cfg = EngineConfig::from_env()?;

        // The turn always answers the most recent user message. With no
        // user message there is nothing to send, so fail without a provider.
        let user_text = last_user_text(messages)
            .ok_or_else(|| EngineError::Other("no user message in chat history".to_string()))?;

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
        let history = self.history_strategy.build(&messages[..current_user_pos]);

        // Build the system prompt, optionally injecting durable memory.
        let mut system_prompt = build_system_prompt(self.mode, &self.skills, &self.tools);
        if let Some(memory_section) = self.memory.as_ref().and_then(|m| m.format()) {
            system_prompt.push_str("\n\n");
            system_prompt.push_str(&memory_section);
        }

        let provider = Provider::for_model(self.model, &cfg.api_key, &cfg.base_url)?;
        trace::record_turn_input(&tracing::Span::current(), &system_prompt, &user_text);

        // Emit Start before the first token so the client can prepare.
        let message_id = uuid::Uuid::new_v4();
        let started = std::time::Instant::now();
        tx.send(StreamEvent::Start {
            message_id,
            mode: self.mode,
            model: self.model,
        })
        .await
        .map_err(|e| EngineError::Other(e.to_string()))?;

        // Stream the reply — invoke_agent_streaming emits TextDelta events
        // as tokens arrive from the provider.
        let reply = self
            .stream_agent(provider, system_prompt, history, user_text, tx)
            .await?;
        trace::record_turn_output(&tracing::Span::current(), &reply);

        // Emit Finish, recording wall-clock duration (token counts deferred
        // until provider reports them).
        tx.send(StreamEvent::Finish {
            duration_ms: started.elapsed().as_millis() as u64,
            input_tokens: None,
            output_tokens: None,
        })
        .await
        .map_err(|e| EngineError::Other(e.to_string()))?;

        Ok(reply)
    }

    /// Run exactly one prompt through the routed Rig agent with conversation
    /// history, streaming TextDelta events through `tx`. Kept off the emission
    /// path so `run_turn` emits nothing until a reply exists.
    async fn stream_agent(
        &self,
        provider: Provider,
        system_prompt: String,
        history: Vec<rig_core::completion::Message>,
        user_text: String,
        tx: &mpsc::Sender<StreamEvent>,
    ) -> Result<String, EngineError> {
        provider
            .invoke_agent_streaming(
                AgentRequest {
                    model_id: self.model.provider_id(),
                    system_prompt,
                    history,
                    user_text,
                    max_tokens: Self::MAX_TOKENS,
                    max_turns: Self::MAX_AGENT_TURNS,
                },
                tx,
            )
            .await
    }

    /// Output token cap for a single turn.
    const MAX_TOKENS: u64 = 4096;

    /// Max internal Rig agent turns. No tools are registered yet, so this is a
    /// no-op today; keeping it explicit prevents the next phase from having to
    /// rediscover where multi-turn depth belongs.
    const MAX_AGENT_TURNS: usize = 1;

    /// Emit the success-path event sequence for one turn: exactly one `Start`
    /// carrying this turn's mode and model, then a single `TextDelta` (omitted
    /// when `reply` is empty), then exactly one `Finish`, with zero tool events.
    pub async fn emit_reply(
        &self,
        reply: &str,
        tx: &mpsc::Sender<StreamEvent>,
    ) -> Result<(), EngineError> {
        let started = std::time::Instant::now();
        let message_id = uuid::Uuid::new_v4();

        tx.send(StreamEvent::Start {
            message_id,
            mode: self.mode,
            model: self.model,
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
        })
        .await
        .map_err(|e| EngineError::Other(e.to_string()))?;

        Ok(())
    }
}
