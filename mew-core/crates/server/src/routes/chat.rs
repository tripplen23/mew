//! `POST /chat` — accept a `ChatRequest`, stream `StreamEvent`s back as SSE.

use std::convert::Infallible;

use axum::Json;
use axum::extract::State;
use axum::response::sse::Sse;
use futures::stream::Stream;
use mewcode_protocol::StreamEvent;
use mewcode_protocol::event::ChatRequest;

use crate::AppState;
use crate::services;
use crate::sse::from_channel;

/// `POST /chat` — stream a chat turn. The response is `text/event-stream`;
/// each `data:` line is a JSON [`StreamEvent`]. The HTTP status is always
/// `200`: turn failures are delivered in-band as a single terminal
/// [`StreamEvent::Error`] carrying a stable [`mewcode_protocol::event::ErrorCode`] and a retryable flag.
#[utoipa::path(
    post,
    path = "/chat",
    tag = "chat",
    request_body = ChatRequest,
    responses(
        (status = 200, description = "SSE stream of StreamEvent. Always 200; failures arrive in-band as a terminal StreamEvent::Error.", body = StreamEvent, content_type = "text/event-stream"),
    ),
)]
pub async fn chat_stream(
    State(state): State<AppState>,
    Json(req): Json<ChatRequest>,
) -> Sse<impl Stream<Item = Result<axum::response::sse::Event, Infallible>>> {
    let rx = services::chat::start_chat_stream(state, req).await;
    from_channel(rx)
}
