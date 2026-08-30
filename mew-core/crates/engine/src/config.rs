use std::env;

use mewcode_protocol::ModelId;
use mewcode_protocol::env::{
    ANTHROPIC_API_KEY, DEEPSEEK_API_KEY, OPENAI_API_KEY, OPENCODE_GO_API_KEY, OPENCODE_ZEN_API_KEY,
    OPENROUTER_API_KEY,
};

use crate::error::EngineError;

/// Default versioned base URL of the OpenCode Go API.
pub const DEFAULT_BASE_URL: &str = "https://opencode.ai/zen/go/v1";

/// Env-var name for overriding [`DEFAULT_BASE_URL`].
pub const ENV_BASE_URL: &str = "MEWCODE_ENGINE_BASE_URL";

/// Env-var name for the default model.
pub const ENV_DEFAULT_MODEL: &str = "MEWCODE_DEFAULT_MODEL";

/// Runtime configuration for the engine.
#[derive(Debug, Clone)]
pub struct EngineConfig {
    /// OpenCode Go subscription key.
    pub api_key: String,
    /// OpenCode Zen API key.
    pub opencode_zen_api_key: Option<String>,
    /// Native OpenAI API key.
    pub openai_api_key: Option<String>,
    /// Base URL for the native OpenAI API.
    pub openai_base_url: Option<String>,
    /// Native Anthropic API key.
    pub anthropic_api_key: Option<String>,
    /// Native DeepSeek API key.
    pub deepseek_api_key: Option<String>,
    /// OpenRouter API key.
    pub openrouter_api_key: Option<String>,
    /// Default model used when the client does not specify one.
    pub default_model: ModelId,
    /// Base URL of the OpenCode Go API. Defaults to the production endpoint.
    pub base_url: String,
}

impl EngineConfig {
    /// Load the configuration from process environment.
    ///
    /// Provider credentials are optional here and validated against the
    /// selected model by [`crate::agent::Provider::for_model`].
    pub fn from_env() -> Result<Self, EngineError> {
        let api_key = env::var(OPENCODE_GO_API_KEY)
            .ok()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_default();

        let base_url = env::var(ENV_BASE_URL).unwrap_or_else(|_| DEFAULT_BASE_URL.to_string());

        let default_model = env::var(ENV_DEFAULT_MODEL)
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(ModelId::DEFAULT);

        let opencode_zen_api_key = env::var(OPENCODE_ZEN_API_KEY)
            .ok()
            .filter(|s| !s.trim().is_empty());

        let openai_api_key = env::var(OPENAI_API_KEY)
            .ok()
            .filter(|s| !s.trim().is_empty());

        let anthropic_api_key = env::var(ANTHROPIC_API_KEY)
            .ok()
            .filter(|s| !s.trim().is_empty());

        let deepseek_api_key = env::var(DEEPSEEK_API_KEY)
            .ok()
            .filter(|s| !s.trim().is_empty());

        let openrouter_api_key = env::var(OPENROUTER_API_KEY)
            .ok()
            .filter(|s| !s.trim().is_empty());

        Ok(Self {
            api_key,
            opencode_zen_api_key,
            openai_api_key,
            openai_base_url: None,
            anthropic_api_key,
            deepseek_api_key,
            openrouter_api_key,
            default_model,
            base_url,
        })
    }
}
