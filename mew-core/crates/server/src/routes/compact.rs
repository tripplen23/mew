//! `POST /sessions/:id/compact` — manually trigger context compaction.

use std::convert::Infallible;

use axum::extract::{Path, State};
use axum::response::sse::Sse;
use futures::stream::Stream;
use mewcode_protocol::StreamEvent;

use crate::AppState;
use crate::services;
use crate::sse::from_channel;

/// `POST /sessions/:id/compact` — manually trigger context compaction.
#[utoipa::path(
    post,
    path = "/sessions/{id}/compact",
    tag = "sessions",
    params(
        ("id" = Uuid, Path, description = "Session identifier"),
    ),
    responses(
        (status = 200, description = "SSE stream of compaction events", body = StreamEvent, content_type = "text/event-stream"),
        (status = 404, description = "Session not found"),
        (status = 500, description = "Compaction failed"),
    ),
)]
pub async fn compact_session(
    State(state): State<AppState>,
    Path(id): Path<uuid::Uuid>,
) -> Sse<impl Stream<Item = Result<axum::response::sse::Event, Infallible>>> {
    let rx = services::compact::start_compaction(state, id).await;
    from_channel(rx)
}
