use crate::{Message, MessagePart, Mode, ModelId, ProviderId};

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
    },
    /// Stream was aborted by the user.
    Aborted,
    /// Stream emitted an error.
    Error {
        /// Human-readable error message.
        message: String,
    },
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
