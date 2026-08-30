use crate::credential::{ValidationError, validate_key};
use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use mewcode_protocol::credential::{
    ConnectProviderRequest, ConnectProviderResponse, ProviderCredential, ProviderStatus,
};
use mewcode_protocol::{ProviderEntry, ProviderId, SUPPORTED_PROVIDERS};

use crate::AppState;

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
    use crate::provider_catalog::{
        ANTHROPIC_MODELS_URL, DEEPSEEK_MODELS_URL, MODELS_DEV_URL, OPENAI_MODELS_URL,
        OPENCODE_ZEN_MODELS_URL, OPENROUTER_MODELS_URL,
    };

    let config = crate::services::chat::build_engine_config(&state).await;
    let opencode_url = format!("{}/models", config.base_url.trim_end_matches('/'));
    let opencode_key = (!config.api_key.trim().is_empty()).then_some(config.api_key.as_str());
    let openai_key = config
        .openai_api_key
        .as_deref()
        .filter(|key| !key.trim().is_empty());
    let zen_key = config
        .opencode_zen_api_key
        .as_deref()
        .filter(|key| !key.trim().is_empty());
    let anthropic_key = config
        .anthropic_api_key
        .as_deref()
        .filter(|key| !key.trim().is_empty());
    let deepseek_key = config
        .deepseek_api_key
        .as_deref()
        .filter(|key| !key.trim().is_empty());
    let openrouter_key = config
        .openrouter_api_key
        .as_deref()
        .filter(|key| !key.trim().is_empty());
    let (opencode, zen, openai, anthropic, deepseek, openrouter) = tokio::join!(
        discover_configured(ProviderId::OpenCodeGo, &opencode_url, opencode_key),
        discover_zen_configured(zen_key, OPENCODE_ZEN_MODELS_URL, MODELS_DEV_URL),
        discover_configured(ProviderId::OpenAi, OPENAI_MODELS_URL, openai_key),
        discover_configured(ProviderId::Anthropic, ANTHROPIC_MODELS_URL, anthropic_key),
        discover_configured(ProviderId::DeepSeek, DEEPSEEK_MODELS_URL, deepseek_key),
        discover_configured(
            ProviderId::OpenRouter,
            OPENROUTER_MODELS_URL,
            openrouter_key,
        ),
    );
    let mut results = std::collections::HashMap::from([
        (ProviderId::OpenCodeGo, opencode),
        (ProviderId::OpenCodeZen, zen),
        (ProviderId::OpenAi, openai),
        (ProviderId::Anthropic, anthropic),
        (ProviderId::DeepSeek, deepseek),
        (ProviderId::OpenRouter, openrouter),
    ]);
    let entries = SUPPORTED_PROVIDERS
        .iter()
        .map(|descriptor| {
            let result = results
                .remove(&descriptor.id)
                .unwrap_or_else(|| Err("model catalog unavailable".to_owned()));
            let available = provider_available(descriptor.id, &config);
            let (models, error) = match result {
                Ok(models) if available => (models, None),
                Ok(_) => (Vec::new(), None),
                Err(error) if available => (Vec::new(), Some(error)),
                Err(_) => (Vec::new(), None),
            };
            ProviderEntry {
                id: descriptor.id,
                display_name: descriptor.display_name.to_owned(),
                available,
                models,
                error,
            }
        })
        .collect();
    Json(entries)
}

async fn discover_configured(
    provider: ProviderId,
    url: &str,
    api_key: Option<&str>,
) -> Result<Vec<mewcode_protocol::ModelEntry>, String> {
    match api_key {
        Some(key) => crate::provider_catalog::discover_models(provider, url, key).await,
        None => Ok(Vec::new()),
    }
}

async fn discover_zen_configured(
    api_key: Option<&str>,
    live_url: &str,
    metadata_url: &str,
) -> Result<Vec<mewcode_protocol::ModelEntry>, String> {
    match api_key {
        Some(key) => {
            crate::provider_catalog::discover_zen_models(live_url, metadata_url, key).await
        }
        None => Ok(Vec::new()),
    }
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
        (status = 502, description = "Provider validation was unavailable.", body = ConnectProviderResponse),
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
        Err(ValidationError::InvalidKey(reason)) => (
            StatusCode::UNAUTHORIZED,
            Json(ConnectProviderResponse::InvalidKey { provider, reason }),
        ),
        Err(ValidationError::Unavailable(message)) => (
            StatusCode::BAD_GATEWAY,
            Json(ConnectProviderResponse::Error { provider, message }),
        ),
    }
}

fn provider_available(provider: ProviderId, config: &mewcode_engine::EngineConfig) -> bool {
    match provider {
        ProviderId::OpenCodeGo => !config.api_key.trim().is_empty(),
        ProviderId::OpenCodeZen => config
            .opencode_zen_api_key
            .as_deref()
            .is_some_and(|key| !key.trim().is_empty()),
        ProviderId::OpenAi => config
            .openai_api_key
            .as_deref()
            .is_some_and(|key| !key.trim().is_empty()),
        ProviderId::Anthropic => config
            .anthropic_api_key
            .as_deref()
            .is_some_and(|key| !key.trim().is_empty()),
        ProviderId::DeepSeek => config
            .deepseek_api_key
            .as_deref()
            .is_some_and(|key| !key.trim().is_empty()),
        ProviderId::OpenRouter => config
            .openrouter_api_key
            .as_deref()
            .is_some_and(|key| !key.trim().is_empty()),
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
    let config = crate::services::chat::build_engine_config(&state).await;
    let store = state.credentials.lock().await;
    Json(
        SUPPORTED_PROVIDERS
            .iter()
            .map(|descriptor| ProviderStatus {
                provider: descriptor.id,
                connected: provider_available(descriptor.id, &config),
                validated_at: store
                    .credentials
                    .get(&descriptor.id)
                    .and_then(|credential| credential.validated_at.clone()),
            })
            .collect(),
    )
}
