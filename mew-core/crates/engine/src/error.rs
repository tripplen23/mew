use thiserror::Error;

/// All errors the engine can produce.
#[derive(Debug, Error)]
pub enum EngineError {
    /// No `OPENCODE_GO_API_KEY` was provided.
    #[error("OPENCODE_GO_API_KEY is not set")]
    MissingApiKey,

    /// The HTTP request to OpenCode Go failed.
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

    /// JSON (de)serialisation failed.
    #[error("serialization error: {0}")]
    Serde(#[from] serde_json::Error),

    /// Catch-all.
    #[error("{0}")]
    Other(String),
}
