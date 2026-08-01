use crate::credential::validate_key;
use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use mewcode_protocol::credential::{
    ConnectProviderRequest, ConnectProviderResponse, ProviderCredential, ProviderStatus,
};
use mewcode_protocol::{ModelId, ModelKind, ProviderId};
use serde::Serialize;

use crate::AppState;

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
pub async fn list_providers(State(state): State<AppState>) -> Json<Vec<ProviderEntry>> {
    let store = state.credentials.lock().await;
    let entries = [ProviderId::OpenCodeGo, ProviderId::OpenAi]
        .into_iter()
        .map(|provider| {
            let available = store.has(provider);
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

/// `POST /providers/connect` — validate and store an API key.
///
/// Validation happens without holding the credentials lock so other handlers
/// are not blocked during the outbound HTTP call. The lock is only held for
/// the brief insert+save.
#[utoipa::path(
    post,
    path = "/providers/connect",
    tag = "meta",
    request_body = ConnectProviderRequest,
    responses(
        (status = 200, description = "Key validated and stored", body = ConnectProviderResponse),
        (status = 401, description = "Key rejected by the provider. The body still carries the ConnectProviderResponse::InvalidKey payload for backward compatibility.", body = ConnectProviderResponse),
        (status = 500, description = "Key validated but could not be persisted. The body still carries the ConnectProviderResponse::Error payload for backward compatibility.", body = ConnectProviderResponse),
    ),
)]
pub async fn connect_provider(
    State(state): State<AppState>,
    Json(req): Json<ConnectProviderRequest>,
) -> (StatusCode, Json<ConnectProviderResponse>) {
    let ConnectProviderRequest { provider, api_key } = req;

    // Validate without holding the lock.
    let result = validate_key(provider, &api_key).await;

    match result {
        Ok(validated_at) => {
            let credential = ProviderCredential {
                provider,
                api_key: api_key.clone(),
                validated_at: Some(validated_at.clone()),
            };
            // Brief critical section: lock, insert, save, unlock.
            let mut store = state.credentials.lock().await;
            if let Err(e) = store.store(credential) {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ConnectProviderResponse::Error {
                        provider,
                        message: format!("key validated but failed to save: {e}"),
                    }),
                );
            }
            // Release lock (guard dropped here).
            drop(store);
            (
                StatusCode::OK,
                Json(ConnectProviderResponse::Ok {
                    provider,
                    validated_at,
                }),
            )
        }
        Err(reason) => (
            StatusCode::UNAUTHORIZED,
            Json(ConnectProviderResponse::InvalidKey { provider, reason }),
        ),
    }
}

/// `GET /providers/status` — connection status for all providers.
#[utoipa::path(
    get,
    path = "/providers/status",
    tag = "meta",
    responses(
        (status = 200, description = "Provider connection status", body = [ProviderStatus]),
    ),
)]
pub async fn provider_status(State(state): State<AppState>) -> Json<Vec<ProviderStatus>> {
    let store = state.credentials.lock().await;
    Json(store.status())
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
