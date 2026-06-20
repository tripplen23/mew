//! Tracing constants and span helpers for the agent harness.
//!
//! All Langfuse/OpenTelemetry semantic-convention identifiers and the
//! `chat-turn` span construction live here so [`super::Harness`] stays
//! focused on the turn lifecycle. References:
//! - <https://opentelemetry.io/docs/specs/semconv/gen-ai/>
//! - <https://langfuse.com/docs/opentelemetry/get-started/>

use mewcode_protocol::{Mode, ModelId};

// ---------------------------------------------------------------------------
// OpenTelemetry / Langfuse semantic convention identifiers
// ---------------------------------------------------------------------------

/// Span name for a single agent turn (also the Langfuse trace name).
pub const TRACE_NAME_CHAT_TURN: &str = "chat-turn";

/// Gen AI operation name (`gen_ai.operation.name`). Wraps an agent invocation.
pub const GEN_AI_OP_INVOKE_AGENT: &str = "invoke_agent";

/// Agent identity (`gen_ai.agent.name`).
pub const GEN_AI_AGENT_MEWCODE: &str = "mewcode";

/// Provider identity (`gen_ai.provider.name`).
pub const GEN_AI_PROVIDER_OPENCODE_GO: &str = "opencode-go";

/// Langfuse observation type for LLM generations.
pub const LANGFUSE_OBSERVATION_GENERATION: &str = "generation";

/// Role strings used in observation [`serde_json::Value`] payloads.
pub const GEN_AI_ROLE_USER: &str = "user";
pub const GEN_AI_ROLE_ASSISTANT: &str = "assistant";

// ---------------------------------------------------------------------------
// Span-attribute field names passed to [`tracing::Span::record`].
//
// Each corresponds to a field declared in the `chat-turn`
// [`tracing::info_span!`] macro below; mismatches are silently dropped
// by tracing.
// ---------------------------------------------------------------------------

/// `gen_ai.system_instructions` — system prompt.
pub const FIELD_GEN_AI_SYSTEM_INSTRUCTIONS: &str = "gen_ai.system_instructions";
/// `gen_ai.prompt` — user prompt.
pub const FIELD_GEN_AI_PROMPT: &str = "gen_ai.prompt";
/// `gen_ai.completion` — assistant reply.
pub const FIELD_GEN_AI_COMPLETION: &str = "gen_ai.completion";
/// `langfuse.trace.input` — trace-level input text.
pub const FIELD_LANGFUSE_TRACE_INPUT: &str = "langfuse.trace.input";
/// `langfuse.trace.output` — trace-level output text.
pub const FIELD_LANGFUSE_TRACE_OUTPUT: &str = "langfuse.trace.output";
/// `langfuse.observation.input` — generation-observation input
/// (JSON-encoded `{"role": "user", "content": "..."}`).
pub const FIELD_LANGFUSE_OBSERVATION_INPUT: &str = "langfuse.observation.input";
/// `langfuse.observation.output` — generation-observation output
/// (JSON-encoded `{"role": "assistant", "content": "..."}`).
pub const FIELD_LANGFUSE_OBSERVATION_OUTPUT: &str = "langfuse.observation.output";
/// `input.value` — duplicate of [`FIELD_LANGFUSE_TRACE_INPUT`] for
/// OpenInference compatibility.
pub const FIELD_INPUT_VALUE: &str = "input.value";
/// `output.value` — duplicate of [`FIELD_LANGFUSE_TRACE_OUTPUT`] for
/// OpenInference compatibility.
pub const FIELD_OUTPUT_VALUE: &str = "output.value";

/// Max tokens recorded on the span — kept here so the [`super::Harness`]
/// constant and the span declaration stay in sync.
pub const MAX_TOKENS: u64 = 4096;

/// Create the [`tracing::Span`] for one agent turn.
///
/// Exposed as `pub` for the tracing-instrumentation test in
/// `crates/engine/tests/chat_turn_span.rs`.
pub fn chat_turn_span(model: ModelId, mode: Mode) -> tracing::Span {
    tracing::info_span!(
        "chat-turn",
        gen_ai.operation.name = GEN_AI_OP_INVOKE_AGENT,
        gen_ai.agent.name = GEN_AI_AGENT_MEWCODE,
        gen_ai.provider.name = GEN_AI_PROVIDER_OPENCODE_GO,
        gen_ai.request.model = model.provider_id(),
        gen_ai.request.max_tokens = MAX_TOKENS,
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
        mewcode.mode = ?mode,
        langfuse.trace.name = TRACE_NAME_CHAT_TURN,
        langfuse.session.id = tracing::field::Empty,
        langfuse.trace.input = tracing::field::Empty,
        langfuse.trace.output = tracing::field::Empty,
        langfuse.observation.type = LANGFUSE_OBSERVATION_GENERATION,
        langfuse.observation.input = tracing::field::Empty,
        langfuse.observation.output = tracing::field::Empty,
        input.value = tracing::field::Empty,
        output.value = tracing::field::Empty,
    )
}

/// Record the turn's input on the current span.
///
/// Exposed as `pub` for the tracing-instrumentation test.
pub fn record_turn_input(span: &tracing::Span, system_prompt: &str, user_text: &str) {
    span.record(FIELD_GEN_AI_SYSTEM_INSTRUCTIONS, system_prompt);
    span.record(FIELD_GEN_AI_PROMPT, user_text);
    span.record(FIELD_LANGFUSE_TRACE_INPUT, user_text);
    span.record(FIELD_INPUT_VALUE, user_text);

    let input = serde_json::json!({
        "role": GEN_AI_ROLE_USER,
        "content": user_text,
    });
    span.record(FIELD_LANGFUSE_OBSERVATION_INPUT, input.to_string());
}

/// Record the turn's output on the current span.
///
/// Exposed as `pub` for the tracing-instrumentation test.
pub fn record_turn_output(span: &tracing::Span, reply: &str) {
    span.record(FIELD_GEN_AI_COMPLETION, reply);
    span.record(FIELD_LANGFUSE_TRACE_OUTPUT, reply);
    span.record(FIELD_OUTPUT_VALUE, reply);

    let output = serde_json::json!({
        "role": GEN_AI_ROLE_ASSISTANT,
        "content": reply,
    });
    span.record(FIELD_LANGFUSE_OBSERVATION_OUTPUT, output.to_string());
}
