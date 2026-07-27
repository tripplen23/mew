//! mewcode server: [axum](https://docs.rs/axum/latest/axum/) app with
//! session CRUD, model registry, and SSE chat.

#![forbid(unsafe_code)]

pub mod config;
pub mod error;
pub mod openapi;
pub mod routes;
pub mod services;
pub mod sse;
pub mod store;

pub use config::ServerConfig;
pub use error::AppError;

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;

use axum::Router;
use mewcode_engine::context::MemoryStore;
use mewcode_engine::credential::CredentialStore;
use mewcode_engine::tools::ApprovalBroker;
use mewcode_protocol::routes::{
    CHAT, CHOICES, HEALTH, MEMORY_GET, MEMORY_POST, PROVIDERS, PROVIDER_CONNECT,
    PROVIDER_STATUS, SESSION_BY_ID, SESSION_COMPACT, SESSIONS, SKILLS, STORAGE_STATUS,
};
use tokio::sync::{Mutex, RwLock};
use tower_http::trace::TraceLayer;
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

use crate::openapi::ApiDoc;
use crate::store::{SessionStore, StoreError};

#[doc(hidden)]
pub fn should_remove_operation_lock(error: &StoreError) -> bool {
    matches!(error, StoreError::NotFound)
}

/// Shared application state.
///
/// The session backend is chosen at startup and held behind a shared
/// `Arc<dyn SessionStore>`, so cloning the state is just an `Arc` clone.
#[derive(Clone)]
pub struct AppState {
    /// Server config.
    pub config: ServerConfig,
    /// Session store backend (filesystem in production, in-memory in tests).
    pub store: Arc<dyn SessionStore>,
    /// Memory fact store.
    pub memory: MemoryStore,
    /// Provider credential store.
    pub credentials: Arc<tokio::sync::Mutex<CredentialStore>>,
    /// In-memory pending choice/approval broker.
    pub approvals: ApprovalBroker,
    /// Per-session accumulated token usage for compaction decisions.
    pub session_tokens: Arc<RwLock<HashMap<uuid::Uuid, u64>>>,
    /// Serializes mutations for each known session independently.
    pub session_operations: Arc<Mutex<HashMap<uuid::Uuid, Arc<Mutex<()>>>>>,
}

impl AppState {
    /// Construct a new state over the given session store and memory store.
    pub fn new(config: ServerConfig, store: Arc<dyn SessionStore>, memory: MemoryStore) -> Self {
        let credentials = CredentialStore::load().unwrap_or_default();
        Self {
            config,
            store,
            memory,
            credentials: Arc::new(tokio::sync::Mutex::new(credentials)),
            approvals: ApprovalBroker::default(),
            session_tokens: Arc::new(RwLock::new(HashMap::new())),
            session_operations: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub async fn session_operation_lock(&self, session_id: uuid::Uuid) -> Arc<Mutex<()>> {
        let mut operations = self.session_operations.lock().await;
        Arc::clone(
            operations
                .entry(session_id)
                .or_insert_with(|| Arc::new(Mutex::new(()))),
        )
    }

    pub async fn existing_session_operation_lock(
        &self,
        session_id: uuid::Uuid,
    ) -> Result<Arc<Mutex<()>>, crate::store::StoreError> {
        let operation_lock = self.session_operation_lock(session_id).await;
        let operation_guard = operation_lock.lock().await;
        if let Err(error) = self.store.get_session(session_id).await {
            if should_remove_operation_lock(&error) {
                self.remove_session_operation_lock(session_id, &operation_lock)
                    .await;
            } else {
                self.remove_uncontended_session_operation_lock(session_id, &operation_lock)
                    .await;
            }
            return Err(error);
        }
        drop(operation_guard);
        Ok(operation_lock)
    }

    pub async fn remove_uncontended_session_operation_lock(
        &self,
        session_id: uuid::Uuid,
        operation_lock: &Arc<Mutex<()>>,
    ) {
        let mut operations = self.session_operations.lock().await;
        if operations
            .get(&session_id)
            .is_some_and(|current| Arc::ptr_eq(current, operation_lock))
            && Arc::strong_count(operation_lock) == 2
        {
            operations.remove(&session_id);
        }
    }

    pub async fn remove_session_operation_lock(
        &self,
        session_id: uuid::Uuid,
        operation_lock: &Arc<Mutex<()>>,
    ) {
        let mut operations = self.session_operations.lock().await;
        if operations
            .get(&session_id)
            .is_some_and(|current| Arc::ptr_eq(current, operation_lock))
        {
            operations.remove(&session_id);
        }
    }
}

/// Build the axum app.
pub fn build_app(state: AppState) -> Router {
    Router::new()
        .route(HEALTH, axum::routing::get(routes::health::health))
        .route(
            PROVIDERS,
            axum::routing::get(routes::providers::list_providers),
        )
        .route(
            PROVIDER_CONNECT,
            axum::routing::post(routes::providers::connect_provider),
        )
        .route(
            PROVIDER_STATUS,
            axum::routing::get(routes::providers::provider_status),
        )
        .route(SKILLS, axum::routing::get(routes::skills::list_skills))
        .route(
            SESSIONS,
            axum::routing::get(routes::sessions::list).post(routes::sessions::create),
        )
        .route(
            SESSION_BY_ID,
            axum::routing::get(routes::sessions::get_one)
                .patch(routes::sessions::patch)
                .delete(routes::sessions::delete),
        )
        .route(
            SESSION_COMPACT,
            axum::routing::post(routes::compact::compact_session),
        )
        .route(CHAT, axum::routing::post(routes::chat::chat_stream))
        .route(CHOICES, axum::routing::post(routes::choices::respond))
        .route(STORAGE_STATUS, axum::routing::get(routes::storage::status))
        .route(MEMORY_GET, axum::routing::get(routes::memory::get_memory))
        .route(
            MEMORY_POST,
            axum::routing::post(routes::memory::post_memory),
        )
        .with_state(state)
        .layer(TraceLayer::new_for_http())
        .merge(SwaggerUi::new("/swagger-ui").url("/api-docs/openapi.json", ApiDoc::openapi()))
}

/// Run the server, blocking the current task.
pub async fn serve(addr: SocketAddr, state: AppState) -> anyhow::Result<()> {
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!(%addr, "mewcode server listening");
    axum::serve(listener, build_app(state)).await?;
    Ok(())
}
