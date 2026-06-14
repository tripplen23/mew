//! Session CRUD. Delegates to the active [`SessionStore`] backend held in
//! [`AppState`].

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use mewcode_protocol::{Mode, ModelId};
use serde::Deserialize;

use crate::store::{NewSession, Session, SessionSummary};
use crate::AppError;
use crate::AppState;

/// Request body for `POST /sessions`.
#[derive(Debug, Deserialize)]
pub struct CreateSessionRequest {
    /// Human-readable title (required, non-empty after trimming).
    pub title: String,
    /// Model to use; defaults when omitted.
    pub model: Option<ModelId>,
    /// Interaction mode; defaults when omitted.
    pub mode: Option<Mode>,
}

pub async fn list(State(state): State<AppState>) -> Result<Json<Vec<SessionSummary>>, AppError> {
    let summaries = state.store.list_sessions().await?;
    Ok(Json(summaries))
}

pub async fn get_one(
    State(state): State<AppState>,
    Path(id): Path<uuid::Uuid>,
) -> Result<Json<Session>, AppError> {
    let session = state.store.get_session(id).await?;
    Ok(Json(session))
}

pub async fn create(
    State(state): State<AppState>,
    Json(body): Json<CreateSessionRequest>,
) -> Result<(StatusCode, Json<Session>), AppError> {
    let title = body.title.trim();
    if title.is_empty() {
        return Err(AppError::BadRequest("title is required".into()));
    }
    let session = state
        .store
        .create_session(NewSession {
            title: title.to_owned(),
            model: body.model.unwrap_or_default(),
            mode: body.mode.unwrap_or_default(),
        })
        .await?;
    Ok((StatusCode::CREATED, Json(session)))
}

/// Delete a session by id. Returns `204 No Content` on success, or `404` when
/// the session does not exist (the store surfaces [`StoreError::NotFound`]).
pub async fn delete(
    State(state): State<AppState>,
    Path(id): Path<uuid::Uuid>,
) -> Result<StatusCode, AppError> {
    state.store.delete_session(id).await?;
    Ok(StatusCode::NO_CONTENT)
}
