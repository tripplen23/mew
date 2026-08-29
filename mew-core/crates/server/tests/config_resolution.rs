//! Server-side engine-config resolution exercised through
//! `build_engine_config`, the single config builder shared by `/chat` and
//! `/sessions/:id/compact`.
//!
//! Lives in an integration test because it mutates process-global env vars,
//! which the lib crate's `#![forbid(unsafe_code)]` forbids in unit tests.
//! The cases are serial inside one test so the env restores deterministically.

use std::sync::Arc;

use mewcode_engine::context::MemoryStore as FactStore;
use mewcode_protocol::ProviderId;
use mewcode_protocol::credential::ProviderCredential;
use mewcode_protocol::env::{OPENAI_API_KEY, OPENCODE_GO_API_KEY};
use mewcode_server::services::chat::build_engine_config;
use mewcode_server::store::memory::MemoryStore;
use mewcode_server::{AppState, ServerConfig};

fn config() -> ServerConfig {
    ServerConfig {
        host: "127.0.0.1".into(),
        port: 0,
        opencode_go_api_key: None,
        openai_api_key: None,
        default_model: None,
        log: "off".into(),
        skills: Default::default(),
        github: Default::default(),
        mcp: Default::default(),
    }
}

fn app_with(config: ServerConfig) -> AppState {
    AppState::new(
        config,
        Arc::new(MemoryStore::default()),
        FactStore::new(std::env::temp_dir().join(uuid::Uuid::new_v4().to_string())),
    )
}

/// Clear the credential store so a real `~/.config/mew/credentials.yaml`
/// (if present) can't leak into the assertions.
async fn clear_credentials(state: &AppState) {
    let mut store = state.credentials.lock().await;
    store.credentials.clear();
}

/// Resolution priority is store -> config field -> env; a blank key
/// is left empty so the engine's `Provider::for_model` rejects it as missing.
#[tokio::test]
async fn build_engine_config_resolution_priority() {
    let env_keys = [
        OPENCODE_GO_API_KEY,
        OPENAI_API_KEY,
        mewcode_engine::config::ENV_BASE_URL,
    ];
    let prior: Vec<Option<String>> = env_keys.iter().map(|k| std::env::var(k).ok()).collect();

    // Stored credential wins over config and env.
    {
        let state = app_with(ServerConfig {
            opencode_go_api_key: Some("cfg-key".into()),
            ..config()
        });
        clear_credentials(&state).await;
        state.credentials.lock().await.credentials.insert(
            ProviderId::OpenCodeGo,
            ProviderCredential::new(ProviderId::OpenCodeGo, "store-key".into()),
        );
        unsafe {
            std::env::set_var(OPENCODE_GO_API_KEY, "env-key");
        }
        let cfg = build_engine_config(&state).await;
        assert_eq!(cfg.api_key, "store-key");
    }

    // Config field beats a raw env var.
    {
        let state = app_with(ServerConfig {
            opencode_go_api_key: Some("cfg-key".into()),
            ..config()
        });
        clear_credentials(&state).await;
        unsafe {
            std::env::set_var(OPENCODE_GO_API_KEY, "env-key");
        }
        let cfg = build_engine_config(&state).await;
        assert_eq!(cfg.api_key, "cfg-key");
    }

    // Env is the fallback when neither store nor config has a key.
    {
        let state = app_with(config());
        clear_credentials(&state).await;
        unsafe {
            std::env::set_var(OPENCODE_GO_API_KEY, "env-key");
        }
        let cfg = build_engine_config(&state).await;
        assert_eq!(cfg.api_key, "env-key");
    }

    // No credentials anywhere -> empty key, which Provider::for_model rejects.
    {
        let state = app_with(config());
        clear_credentials(&state).await;
        unsafe {
            std::env::remove_var(OPENCODE_GO_API_KEY);
        }
        let cfg = build_engine_config(&state).await;
        assert_eq!(cfg.api_key, "");
    }

    // Custom MEWCODE_ENGINE_BASE_URL propagates; openai key resolves too.
    {
        let state = app_with(ServerConfig {
            openai_api_key: Some("cfg-openai".into()),
            ..config()
        });
        clear_credentials(&state).await;
        unsafe {
            std::env::set_var(
                mewcode_engine::config::ENV_BASE_URL,
                "http://custom:8080/v1",
            );
        }
        let cfg = build_engine_config(&state).await;
        assert_eq!(cfg.base_url, "http://custom:8080/v1");
        assert_eq!(cfg.openai_api_key.as_deref(), Some("cfg-openai"));
    }

    for (key, value) in env_keys.iter().zip(prior) {
        unsafe {
            match value {
                Some(v) => std::env::set_var(key, v),
                None => std::env::remove_var(key),
            }
        }
    }
}
