//! Provider routing. Selects the right Rig client + credentials for the
//! model's provider, so the agent layer can ask for a provider by model alone.
//!
//! Thin wrappers over [rig-core](https://docs.rs/rig-core/latest/rig_core/)'
//! [Anthropic](https://docs.rs/rig-core/latest/rig_core/providers/anthropic/index.html)
//! and [OpenAI](https://docs.rs/rig-core/latest/rig_core/providers/openai/index.html)
//! provider clients.

use mewcode_protocol::model::provider_supports_kind;
use mewcode_protocol::{ModelKind, ModelRef, ProviderId};

use crate::config::EngineConfig;
use crate::error::EngineError;

/// OpenCode Zen API base shared by all supported endpoint protocols.
pub const OPENCODE_ZEN_BASE_URL: &str = "https://opencode.ai/zen/v1";
/// Native Anthropic API base.
pub const ANTHROPIC_BASE_URL: &str = "https://api.anthropic.com/v1";

/// A provider client capable of issuing model requests.
#[derive(Clone)]
pub enum Provider {
    /// OpenCode Go Anthropic-compatible endpoint (`/v1/messages`).
    Anthropic(AnthropicProvider),
    /// OpenCode Go OpenAI-compatible endpoint (`/v1/chat/completions`).
    OpenCodeGo(OpenAiProvider),
    /// OpenCode Zen OpenAI-compatible endpoint (`/v1/chat/completions`).
    OpenCodeZen(OpenAiProvider),
    /// OpenAI-compatible Responses endpoint (`/v1/responses`).
    OpenAiResponses(OpenAiResponsesProvider),
    /// Native OpenAI API at `api.openai.com/v1`.
    OpenAi(OpenAiProvider),
    /// Native DeepSeek API at `api.deepseek.com`.
    DeepSeek(OpenAiProvider),
    /// OpenRouter API.
    OpenRouter(OpenAiProvider),
}

impl Provider {
    /// Build a provider using the model's legacy endpoint default.
    pub fn for_model(model: &ModelRef, cfg: &EngineConfig) -> Result<Self, EngineError> {
        Self::for_model_kind(model, None, cfg)
    }

    /// Build a provider using a persisted transport snapshot when present.
    pub fn for_model_kind(
        model: &ModelRef,
        model_kind: Option<ModelKind>,
        cfg: &EngineConfig,
    ) -> Result<Self, EngineError> {
        let provider_id = model.provider();
        let kind = model_kind.unwrap_or_else(|| model.kind());
        if !provider_supports_kind(provider_id, kind) {
            return Err(EngineError::UnsupportedProviderTransport {
                provider: provider_id,
                kind,
            });
        }
        let (api_key, base_url) = match provider_id {
            ProviderId::OpenCodeGo => {
                if cfg.api_key.trim().is_empty() {
                    return Err(EngineError::MissingApiKey);
                }
                (cfg.api_key.as_str(), cfg.base_url.as_str())
            }
            ProviderId::OpenCodeZen => {
                let key = cfg
                    .opencode_zen_api_key
                    .as_deref()
                    .filter(|key| !key.trim().is_empty())
                    .ok_or(EngineError::MissingNativeApiKey("OPENCODE_ZEN_API_KEY"))?;
                (key, OPENCODE_ZEN_BASE_URL)
            }
            ProviderId::OpenAi => {
                let key = cfg
                    .openai_api_key
                    .as_deref()
                    .filter(|key| !key.trim().is_empty())
                    .ok_or(EngineError::MissingNativeApiKey("OPENAI_API_KEY"))?;
                (
                    key,
                    cfg.openai_base_url
                        .as_deref()
                        .unwrap_or("https://api.openai.com/v1"),
                )
            }
            ProviderId::Anthropic => {
                let key = cfg
                    .anthropic_api_key
                    .as_deref()
                    .filter(|key| !key.trim().is_empty())
                    .ok_or(EngineError::MissingNativeApiKey("ANTHROPIC_API_KEY"))?;
                (key, ANTHROPIC_BASE_URL)
            }
            ProviderId::DeepSeek => {
                let key = cfg
                    .deepseek_api_key
                    .as_deref()
                    .filter(|key| !key.trim().is_empty())
                    .ok_or(EngineError::MissingNativeApiKey("DEEPSEEK_API_KEY"))?;
                (key, "https://api.deepseek.com")
            }
            ProviderId::OpenRouter => {
                let key = cfg
                    .openrouter_api_key
                    .as_deref()
                    .filter(|key| !key.trim().is_empty())
                    .ok_or(EngineError::MissingNativeApiKey("OPENROUTER_API_KEY"))?;
                (key, "https://openrouter.ai/api/v1")
            }
        };

        Ok(match kind {
            ModelKind::AnthropicMessages => {
                Provider::Anthropic(AnthropicProvider::new(api_key, base_url))
            }
            ModelKind::OpenCodeGo => Provider::OpenCodeGo(OpenAiProvider::new(api_key, base_url)),
            ModelKind::OpenCodeZen => Provider::OpenCodeZen(OpenAiProvider::new(api_key, base_url)),
            ModelKind::OpenAiResponses => {
                Provider::OpenAiResponses(OpenAiResponsesProvider::new(api_key, base_url))
            }
            ModelKind::OpenAi => Provider::OpenAi(OpenAiProvider::new(api_key, base_url)),
            ModelKind::DeepSeek => Provider::DeepSeek(OpenAiProvider::new(api_key, base_url)),
            ModelKind::OpenRouter => Provider::OpenRouter(OpenAiProvider::new(api_key, base_url)),
        })
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

/// OpenAI Responses provider backed by Rig's default OpenAI client.
#[derive(Clone)]
pub struct OpenAiResponsesProvider {
    client: rig_core::providers::openai::Client,
}

impl OpenAiResponsesProvider {
    /// Build a Responses client for an OpenAI-compatible base URL.
    pub fn new(api_key: &str, base_url: &str) -> Self {
        let client = rig_core::providers::openai::Client::builder()
            .api_key(api_key)
            .base_url(base_url)
            .build()
            .expect("openai responses client build is infallible");
        Self { client }
    }

    /// Borrow the underlying Rig client.
    pub fn client(&self) -> &rig_core::providers::openai::Client {
        &self.client
    }
}

impl std::fmt::Debug for Provider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Anthropic(_) => "Anthropic",
            Self::OpenCodeGo(_) => "OpenCodeGo",
            Self::OpenCodeZen(_) => "OpenCodeZen",
            Self::OpenAiResponses(_) => "OpenAiResponses",
            Self::OpenAi(_) => "OpenAi",
            Self::DeepSeek(_) => "DeepSeek",
            Self::OpenRouter(_) => "OpenRouter",
        })
    }
}
