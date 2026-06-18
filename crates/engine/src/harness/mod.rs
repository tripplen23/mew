//! Long-running agent harness. Owns the conversation state, drives
//! the tool-calling loop, and streams [`mewcode_protocol::StreamEvent`]s
//! back through an mpsc channel until the model stops emitting tool
//! calls or the user cancels.

mod completion;

pub use self::completion::{last_user_text, reply_text};

use std::sync::Arc;

use mewcode_protocol::{Message, Mode, ModelId, StreamEvent};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tracing::Instrument;
use uuid::Uuid;

use crate::agent::build_system_prompt;
use crate::config::EngineConfig;
use crate::error::EngineError;
use crate::provider::Provider;
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
        }
    }

    /// Record the chat session id so reported turns are grouped by session in Langfuse.
    pub fn with_session(mut self, session_id: Uuid) -> Self {
        self.session_id = Some(session_id);
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

    /// Run one completion against OpenCode Go and stream the reply as
    /// `Start` → `TextDelta`* → `Finish`. Returns `Err` on any failure and
    /// emits nothing on that path — the caller owns the `Error` event.
    pub async fn run_turn(
        &self,
        messages: &[Message],
        tx: mpsc::Sender<StreamEvent>,
    ) -> Result<(), EngineError> {
        let span = self.chat_turn_span();
        if let Some(session_id) = self.session_id {
            span.record("langfuse.session.id", session_id.to_string());
        }

        self.run_turn_inner(messages, &tx)
            .instrument(span)
            .await
            .map(|_| ())
    }

    fn chat_turn_span(&self) -> tracing::Span {
        tracing::info_span!(
            "chat-turn",
            gen_ai.operation.name = "invoke_agent",
            gen_ai.agent.name = "mewcode",
            gen_ai.provider.name = "opencode-go",
            gen_ai.request.model = self.model.provider_id(),
            gen_ai.request.max_tokens = Self::MAX_TOKENS,
            gen_ai.system_instructions = tracing::field::Empty,
            gen_ai.prompt = tracing::field::Empty,
            gen_ai.completion = tracing::field::Empty,
            gen_ai.response.id = tracing::field::Empty,
            gen_ai.response.model = tracing::field::Empty,
            gen_ai.usage.input_tokens = tracing::field::Empty,
            gen_ai.usage.output_tokens = tracing::field::Empty,
            gen_ai.usage.cache_read.input_tokens = tracing::field::Empty,
            gen_ai.usage.cache_creation.input_tokens = tracing::field::Empty,
            gen_ai.usage.tool_use_prompt_tokens = tracing::field::Empty,
            gen_ai.usage.reasoning_tokens = tracing::field::Empty,
            mewcode.mode = ?self.mode,
            langfuse.trace.name = "chat-turn",
            langfuse.session.id = tracing::field::Empty,
            langfuse.trace.input = tracing::field::Empty,
            langfuse.trace.output = tracing::field::Empty,
            langfuse.observation.type = "generation",
            langfuse.observation.input = tracing::field::Empty,
            langfuse.observation.output = tracing::field::Empty,
            input.value = tracing::field::Empty,
            output.value = tracing::field::Empty,
        )
    }

    /// The turn proper: resolve config, select the user message, run one
    /// completion, and emit the success-path events. Returns the assistant
    /// reply on success so the caller can both report it and discard it. The
    /// SSE emission is unchanged — nothing reaches the channel on failure, so
    /// the server route stays the single owner of the `Error` event.
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

        let provider = Provider::for_model(self.model, &cfg.api_key, &cfg.base_url)?;
        let system_prompt = build_system_prompt(self.mode, &self.skills, &self.tools);
        Self::record_turn_input(&tracing::Span::current(), &system_prompt, &user_text);

        // Exactly one completion, no tool dispatch, no follow-up turn.
        let reply = self.complete(provider, system_prompt, user_text).await?;
        Self::record_turn_output(&tracing::Span::current(), &reply);

        // Only on success do we emit anything, so a failed completion leaves the
        // channel untouched for the caller's single `Error` event.
        self.emit_reply(&reply, tx).await?;
        Ok(reply)
    }

    /// Run exactly one completion through the routed provider and fold its text
    /// segments into one reply string. Kept off the emission path so `run_turn`
    /// emits nothing until a reply exists.
    async fn complete(
        &self,
        provider: Provider,
        system_prompt: String,
        user_text: String,
    ) -> Result<String, EngineError> {
        let choice = provider
            .complete(
                self.model.provider_id(),
                system_prompt,
                user_text,
                Self::MAX_TOKENS,
            )
            .await?;
        Ok(reply_text(&choice))
    }

    /// Output token cap for a single turn.
    const MAX_TOKENS: u64 = 4096;

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

    fn record_turn_input(span: &tracing::Span, system_prompt: &str, user_text: &str) {
        span.record("gen_ai.system_instructions", system_prompt);
        span.record("gen_ai.prompt", user_text);
        span.record("langfuse.trace.input", user_text);
        span.record("input.value", user_text);

        let input = serde_json::json!({
            "role": "user",
            "content": user_text,
        });
        span.record("langfuse.observation.input", input.to_string());
    }

    fn record_turn_output(span: &tracing::Span, reply: &str) {
        span.record("gen_ai.completion", reply);
        span.record("langfuse.trace.output", reply);
        span.record("output.value", reply);

        let output = serde_json::json!({
            "role": "assistant",
            "content": reply,
        });
        span.record("langfuse.observation.output", output.to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    use tracing::field::{Field, Visit};
    use tracing::{Id, Subscriber};
    use tracing_subscriber::layer::{Context, SubscriberExt};
    use tracing_subscriber::{Layer, Registry};

    #[derive(Clone, Default)]
    struct Records(Arc<Mutex<Vec<(String, String)>>>);

    impl Records {
        fn contains(&self, field: &str, value: &str) -> bool {
            self.0
                .lock()
                .expect("records lock")
                .iter()
                .any(|(f, v)| f == field && v == value)
        }
    }

    struct CaptureLayer(Records);

    impl<S: Subscriber> Layer<S> for CaptureLayer {
        fn on_record(&self, _span: &Id, values: &tracing::span::Record<'_>, _ctx: Context<'_, S>) {
            values.record(&mut CaptureVisitor(&self.0));
        }
    }

    struct CaptureVisitor<'a>(&'a Records);

    impl Visit for CaptureVisitor<'_> {
        fn record_str(&mut self, field: &Field, value: &str) {
            self.0
                .0
                .lock()
                .expect("records lock")
                .push((field.name().to_string(), value.to_string()));
        }

        fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
            self.0
                .0
                .lock()
                .expect("records lock")
                .push((field.name().to_string(), format!("{value:?}")));
        }
    }

    #[test]
    fn chat_turn_span_records_langfuse_io_fields() {
        let records = Records::default();
        let subscriber = Registry::default().with(CaptureLayer(records.clone()));
        let _guard = tracing::subscriber::set_default(subscriber);

        let harness = Harness::new(
            ModelId::default(),
            Mode::default(),
            Arc::new(SkillRegistry::default()),
            Arc::new(ToolRegistry::new()),
        );
        let span = harness.chat_turn_span();

        // If any field was not declared when the span was created, tracing drops
        // this record call and the assertion below fails.
        Harness::record_turn_input(&span, "system", "hello");
        Harness::record_turn_output(&span, "pong");

        assert!(records.contains("langfuse.trace.input", "hello"));
        assert!(records.contains("langfuse.trace.output", "pong"));
        assert!(records.contains("gen_ai.prompt", "hello"));
        assert!(records.contains("gen_ai.completion", "pong"));
    }
}
