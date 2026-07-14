//! Langfuse-specific span helpers for the agent harness.
//!
//! References:
//! - <https://opentelemetry.io/docs/specs/semconv/gen-ai/>
//! - <https://langfuse.com/docs/opentelemetry/get-started>

use mewcode_protocol::{Mode, ModelId};

/// Span name for a single agent turn.
pub const TRACE_NAME_CHAT_TURN: &str = "chat-turn";

/// Langfuse observation type for LLM generations.
pub const LANGFUSE_OBSERVATION_GENERATION: &str = "generation";

/// Role strings used in observation JSON payloads.
pub const GEN_AI_ROLE_SYSTEM: &str = "system";
pub const GEN_AI_ROLE_USER: &str = "user";
pub const GEN_AI_ROLE_ASSISTANT: &str = "assistant";

/// `langfuse.trace.input` — trace-level input text.
pub const FIELD_LANGFUSE_TRACE_INPUT: &str = "langfuse.trace.input";
/// `langfuse.trace.output` — trace-level output text.
pub const FIELD_LANGFUSE_TRACE_OUTPUT: &str = "langfuse.trace.output";
/// `langfuse.observation.input` — generation-observation input
/// (JSON-encoded `[{"role": "user", ...}, {"role": "system", ...}]`).
pub const FIELD_LANGFUSE_OBSERVATION_INPUT: &str = "langfuse.observation.input";
/// `langfuse.observation.output` — generation-observation output
/// (JSON-encoded `{\"role\": \"assistant\", \"content\": \"...\"}`).
pub const FIELD_LANGFUSE_OBSERVATION_OUTPUT: &str = "langfuse.observation.output";

/// Create the `chat-turn` span for one agent turn.
///
/// Only Langfuse-specific fields are declared here. Rig's `invoke_agent`
/// span (a child of this span) carries the `gen_ai.*` fields.
///
/// Exposed as `pub` for the tracing-instrumentation test in
/// `crates/engine/tests/chat_turn_span.rs`.
pub fn chat_turn_span(model: ModelId, mode: Mode) -> tracing::Span {
    tracing::info_span!(
        "chat-turn",
        gen_ai.request.model = model.as_str(),
        mewcode.mode = ?mode,
        langfuse.trace.name = TRACE_NAME_CHAT_TURN,
        langfuse.session.id = tracing::field::Empty,
        langfuse.trace.input = tracing::field::Empty,
        langfuse.trace.output = tracing::field::Empty,
        langfuse.observation.type = LANGFUSE_OBSERVATION_GENERATION,
        langfuse.observation.input = tracing::field::Empty,
        langfuse.observation.output = tracing::field::Empty,
        gen_ai.usage.cache_read.input_tokens = tracing::field::Empty,
        gen_ai.usage.cache_creation.input_tokens = tracing::field::Empty,
    )
}

/// Record the turn's input on the current span.
/// Exposed as `pub` for the tracing-instrumentation test.
pub fn record_turn_input(span: &tracing::Span, system_prompt: &str, user_text: &str) {
    let trace_input = format!("{system_prompt}\n\n{user_text}");
    span.record(FIELD_LANGFUSE_TRACE_INPUT, &trace_input);

    let input = serde_json::json!([
        { "role": GEN_AI_ROLE_USER, "content": user_text },
        { "role": GEN_AI_ROLE_SYSTEM, "content": system_prompt },
    ]);
    span.record(FIELD_LANGFUSE_OBSERVATION_INPUT, input.to_string());
}

/// Record the turn's output on the current span.
///
/// Exposed as `pub` for the tracing-instrumentation test.
pub fn record_turn_output(span: &tracing::Span, reply: &str) {
    span.record(FIELD_LANGFUSE_TRACE_OUTPUT, reply);

    let output = serde_json::json!({
        "role": GEN_AI_ROLE_ASSISTANT,
        "content": reply,
    });
    span.record(FIELD_LANGFUSE_OBSERVATION_OUTPUT, output.to_string());
}
