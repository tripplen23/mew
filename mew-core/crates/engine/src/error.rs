use thiserror::Error;

use mewcode_protocol::event::ErrorCode;

/// All errors the engine can produce.
#[derive(Debug, Error)]
pub enum EngineError {
    /// No `OPENCODE_GO_API_KEY` was provided.
    #[error("OPENCODE_GO_API_KEY is not set")]
    MissingApiKey,

    /// A native provider's API key was not found.
    #[error("{0} is not set")]
    MissingNativeApiKey(&'static str),

    /// The HTTP request upstream failed.
    #[error("upstream error: {0}")]
    Upstream(#[from] reqwest::Error),

    /// The provider returned a non-2xx response.
    #[error("upstream returned {status}: {body}")]
    UpstreamStatus {
        /// HTTP status code.
        status: u16,
        /// Response body (truncated).
        body: String,
    },

    /// A tool emitted a structured error.
    #[error("tool error in {tool}: {message}")]
    Tool {
        /// Tool that errored.
        tool: String,
        /// Error message.
        message: String,
    },

    /// The stream was aborted by the user.
    #[error("aborted")]
    Aborted,

    /// The provider rejected the request due to context length overflow.
    #[error("context overflow: {0}")]
    ContextOverflow(String),

    /// JSON (de)serialisation failed.
    #[error("serialization error: {0}")]
    Serde(#[from] serde_json::Error),

    /// Catch-all.
    #[error("{0}")]
    Other(String),
}

impl EngineError {
    /// Check if this error represents a context overflow from the provider.
    ///
    /// Detects common patterns from OpenAI, Anthropic, and other providers
    /// when the request exceeds the model's context limit.
    pub fn is_context_overflow(&self) -> bool {
        match self {
            EngineError::ContextOverflow(_) => true,
            EngineError::UpstreamStatus { status, body } => {
                (*status == 400 || *status == 413) && contains_context_overflow(body)
            }
            EngineError::Other(msg) => contains_context_overflow(msg),
            _ => false,
        }
    }
}

fn contains_context_overflow(msg: &str) -> bool {
    let lower = msg.to_lowercase();
    lower.contains("context_length")
        || lower.contains("maximum context length")
        || lower.contains("too many tokens")
        || lower.contains("max_tokens")
        || lower.contains("prompt is too long")
        || lower.contains("context length exceeded")
}

/// Split an engine error into its streamed [`StreamEvent::Error`] fields.
///
/// `message` is a sanitised, user-actionable summary — the caller logs the
/// raw error, which may carry provider bodies or keys. `retryable` is true
/// only for transient conditions (upstream 5xx/429/network), so the client
/// can offer a retry without reimplementing engine semantics.
/// `EngineError::Aborted` never reaches here: callers intercept it and emit
/// [`StreamEvent::Aborted`]; if it slips through it degrades to `Internal`.
pub fn engine_error_parts(error: &EngineError) -> (ErrorCode, String, bool) {
    match error {
        EngineError::MissingApiKey => (
            ErrorCode::MissingApiKey,
            "no OpenCode Go API key is configured".into(),
            false,
        ),
        EngineError::MissingNativeApiKey(name) => (
            ErrorCode::MissingApiKey,
            format!("{name} is not set"),
            false,
        ),
        EngineError::Upstream(_) => (ErrorCode::Upstream, "upstream provider error".into(), true),
        EngineError::UpstreamStatus { status, .. } => (
            ErrorCode::Upstream,
            format!("provider returned HTTP {status}"),
            retryable_status(*status),
        ),
        EngineError::Tool { tool, .. } => (
            ErrorCode::ToolFailed,
            format!("tool `{tool}` failed"),
            false,
        ),
        EngineError::Aborted => (ErrorCode::Internal, "internal error".into(), false),
        EngineError::ContextOverflow(_) => (
            ErrorCode::ContextOverflow,
            "model context limit reached".into(),
            false,
        ),
        EngineError::Serde(_) | EngineError::Other(_) => {
            (ErrorCode::Internal, "internal error".into(), false)
        }
    }
}

/// True for HTTP statuses that commonly indicate a transient upstream fault
/// worth retrying unchanged.
#[doc(hidden)]
pub fn retryable_status(status: u16) -> bool {
    matches!(status, 408 | 409 | 425 | 429 | 500 | 502 | 503 | 504 | 529)
}
