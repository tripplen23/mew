use axum::Json;
use mewcode_protocol::{ModelId, ModelKind, ProviderId};
use serde::Serialize;

/// One model entry in a provider's model list.
#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct ModelEntry {
    /// Provider-side model id
    pub id: String,
    /// Human-friendly display name for the model picker.
    pub display_name: &'static str,
    /// Which provider serves this model.
    pub provider: ProviderId,
    /// Which endpoint protocol this model speaks.
    pub kind: ModelKind,
}

/// One provider entry in the provider registry.
#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct ProviderEntry {
    /// Provider id used on the wire.
    pub id: ProviderId,
    /// Human-friendly provider name.
    pub display_name: String,
    /// Whether this provider can currently be used.
    pub available: bool,
    /// Available models for this provider.
    pub models: Vec<ModelEntry>,
}

/// `GET /providers` — list providers and the models each currently exposes.
#[utoipa::path(
    get,
    path = "/providers",
    tag = "meta",
    responses(
        (status = 200, description = "Provider registry", body = [ProviderEntry]),
    ),
)]
pub async fn list_providers() -> Json<Vec<ProviderEntry>> {
    let entries = [ProviderId::OpenCodeGo, ProviderId::OpenAi]
        .into_iter()
        .map(|provider| {
            let available = provider_available(provider);
            let models = if available {
                models_for_provider(provider)
            } else {
                Vec::new()
            };
            ProviderEntry {
                id: provider,
                display_name: provider.to_string(),
                available,
                models,
            }
        })
        .collect();
    Json(entries)
}

fn models_for_provider(provider: ProviderId) -> Vec<ModelEntry> {
    ModelId::ALL
        .iter()
        .copied()
        .filter(|m| m.provider() == provider)
        .map(model_entry)
        .collect()
}

fn model_entry(model: ModelId) -> ModelEntry {
    ModelEntry {
        id: model.as_str().to_string(),
        display_name: model.display_name(),
        provider: model.provider(),
        kind: model.kind(),
    }
}

fn provider_available(provider: ProviderId) -> bool {
    match provider {
        ProviderId::OpenCodeGo => true,
        ProviderId::OpenAi => std::env::var("OPENAI_API_KEY")
            .ok()
            .is_some_and(|k| !k.trim().is_empty()),
    }
}
