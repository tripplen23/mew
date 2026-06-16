use std::env;

use mewcode_protocol::ModelId;
use mewcode_protocol::env::OPENCODE_GO_API_KEY;

use crate::error::EngineError;

/// Default base URL of the OpenCode Go API.
pub const DEFAULT_BASE_URL: &str = "https://opencode.ai/zen/go";

/// Env-var name for overriding [`DEFAULT_BASE_URL`].
pub const ENV_BASE_URL: &str = "MEWCODE_ENGINE_BASE_URL";

/// Env-var name for the default model.
pub const ENV_DEFAULT_MODEL: &str = "MEWCODE_DEFAULT_MODEL";

/// Runtime configuration for the engine.
#[derive(Debug, Clone)]
pub struct EngineConfig {
    /// OpenCode Go subscription key.
    pub api_key: String,
    /// Default model used when the client does not specify one.
    pub default_model: ModelId,
    /// Base URL of the OpenCode Go API. Defaults to the production endpoint.
    pub base_url: String,
}

impl EngineConfig {
    /// Load the configuration from process environment.
    ///
    /// Required: `OPENCODE_GO_API_KEY`.
    /// Optional: `MEWCODE_ENGINE_BASE_URL` (defaults to OpenCode Go production).
    pub fn from_env() -> Result<Self, EngineError> {
        let api_key = env::var(OPENCODE_GO_API_KEY)
            .ok()
            .filter(|s| !s.trim().is_empty())
            .ok_or(EngineError::MissingApiKey)?;

        let base_url = env::var(ENV_BASE_URL).unwrap_or_else(|_| DEFAULT_BASE_URL.to_string());

        let default_model = env::var(ENV_DEFAULT_MODEL)
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(ModelId::DEFAULT);

        Ok(Self {
            api_key,
            default_model,
            base_url,
        })
    }
}
