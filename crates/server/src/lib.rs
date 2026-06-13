//! mewcode server: axum app with session CRUD, model registry, and SSE chat.

#![forbid(unsafe_code)]

pub mod config;
pub mod db;
pub mod error;
pub mod routes;
pub mod sse;

pub use config::ServerConfig;
pub use error::AppError;

use std::net::SocketAddr;
use std::sync::Arc;

use axum::Router;
use mewcode_protocol::routes::{CHAT, HEALTH, MODELS, SESSIONS, SESSION_BY_ID};
use tokio::sync::RwLock;
use tower_http::trace::TraceLayer;

use crate::routes::sessions::MemoryStore;

/// Shared application state.
#[derive(Clone)]
pub struct AppState {
    /// Server config.
    pub config: ServerConfig,
    /// Database pool. `None` in in-memory mode.
    pub pool: Option<sqlx::PgPool>,
    /// In-memory session store, used while the sqlx layer is being built.
    pub store: Arc<RwLock<MemoryStore>>,
}

impl AppState {
    /// Construct a new state with an empty in-memory store.
    pub fn new(config: ServerConfig, pool: Option<sqlx::PgPool>) -> Self {
        Self {
            config,
            pool,
            store: Arc::new(RwLock::new(MemoryStore::default())),
        }
    }
}

/// Build the axum app.
pub fn build_app(state: AppState) -> Router {
    Router::new()
        .route(HEALTH, axum::routing::get(routes::health::health))
        .route(MODELS, axum::routing::get(routes::models::list_models))
        .route(
            SESSIONS,
            axum::routing::get(routes::sessions::list).post(routes::sessions::create),
        )
        .route(SESSION_BY_ID, axum::routing::get(routes::sessions::get_one))
        .route(CHAT, axum::routing::post(routes::chat::chat_stream))
        .with_state(state)
        .layer(TraceLayer::new_for_http())
}

/// Run the server, blocking the current task.
pub async fn serve(addr: SocketAddr, state: AppState) -> anyhow::Result<()> {
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!(%addr, "mewcode server listening");
    axum::serve(listener, build_app(state)).await?;
    Ok(())
}
