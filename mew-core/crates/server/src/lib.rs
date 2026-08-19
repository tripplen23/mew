//! mewcode server: [axum](https://docs.rs/axum/latest/axum/) app with
//! session CRUD, model registry, and SSE chat.

#![forbid(unsafe_code)]

pub mod config;
pub mod credential;
pub mod error;
pub mod github;
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
use std::sync::atomic::{AtomicBool, Ordering};

use axum::Json;
use axum::Router;
use axum::extract::Request;
use axum::http::{StatusCode, header};
use axum::middleware::{Next, from_fn};
use axum::response::{IntoResponse, Response};
use http_body_util::BodyExt;
use mewcode_engine::context::MemoryStore;
use mewcode_engine::tools::ApprovalBroker;
use mewcode_protocol::routes::{
    CHAT, CHOICES, GITHUB_WEBHOOK, HEALTH, MEMORY_GET, MEMORY_POST, PROVIDER_CONNECT,
    PROVIDER_STATUS, PROVIDERS, REVIEW, SESSION_ABORT, SESSION_BY_ID, SESSION_COMPACT, SESSIONS,
    SKILLS, STORAGE_STATUS,
};
use serde_json::json;
use tokio::sync::{Mutex, RwLock};
use tower_http::trace::TraceLayer;
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

use crate::credential::CredentialStore;
use crate::github::GithubClient;
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
    /// Per-session abort flags: set by `POST /sessions/{id}/abort`, polled by
    /// the chat worker so a turn can be cancelled mid-flight.
    pub aborts: Arc<Mutex<HashMap<uuid::Uuid, Arc<AtomicBool>>>>,
    /// Reusable GitHub App client with token cache; `None` disables the webhook.
    pub github_client: Option<GithubClient>,
    /// Recently seen delivery IDs, so redeliveries don't double-review.
    pub webhook_deliveries: Arc<Mutex<HashMap<String, std::time::Instant>>>,
}

impl AppState {
    /// Construct a new state over the given session store and memory store.
    pub fn new(config: ServerConfig, store: Arc<dyn SessionStore>, memory: MemoryStore) -> Self {
        let credentials = CredentialStore::load().unwrap_or_default();
        let github_client = GithubClient::from_config(&config.github);
        Self {
            config,
            store,
            memory,
            credentials: Arc::new(tokio::sync::Mutex::new(credentials)),
            approvals: ApprovalBroker::default(),
            session_tokens: Arc::new(RwLock::new(HashMap::new())),
            session_operations: Arc::new(Mutex::new(HashMap::new())),
            aborts: Arc::new(Mutex::new(HashMap::new())),
            github_client,
            webhook_deliveries: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Register the chat turn's abort flag for `session_id` and return it.
    /// The worker polls the flag; `request_abort` sets it.
    pub async fn register_abort(&self, session_id: uuid::Uuid) -> Arc<AtomicBool> {
        let mut aborts = self.aborts.lock().await;
        let flag = Arc::new(AtomicBool::new(false));
        aborts.insert(session_id, flag.clone());
        flag
    }

    /// Raise the abort flag for a session with a live turn. Returns `false`
    /// when no turn is registered (abort is a no-op).
    pub async fn request_abort(&self, session_id: uuid::Uuid) -> bool {
        let aborts = self.aborts.lock().await;
        aborts.get(&session_id).is_some_and(|flag| {
            flag.store(true, Ordering::Release);
            true
        })
    }

    /// Remove the abort flag once the turn has ended (either way).
    pub async fn unregister_abort(&self, session_id: uuid::Uuid) {
        let mut aborts = self.aborts.lock().await;
        aborts.remove(&session_id);
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

/// Middleware: rewrite non-JSON error responses to the canonical
/// `{"error": "<message>"}` shape used by [`AppError`]. axum's default
/// rejections — malformed JSON body, bad path param, unmatched route,
/// wrong method — produce plain-text bodies; this keeps every error
/// response consistent without touching success or already-JSON bodies.
async fn jsonify_errors(request: Request, next: Next) -> Response {
    let response = next.run(request).await;
    let status = response.status();
    if status.is_success() || status.is_informational() {
        return response;
    }
    let already_json = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|ct| ct.starts_with("application/json"));
    if already_json {
        return response;
    }
    let (mut parts, body) = response.into_parts();
    let bytes = body.collect().await.unwrap_or_default().to_bytes();
    let message = String::from_utf8_lossy(&bytes).trim().to_string();
    let message = if message.is_empty() {
        status.canonical_reason().unwrap_or("error").to_owned()
    } else {
        message
    };
    // Preserve original headers (`Allow` on 405, `WWW-Authenticate` on 401,
    // ...); only the body format is normalized. Content-Type/Length belong to
    // the new JSON body.
    parts.headers.remove(header::CONTENT_TYPE);
    parts.headers.remove(header::CONTENT_LENGTH);
    let mut response = (parts.status, Json(json!({ "error": message }))).into_response();
    response.headers_mut().extend(parts.headers);
    response
}

/// Fallback for unmatched routes. Registered via `.fallback()` (which axum
/// wraps in the same middleware layers as ordinary routes), so unknown paths
/// get the canonical JSON `{"error": ...}` body instead of plain text.
async fn not_found() -> Response {
    (StatusCode::NOT_FOUND, Json(json!({ "error": "not found" }))).into_response()
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
        .route(SESSION_ABORT, axum::routing::post(routes::sessions::abort))
        .route(CHAT, axum::routing::post(routes::chat::chat_stream))
        .route(REVIEW, axum::routing::post(routes::review::review))
        .route(
            GITHUB_WEBHOOK,
            axum::routing::post(routes::webhook::webhook),
        )
        .route(CHOICES, axum::routing::post(routes::choices::respond))
        .route(STORAGE_STATUS, axum::routing::get(routes::storage::status))
        .route(MEMORY_GET, axum::routing::get(routes::memory::get_memory))
        .route(
            MEMORY_POST,
            axum::routing::post(routes::memory::post_memory),
        )
        .with_state(state)
        .fallback(not_found)
        .layer(TraceLayer::new_for_http())
        .layer(from_fn(jsonify_errors))
        .merge(SwaggerUi::new("/swagger-ui").url("/api-docs/openapi.json", ApiDoc::openapi()))
}

/// Run the server, blocking the current task.
pub async fn serve(addr: SocketAddr, state: AppState) -> anyhow::Result<()> {
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!(%addr, "mewcode server listening");
    axum::serve(listener, build_app(state)).await?;
    Ok(())
}
