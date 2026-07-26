//! Provider routing. Selects the right Rig client + credentials for the
//! model's provider, so the agent layer can ask for a provider by model alone.
//!
//! Thin wrappers over [rig-core](https://docs.rs/rig-core/latest/rig_core/)'
//! [Anthropic](https://docs.rs/rig-core/latest/rig_core/providers/anthropic/index.html)
//! and [OpenAI](https://docs.rs/rig-core/latest/rig_core/providers/openai/index.html)
//! provider clients.

use mewcode_protocol::{ModelId, ModelKind, ProviderId};

use crate::config::EngineConfig;
use crate::error::EngineError;

/// A provider client capable of issuing chat-completion requests.
#[derive(Clone)]
pub enum Provider {
    /// OpenCode Go Anthropic-compatible endpoint (`/v1/messages`).
    Anthropic(AnthropicProvider),
    /// OpenCode Go OpenAI-compatible endpoint (`/v1/chat/completions`).
    OpenCodeGo(OpenAiProvider),
    /// Native OpenAI API at `api.openai.com/v1`.
    OpenAi(OpenAiProvider),
}

impl Provider {
    /// Build a provider for the given model, reading credentials from config.
    pub fn for_model(model: ModelId, cfg: &EngineConfig) -> Result<Self, EngineError> {
        let (api_key, base_url) = match model.provider() {
            ProviderId::OpenCodeGo => (cfg.api_key.as_str(), cfg.base_url.as_str()),
            ProviderId::OpenAi => {
                let key = cfg
                    .openai_api_key
                    .as_deref()
                    .ok_or(EngineError::MissingNativeApiKey("OPENAI_API_KEY"))?;
                (key, "https://api.openai.com/v1")
            }
        };

        let provider = match model.kind() {
            ModelKind::AnthropicMessages => {
                Provider::Anthropic(AnthropicProvider::new(api_key, base_url))
            }
            ModelKind::OpenCodeGo => Provider::OpenCodeGo(OpenAiProvider::new(api_key, base_url)),
            ModelKind::OpenAi => Provider::OpenAi(OpenAiProvider::new(api_key, base_url)),
        };
        Ok(provider)
    }
}

/// Anthropic-compatible provider. Wraps rig-core's
/// [`anthropic::Client`](https://docs.rs/rig-core/latest/rig_core/providers/anthropic/client/index.html#typealias.Client).
#[derive(Clone)]
pub struct AnthropicProvider {
    client: rig_core::providers::anthropic::Client,
}

impl AnthropicProvider {
    /// Build a new provider.
    pub fn new(api_key: &str, base_url: &str) -> Self {
        let client = rig_core::providers::anthropic::Client::builder()
            .api_key(api_key)
            .base_url(base_url)
            .build()
            .expect("anthropic client build is infallible");
        Self { client }
    }

    /// Borrow the underlying rig client.
    pub fn client(&self) -> &rig_core::providers::anthropic::Client {
        &self.client
    }
}

/// OpenAI-compatible provider. Wraps rig-core's chat-completions client.
#[derive(Clone)]
pub struct OpenAiProvider {
    client: rig_core::providers::openai::CompletionsClient,
}

impl OpenAiProvider {
    /// Build a new provider.
    pub fn new(api_key: &str, base_url: &str) -> Self {
        let client = rig_core::providers::openai::CompletionsClient::builder()
            .api_key(api_key)
            .base_url(base_url)
            .build()
            .expect("openai client build is infallible");
        Self { client }
    }

    /// Borrow the underlying rig client.
    pub fn client(&self) -> &rig_core::providers::openai::CompletionsClient {
        &self.client
    }
}
