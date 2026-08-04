use serde_json::json;

use crate::{Message, MessagePart, Mode, ModelId, ProviderId};

/// Phase of a manual compaction operation.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "kebab-case")]
pub enum CompactionPhase {
    /// Pruning tool results and low-value content.
    Pruning,
    /// Running LLM to summarize history.
    Summarizing,
    /// Compaction complete.
    Done,
}

/// Choice option id for approving only the current tool call.
pub const CHOICE_ALLOW_ONCE: &str = "allow_once";
/// Choice option id for approving matching calls in the current session.
pub const CHOICE_ALLOW_SESSION: &str = "allow_session";
/// Choice option id for rejecting the pending request.
pub const CHOICE_DENY: &str = "deny";

/// Server → client streaming events. Sent over SSE as JSON lines; the
/// shape mirrors the AI SDK's `UIMessageStreamResponse`.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum StreamEvent {
    /// Stream has started; the assistant message id is known.
    Start {
        /// Id of the assistant message being produced.
        message_id: uuid::Uuid,
        /// Mode the user picked.
        mode: Mode,
        /// Model the user picked.
        model: ModelId,
        /// Server working directory.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pwd: Option<String>,
    },
    /// A chunk of assistant text.
    TextDelta {
        /// Text to append.
        delta: String,
    },
    /// The model is about to call a tool.
    ToolInputAvailable {
        /// Stable id of the call.
        tool_call_id: String,
        /// Name of the tool.
        tool_name: String,
        /// JSON arguments.
        input: serde_json::Value,
    },
    /// A tool call has finished executing.
    ToolOutputAvailable {
        /// Id of the call this result is for.
        tool_call_id: String,
        /// Tool output (already serialised to JSON).
        output: serde_json::Value,
    },
    /// Render-only display data for a tool call (e.g. a code diff).
    ToolDisplayAvailable {
        /// Id of the call this display is for.
        tool_call_id: String,
        /// The render payload.
        display: crate::ToolDisplay,
    },
    /// Runtime asks the interactive client to choose one option.
    ChoiceRequest(ChoiceRequest),
    /// Manual compaction has started.
    CompactionStarted {
        /// Session being compacted.
        session_id: uuid::Uuid,
    },
    /// Compaction progress update.
    CompactionProgress {
        /// Current phase of compaction.
        phase: CompactionPhase,
        /// Human-readable status message.
        message: String,
    },
    /// A chunk of the compaction summary, streamed as the LLM generates it.
    /// Mirrors `TextDelta` but is kept as a distinct variant so the client
    /// can accumulate it separately from any in-flight chat reply.
    CompactionSummaryDelta {
        /// Text to append to the in-progress summary.
        delta: String,
    },
    /// History was compacted during this turn.
    Compacted {
        /// Accumulated tokens before compaction.
        tokens_before: u64,
        /// Model context limit that triggered compaction.
        context_limit: u64,
        /// LLM-generated summary of the compacted history.
        summary: String,
        /// Wall-clock duration of the compaction LLM call in milliseconds.
        thought_duration_ms: u64,
    },
    /// Stream finished successfully.
    Finish {
        /// Wall-clock duration in milliseconds.
        duration_ms: u64,
        /// Input token usage, if reported.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        input_tokens: Option<u64>,
        /// Output token usage, if reported.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        output_tokens: Option<u64>,
        /// Current session token total.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        session_tokens: Option<u64>,
        /// Model context limit.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        context_limit: Option<u64>,
        /// Cost of this turn in USD, or `None` when unknown.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cost_usd: Option<f64>,
    },
    /// Stream was aborted by the user. A terminal, expected outcome: the
    /// client should not surface it as an error or offer a retry.
    Aborted,
    /// Stream emitted an error. The HTTP response is always 200 for SSE; the
    /// [`ErrorCode`] field is the machine-readable contract consumers branch on.
    Error {
        /// Stable, machine-readable error classification.
        code: ErrorCode,
        /// Human-readable error message. Sanitised server-side: it never
        /// contains provider response bodies, API keys, or other secrets.
        message: String,
        /// Whether retrying the same request may succeed. Server-determined:
        /// true only for transient conditions (e.g. upstream 5xx/429/network).
        retryable: bool,
        /// Session the error applies to, when known.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        session_id: Option<uuid::Uuid>,
    },
}

/// Stable, machine-readable classification of a streamed [`StreamEvent::Error`].
///
/// Consumers branch on this code to decide next steps (e.g. prompt to
/// `/connect`, suggest `/compact`, or retry) instead of parsing the free-text
/// `message`. Serialised kebab-case on the wire.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, utoipa::ToSchema,
)]
#[serde(rename_all = "kebab-case")]
pub enum ErrorCode {
    /// No API key is configured for the requested provider (store, config, or
    /// env). Client action: prompt the user to run `/connect`.
    MissingApiKey,
    /// The session referenced by the request does not exist.
    SessionNotFound,
    /// The request itself was invalid (malformed history, replayed message id, ...).
    BadRequest,
    /// The upstream model provider failed (transport error or non-2xx status).
    Upstream,
    /// The model's context window overflowed. Client action: suggest `/compact`.
    ContextOverflow,
    /// A tool call failed during the turn.
    ToolFailed,
    /// Manual compaction could not complete.
    CompactionFailed,
    /// An unexpected internal error.
    Internal,
}

/// A stable single-select choice request.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
pub struct ChoiceRequest {
    /// Stable id for matching a response to this request.
    pub request_id: String,
    /// Short title shown in the modal header.
    pub title: String,
    /// Prompt/question text.
    pub prompt: String,
    /// Options. Their `id` is the semantic answer value.
    pub options: Vec<ChoiceOption>,
    /// Timeout in milliseconds. Timeout resolves as cancelled.
    pub timeout_ms: u64,
}

/// One selectable option in a choice request.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
pub struct ChoiceOption {
    /// Stable semantic id returned in the response.
    pub id: String,
    /// User-facing label.
    pub label: String,
    /// Optional user-facing explanation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// Response for a single-select choice request.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
#[serde(tag = "status", rename_all = "kebab-case")]
pub enum ChoiceResponse {
    /// User selected one option by stable id.
    Selected {
        /// Request being answered.
        request_id: String,
        /// Stable option id.
        option_id: String,
    },
    /// User cancelled, timeout fired, or no interactive client was available.
    Cancelled {
        /// Request being cancelled.
        request_id: String,
        /// Machine-readable reason.
        reason: ChoiceCancelReason,
    },
}

/// Why a choice resolved without a selected option.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, utoipa::ToSchema,
)]
#[serde(rename_all = "kebab-case")]
pub enum ChoiceCancelReason {
    /// User pressed cancel.
    User,
    /// Timeout elapsed.
    Timeout,
    /// No interactive client was attached.
    NonInteractive,
}

/// Client → server answer for a pending choice request.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
pub struct ChoiceResponseRequest {
    /// Session that owns the pending request.
    pub session_id: uuid::Uuid,
    /// User answer or cancellation.
    pub response: ChoiceResponse,
}

impl StreamEvent {
    /// Serialise to a JSON string suitable for an SSE `data:` line.
    pub fn to_sse_data(&self) -> serde_json::Result<String> {
        serde_json::to_string(self)
    }
}

/// Client → server request to stream a chat turn.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
pub struct ChatRequest {
    /// Session this turn belongs to.
    pub session_id: uuid::Uuid,
    /// Model to use.
    pub model: ModelId,
    /// Provider to route through. `None` defaults to OpenCodeGo.
    #[serde(default)]
    pub provider: Option<ProviderId>,
    /// Mode (Build or Plan).
    pub mode: Mode,
    /// Full message history. The last entry is the user's new turn;
    /// earlier entries are persisted history.
    pub messages: Vec<Message>,
}

/// Client → server request for a headless code review.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
#[schema(example = json!({
    "diff": "diff --git a/src/foo.rs b/src/foo.rs\nindex 111..222 100644\n--- a/src/foo.rs\n+++ b/src/foo.rs\n@@ -1,3 +1,4 @@\n pub fn add(a: i32, b: i32) -> i32 {\n     a + b\n }\n+\n+// TODO: fix\n",
    "extra": "focus on error handling"
}))]
pub struct ReviewRequest {
    /// The diff to review, already fetched by the caller.
    pub diff: String,
    /// Extra focus instruction appended to the review prompt.
    #[serde(default)]
    pub extra: Option<String>,
}

/// Result of a headless code review.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
pub struct ReviewResponse {
    /// Review findings in the `review-pr` skill's output format.
    pub findings: String,
}

/// Concatenate all `Text` parts of a message.
pub fn text_of(msg: &Message) -> String {
    msg.parts
        .iter()
        .filter_map(|p| match p {
            MessagePart::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("")
}
