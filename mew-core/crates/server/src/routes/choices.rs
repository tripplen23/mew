use axum::Json;
use axum::extract::State;
use mewcode_protocol::event::ChoiceResponseRequest;

use crate::{AppError, AppState};

#[utoipa::path(
    post,
    path = "/choices",
    tag = "chat",
    request_body = ChoiceResponseRequest,
    responses(
        (status = 204, description = "Choice response accepted"),
        (status = 404, description = "No matching pending choice", body = crate::openapi::ErrorResponse),
    ),
)]
/// Answer a pending interactive choice for a session.
pub async fn respond(
    State(state): State<AppState>,
    Json(req): Json<ChoiceResponseRequest>,
) -> Result<axum::http::StatusCode, AppError> {
    if state.approvals.answer(req.session_id, req.response) {
        Ok(axum::http::StatusCode::NO_CONTENT)
    } else {
        Err(AppError::NotFound)
    }
}
