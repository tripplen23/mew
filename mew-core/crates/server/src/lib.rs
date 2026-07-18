//! mewcode server: [axum](https://docs.rs/axum/latest/axum/) app with
//! session CRUD, model registry, and SSE chat.

#![forbid(unsafe_code)]

pub mod config;
pub mod error;
pub mod openapi;
pub mod routes;
pub mod sse;
pub mod store;

pub use config::ServerConfig;
pub use error::AppError;

use std::net::SocketAddr;
use std::sync::Arc;

use axum::Router;
use mewcode_engine::memory::MemoryStore;
use mewcode_protocol::routes::{
    CHAT, HEALTH, MEMORY_GET, MEMORY_POST, PROVIDERS, SESSION_BY_ID, SESSIONS, SKILLS,
    STORAGE_STATUS,
};
use tower_http::trace::TraceLayer;
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

use crate::openapi::ApiDoc;
use crate::store::SessionStore;

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
}

impl AppState {
    /// Construct a new state over the given session store and memory store.
    pub fn new(config: ServerConfig, store: Arc<dyn SessionStore>, memory: MemoryStore) -> Self {
        Self {
            config,
            store,
            memory,
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
        .route(CHAT, axum::routing::post(routes::chat::chat_stream))
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
