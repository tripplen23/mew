/// A chat message exchanged between client, server, and engine.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
pub struct Message {
    /// Stable identifier for this message.
    pub id: uuid::Uuid,
    /// Who produced the message.
    pub role: Role,
    /// Ordered parts.
    pub parts: Vec<MessagePart>,
    /// Provider model id used (assistant messages only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Wall-clock time at which the message was created.
    pub created_at: chrono::DateTime<chrono::Utc>,
}

impl Message {
    /// Build a user message with a freshly generated id.
    pub fn user(parts: Vec<MessagePart>) -> Self {
        Self {
            id: uuid::Uuid::new_v4(),
            role: Role::User,
            parts,
            model: None,
            created_at: chrono::Utc::now(),
        }
    }

    /// Build an assistant message.
    pub fn assistant(parts: Vec<MessagePart>, model: impl Into<String>) -> Self {
        Self {
            id: uuid::Uuid::new_v4(),
            role: Role::Assistant,
            parts,
            model: Some(model.into()),
            created_at: chrono::Utc::now(),
        }
    }
}

/// Who produced a message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    /// Human input.
    User,
    /// Model output.
    Assistant,
    /// Result of a tool invocation.
    Tool,
}

/// The ordered, typed parts of a message. Mirrors the AI SDK's
/// `UIMessage` part model.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum MessagePart {
    /// Plain text.
    Text {
        /// The text content.
        text: String,
    },
    /// The model is requesting a tool invocation.
    ToolCall(ToolCall),
    /// The result of a tool invocation, returned to the model.
    ToolResult(ToolResult),
    /// A file mention produced by the client's `@` picker.
    FileMention {
        /// Path relative to the project root.
        path: String,
    },
}

/// A request from the model to invoke a tool.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
pub struct ToolCall {
    /// Stable id used to match the call with its result.
    pub id: String,
    /// Tool name (e.g. `"readFile"`, `"bash"`).
    pub name: String,
    /// JSON arguments, validated against the tool's input schema.
    pub input: serde_json::Value,
}

/// The result of a tool invocation.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
pub struct ToolResult {
    /// The id of the originating [`ToolCall`].
    pub call_id: String,
    /// The tool name (mirrored for log readability).
    pub name: String,
    /// The tool's output, serialised to JSON.
    pub output: serde_json::Value,
    /// `true` if the tool reported an error.
    #[serde(default)]
    pub is_error: bool,
}
