//! Langfuse-specific span helpers for the agent harness.
//!
//! Rig already emits `gen_ai.*` OpenTelemetry fields on its own
//! `invoke_agent`, `chat`, and `execute_tool` spans — those are the
//! standard GenAI semantic conventions. What rig does NOT emit are the
//! `langfuse.*` fields that Langfuse's UI reads for its trace
//! input/output panels and observation metadata.
//!
//! This module adds only those Langfuse-specific fields on a thin
//! `chat-turn` wrapper span. The result in Langfuse is:
//!
//! ```text
//! chat-turn (trace: langfuse.trace.input/output, mewcode.mode)
//!   └─ invoke_agent (generation: gen_ai.* — emitted by Rig)
//!        └─ execute_tool (tool: gen_ai.tool.* — emitted by Rig)
//! ```
//!
//! The per-turn `chat` spans and provider `completions` spans are
//! suppressed in `main.rs` (`rig::agent_chat=off,rig::completions=off`)
//! to avoid noisy duplicates.
//!
//! References:
//! - <https://opentelemetry.io/docs/specs/semconv/gen-ai/>
//! - <https://langfuse.com/docs/opentelemetry/get-started>

use mewcode_protocol::{Mode, ModelId};

// ---------------------------------------------------------------------------
// Langfuse-specific constants
// ---------------------------------------------------------------------------

/// Span name for a single agent turn (also the Langfuse trace name).
pub const TRACE_NAME_CHAT_TURN: &str = "chat-turn";

/// Langfuse observation type for LLM generations.
pub const LANGFUSE_OBSERVATION_GENERATION: &str = "generation";

/// Role strings used in observation JSON payloads.
pub const GEN_AI_ROLE_SYSTEM: &str = "system";
pub const GEN_AI_ROLE_USER: &str = "user";
pub const GEN_AI_ROLE_ASSISTANT: &str = "assistant";

// ---------------------------------------------------------------------------
// Span-attribute field names (Langfuse-specific only — gen_ai.* are
// emitted by Rig's own spans).
// ---------------------------------------------------------------------------

/// `langfuse.trace.input` — trace-level input text.
pub const FIELD_LANGFUSE_TRACE_INPUT: &str = "langfuse.trace.input";
/// `langfuse.trace.output` — trace-level output text.
pub const FIELD_LANGFUSE_TRACE_OUTPUT: &str = "langfuse.trace.output";
/// `langfuse.observation.input` — generation-observation input
/// (JSON-encoded `[{\"role\": \"system\", ...}, {\"role\": \"user\", ...}]`).
pub const FIELD_LANGFUSE_OBSERVATION_INPUT: &str = "langfuse.observation.input";
/// `langfuse.observation.output` — generation-observation output
/// (JSON-encoded `{\"role\": \"assistant\", \"content\": \"...\"}`).
pub const FIELD_LANGFUSE_OBSERVATION_OUTPUT: &str = "langfuse.observation.output";
/// `input.value` — duplicate of [`FIELD_LANGFUSE_TRACE_INPUT`] for
/// OpenInference compatibility.
pub const FIELD_INPUT_VALUE: &str = "input.value";
/// `output.value` — duplicate of [`FIELD_LANGFUSE_TRACE_OUTPUT`] for
/// OpenInference compatibility.
pub const FIELD_OUTPUT_VALUE: &str = "output.value";

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
        gen_ai.request.model = model.provider_id(),
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
    // Langfuse trace input: full prompt context (system + user).
    let trace_input = format!("{system_prompt}\n\n{user_text}");
    span.record(FIELD_LANGFUSE_TRACE_INPUT, &trace_input);
    span.record(FIELD_INPUT_VALUE, &trace_input);

    // Langfuse observation input: JSON message array so the Langfuse UI
    // can render the system and user messages separately.
    let input = serde_json::json!([
        { "role": GEN_AI_ROLE_SYSTEM, "content": system_prompt },
        { "role": GEN_AI_ROLE_USER, "content": user_text },
    ]);
    span.record(FIELD_LANGFUSE_OBSERVATION_INPUT, input.to_string());
}

/// Record the turn's output on the current span.
///
/// Exposed as `pub` for the tracing-instrumentation test.
pub fn record_turn_output(span: &tracing::Span, reply: &str) {
    span.record(FIELD_LANGFUSE_TRACE_OUTPUT, reply);
    span.record(FIELD_OUTPUT_VALUE, reply);

    let output = serde_json::json!({
        "role": GEN_AI_ROLE_ASSISTANT,
        "content": reply,
    });
    span.record(FIELD_LANGFUSE_OBSERVATION_OUTPUT, output.to_string());
}
