use std::sync::Arc;

use axum::body::Body;
use axum::http::Request;
use http_body_util::BodyExt;
use mewcode_engine::context::MemoryStore as FactStore;
use mewcode_protocol::routes::PROVIDER_STATUS;
use mewcode_protocol::{ProviderId, ProviderStatus};
use mewcode_server::store::memory::MemoryStore;
use mewcode_server::{AppState, ServerConfig, build_app};
use tower::ServiceExt;

#[tokio::test]
async fn provider_status_honors_server_config_credentials() {
    let state = AppState::new(
        ServerConfig {
            host: "127.0.0.1".into(),
            port: 0,
            opencode_go_api_key: None,
            opencode_zen_api_key: None,
            openai_api_key: None,
            anthropic_api_key: None,
            openrouter_api_key: Some("config-openrouter-key".into()),
            default_model: None,
            log: "off".into(),
            skills: Default::default(),
            github: Default::default(),
            mcp: Default::default(),
        },
        Arc::new(MemoryStore::default()),
        FactStore::new(std::env::temp_dir().join(uuid::Uuid::new_v4().to_string())),
    );
    state.credentials.lock().await.credentials.clear();

    let response = build_app(state)
        .oneshot(
            Request::builder()
                .uri(PROVIDER_STATUS)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let statuses: Vec<ProviderStatus> = serde_json::from_slice(&body).unwrap();
    let openrouter = statuses
        .iter()
        .find(|status| status.provider == ProviderId::OpenRouter)
        .unwrap();
    assert!(openrouter.connected);
}
