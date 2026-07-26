//! Session CRUD. Delegates to the active [`crate::store::SessionStore`] backend
//! held in [`AppState`].

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use mewcode_protocol::{Mode, ModelId};
use serde::Deserialize;

use crate::AppError;
use crate::AppState;
use crate::store::{NewSession, Session, SessionPatch, SessionSummary};

/// Request body for `POST /sessions`.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct CreateSessionRequest {
    /// Human-readable title (required, non-empty after trimming).
    pub title: String,
    /// Model to use; defaults when omitted.
    pub model: Option<ModelId>,
    /// Interaction mode; defaults when omitted.
    pub mode: Option<Mode>,
}

/// Request body for `PATCH /sessions/{id}`.
#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub struct UpdateSessionRequest {
    /// New title. Omit to keep the current title.
    pub title: Option<String>,
    /// New model. Omit to keep the current model.
    pub model: Option<ModelId>,
    /// New mode. Omit to keep the current mode.
    pub mode: Option<Mode>,
}

/// `GET /sessions` — list every session as a summary (no message history),
/// newest-first.
#[utoipa::path(
    get,
    path = "/sessions",
    tag = "sessions",
    responses(
        (status = 200, description = "Session summaries (newest first)", body = [SessionSummary]),
        (status = 500, description = "Internal error", body = crate::openapi::ErrorResponse),
    ),
)]
pub async fn list(State(state): State<AppState>) -> Result<Json<Vec<SessionSummary>>, AppError> {
    let summaries = state.store.list_sessions().await?;
    Ok(Json(summaries))
}

/// `GET /sessions/{id}` — fetch one session, hydrated with its full message
/// history.
#[utoipa::path(
    get,
    path = "/sessions/{id}",
    tag = "sessions",
    params(
        ("id" = uuid::Uuid, Path, description = "Session id"),
    ),
    responses(
        (status = 200, description = "Session with message history", body = Session),
        (status = 404, description = "Session not found", body = crate::openapi::ErrorResponse),
        (status = 500, description = "Internal error", body = crate::openapi::ErrorResponse),
    ),
)]
pub async fn get_one(
    State(state): State<AppState>,
    Path(id): Path<uuid::Uuid>,
) -> Result<Json<Session>, AppError> {
    let session = state.store.get_session(id).await?;
    Ok(Json(session))
}

/// `POST /sessions` — create a session. Title is required and trimmed;
/// empty/whitespace titles are rejected with `400`.
#[utoipa::path(
    post,
    path = "/sessions",
    tag = "sessions",
    request_body = CreateSessionRequest,
    responses(
        (status = 201, description = "Session created", body = Session),
        (status = 400, description = "Empty or whitespace title", body = crate::openapi::ErrorResponse),
        (status = 500, description = "Internal error", body = crate::openapi::ErrorResponse),
    ),
)]
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

/// `DELETE /sessions/{id}` — delete a session and its message history.
/// Returns:
/// `204 No Content` on success,
/// `404` when the session does not exist.
#[utoipa::path(
    delete,
    path = "/sessions/{id}",
    tag = "sessions",
    params(
        ("id" = uuid::Uuid, Path, description = "Session id"),
    ),
    responses(
        (status = 204, description = "Session deleted"),
        (status = 404, description = "Session not found", body = crate::openapi::ErrorResponse),
        (status = 500, description = "Internal error", body = crate::openapi::ErrorResponse),
    ),
)]
pub async fn delete(
    State(state): State<AppState>,
    Path(id): Path<uuid::Uuid>,
) -> Result<StatusCode, AppError> {
    let operation_lock = state.existing_session_operation_lock(id).await?;
    let _operation_guard = operation_lock.lock().await;
    state.store.delete_session(id).await?;
    state.session_tokens.write().await.remove(&id);
    state
        .remove_session_operation_lock(id, &operation_lock)
        .await;
    Ok(StatusCode::NO_CONTENT)
}

/// `PATCH /sessions/{id}` — apply a partial update (title, model, mode).
/// Fields set to `null` in the body are left unchanged. Returns the
/// refreshed session.
#[utoipa::path(
    patch,
    path = "/sessions/{id}",
    tag = "sessions",
    params(
        ("id" = uuid::Uuid, Path, description = "Session id"),
    ),
    request_body = UpdateSessionRequest,
    responses(
        (status = 200, description = "Session updated", body = Session),
        (status = 400, description = "Empty or whitespace title", body = crate::openapi::ErrorResponse),
        (status = 404, description = "Session not found", body = crate::openapi::ErrorResponse),
        (status = 500, description = "Internal error", body = crate::openapi::ErrorResponse),
    ),
)]
pub async fn patch(
    State(state): State<AppState>,
    Path(id): Path<uuid::Uuid>,
    Json(body): Json<UpdateSessionRequest>,
) -> Result<Json<Session>, AppError> {
    let operation_lock = state.existing_session_operation_lock(id).await?;
    let _operation_guard = operation_lock.lock().await;
    let session = state
        .store
        .patch_session(
            id,
            SessionPatch {
                title: body.title,
                model: body.model,
                mode: body.mode,
                ..Default::default()
            },
        )
        .await?;
    Ok(Json(session))
}
