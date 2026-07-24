//! `GET/POST /memory` — read and write the global (`"default"`) memory profile.
//!
//! Memory is stored as a single markdown file per profile in the
//! server's data directory under `memories/<profile>.md`.
//!
//! NOTE: this route always operates on the global `"default"` profile via
//! `state.memory`, not the project-scoped profile that `/chat` and
//! `/sessions/{id}/compact` actually read/write via
//! `MemoryStore::for_project`. There is currently no client UI for this
//! route; if one is added, prefer exposing the project-scoped profile (see
//! `routes::chat::chat_stream`) so what the user edits here matches what
//! the model actually sees during a chat turn.

use axum::Json;
use axum::extract::State;
use serde::{Deserialize, Serialize};

use crate::AppState;
use crate::error::AppError;

/// `GET /memory` — return the current memory content.
#[derive(Serialize)]
pub struct MemoryResponse {
    /// Profile name.
    pub profile: String,
    /// Markdown content of the memory file.
    pub content: String,
}

/// `POST /memory` body.
#[derive(Deserialize)]
pub struct MemoryWriteRequest {
    /// New memory content (plain markdown). Overwrites the current file.
    pub content: String,
}

/// `GET /memory`
pub async fn get_memory(State(state): State<AppState>) -> Json<MemoryResponse> {
    let content = state.memory.read();
    Json(MemoryResponse {
        profile: "default".to_string(),
        content,
    })
}

/// `POST /memory`
pub async fn post_memory(
    State(state): State<AppState>,
    Json(req): Json<MemoryWriteRequest>,
) -> Result<Json<MemoryResponse>, AppError> {
    state
        .memory
        .write(&req.content)
        .map_err(|e| AppError::Internal(format!("failed to write memory: {e}")))?;
    let content = state.memory.read();
    Ok(Json(MemoryResponse {
        profile: "default".to_string(),
        content,
    }))
}
