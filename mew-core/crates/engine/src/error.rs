use thiserror::Error;

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
                // HTTP 400 or 413 with context-related keywords
                (*status == 400 || *status == 413)
                    && (body.contains("context_length")
                        || body.contains("maximum context length")
                        || body.contains("too many tokens")
                        || body.contains("max_tokens")
                        || body.contains("prompt is too long"))
            }
            EngineError::Other(msg) => {
                msg.contains("context_length")
                    || msg.contains("maximum context length")
                    || msg.contains("too many tokens")
            }
            _ => false,
        }
    }
}
