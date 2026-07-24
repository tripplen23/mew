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
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
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
    },
    /// Stream was aborted by the user.
    Aborted,
    /// Stream emitted an error.
    Error {
        /// Human-readable error message.
        message: String,
    },
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
