//! Session CRUD. Backed by the in-memory store until the filesystem
//! `SessionStore` wiring lands.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use chrono::{DateTime, Utc};
use mewcode_protocol::{Message, Mode, ModelId};
use serde::{Deserialize, Serialize};

use crate::AppState;
use crate::AppError;

/// In-memory session store.
#[derive(Debug, Default)]
pub struct MemoryStore {
    /// All sessions known to the server.
    pub sessions: Vec<Session>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: uuid::Uuid,
    pub title: String,
    pub model: ModelId,
    pub mode: Mode,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub messages: Vec<Message>,
}

#[derive(Debug, Deserialize)]
pub struct CreateSessionRequest {
    pub title: String,
    pub model: Option<ModelId>,
    pub mode: Option<Mode>,
}

#[derive(Debug, Serialize)]
pub struct SessionSummary {
    pub id: uuid::Uuid,
    pub title: String,
    pub model: ModelId,
    pub mode: Mode,
    pub created_at: DateTime<Utc>,
}

pub async fn list(State(state): State<AppState>) -> Json<Vec<SessionSummary>> {
    let guard = state.store.read().await;
    let out = guard
        .sessions
        .iter()
        .map(|s| SessionSummary {
            id: s.id,
            title: s.title.clone(),
            model: s.model,
            mode: s.mode,
            created_at: s.created_at,
        })
        .collect();
    Json(out)
}

pub async fn get_one(
    State(state): State<AppState>,
    Path(id): Path<uuid::Uuid>,
) -> Result<Json<Session>, AppError> {
    let guard = state.store.read().await;
    let session = guard
        .sessions
        .iter()
        .find(|s| s.id == id)
        .cloned()
        .ok_or(AppError::NotFound)?;
    Ok(Json(session))
}

pub async fn create(
    State(state): State<AppState>,
    Json(body): Json<CreateSessionRequest>,
) -> Result<(StatusCode, Json<Session>), AppError> {
    if body.title.trim().is_empty() {
        return Err(AppError::BadRequest("title is required".into()));
    }
    let now = Utc::now();
    let session = Session {
        id: uuid::Uuid::new_v4(),
        title: body.title,
        model: body.model.unwrap_or_default(),
        mode: body.mode.unwrap_or_default(),
        created_at: now,
        updated_at: now,
        messages: Vec::new(),
    };
    state.store.write().await.sessions.push(session.clone());
    Ok((StatusCode::CREATED, Json(session)))
}
