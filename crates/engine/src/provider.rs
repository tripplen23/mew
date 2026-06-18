//! OpenCode Go provider routing. Hides which endpoint (`/v1/messages`
//! vs `/v1/chat/completions`) a given [`ModelId`] needs so the rest of
//! the engine can ask for a provider by model alone.
//!
//! Thin wrappers over [rig-core](https://docs.rs/rig-core/latest/rig_core/)'
//! [Anthropic](https://docs.rs/rig-core/latest/rig_core/providers/anthropic/index.html)
//! and [OpenAI](https://docs.rs/rig-core/latest/rig_core/providers/openai/index.html)
//! provider clients. The rest of the engine drives Rig's
//! [`Agent`](https://docs.rs/rig-core/latest/rig_core/agent/struct.Agent.html)
//! abstraction; the provider arms here are only a routing shim that selects the
//! right Rig client for the model kind.

use mewcode_protocol::{ModelId, ModelKind};
use rig_core::client::CompletionClient;
use rig_core::completion::Prompt;

use crate::error::EngineError;

/// A provider client capable of issuing chat-completion requests to OpenCode Go.
#[derive(Clone)]
pub enum Provider {
    /// Anthropic-compatible provider, hits `/v1/messages`.
    Anthropic(AnthropicProvider),
    /// OpenAI-compatible provider, hits `/v1/chat/completions`.
    OpenAi(OpenAiProvider),
}

impl Provider {
    /// Build a provider for the given model.
    pub fn for_model(model: ModelId, api_key: &str, base_url: &str) -> Result<Self, EngineError> {
        let provider = match model.kind() {
            ModelKind::AnthropicMessages => {
                Provider::Anthropic(AnthropicProvider::new(api_key, base_url))
            }
            ModelKind::OpenAiChatCompletions => {
                Provider::OpenAi(OpenAiProvider::new(api_key, base_url))
            }
        };
        Ok(provider)
    }

    /// Build and invoke a Rig agent for one user prompt.
    ///
    /// The two provider arms stay explicit because they use different OpenCode
    /// Go wire protocols, but both go through Rig's `Agent` abstraction. That
    /// keeps the harness ready for the next phase: tools, skills, and streaming
    /// can attach to the agent builder/request instead of a low-level completion
    /// request.
    pub async fn invoke_agent(
        &self,
        model_id: &str,
        system_prompt: String,
        user_text: String,
        max_tokens: u64,
        max_turns: usize,
    ) -> Result<String, EngineError> {
        let reply = match self {
            Provider::Anthropic(p) => {
                let agent = p
                    .client()
                    .agent(model_id)
                    .name("mewcode")
                    .preamble(&system_prompt)
                    .max_tokens(max_tokens)
                    .default_max_turns(max_turns)
                    .build();

                agent
                    .prompt(user_text)
                    .await
                    .map_err(|e| EngineError::Other(e.to_string()))?
            }
            Provider::OpenAi(p) => {
                let agent = p
                    .client()
                    .agent(model_id)
                    .name("mewcode")
                    .preamble(&system_prompt)
                    .max_tokens(max_tokens)
                    .default_max_turns(max_turns)
                    .build();

                agent
                    .prompt(user_text)
                    .await
                    .map_err(|e| EngineError::Other(e.to_string()))?
            }
        };
        Ok(reply)
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
