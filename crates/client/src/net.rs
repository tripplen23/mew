//! Network client (server API).

use mewcode_protocol::routes::{HEALTH, MODELS};
use mewcode_protocol::{ModelId, ModelKind};
use serde::Deserialize;

/// HTTP client wrapper.
#[derive(Debug, Clone)]
pub struct ApiClient {
    base_url: String,
    inner: reqwest::Client,
}

/// Response payload of `GET /health`.
#[derive(Debug, Clone, Deserialize)]
pub struct HealthResponse {
    /// `true` when the server is up.
    pub ok: bool,
    /// Service name.
    pub service: String,
    /// Service version.
    pub version: String,
}

/// One entry in the model registry returned by `GET /models`.
#[derive(Debug, Clone, Deserialize)]
pub struct ModelEntry {
    /// Provider-side model id.
    pub id: String,
    /// Human-friendly display name.
    pub display_name: String,
    /// Which OpenCode Go endpoint serves the model.
    pub kind: ModelKind,
}

impl ApiClient {
    /// Build a new client. `base_url` should not have a trailing slash.
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            inner: reqwest::Client::new(),
        }
    }

    /// `GET /health`
    pub async fn health(&self) -> reqwest::Result<HealthResponse> {
        self.inner
            .get(format!("{}{}", self.base_url, HEALTH))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await
    }

    /// `GET /models`
    pub async fn models(&self) -> reqwest::Result<Vec<ModelEntry>> {
        self.inner
            .get(format!("{}{}", self.base_url, MODELS))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await
    }

    /// Resolve a model id string into the registry.
    pub fn model_id(&self, id: &str) -> Option<ModelId> {
        id.parse().ok()
    }

    /// Base URL the client is configured against.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }
}
