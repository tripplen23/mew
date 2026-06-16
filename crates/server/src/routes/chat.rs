//! `POST /chat` — accept a `ChatRequest`, stream `StreamEvent`s back as SSE.

use std::sync::Arc;

use axum::Json;
use axum::extract::State;
use axum::response::sse::Sse;
use futures::stream::Stream;
use mewcode_engine::{Harness, skills::SkillRegistry, tools::ToolRegistry};
use mewcode_protocol::event::{ChatRequest, text_of};
use mewcode_protocol::{Message, Role, StreamEvent};
use std::convert::Infallible;

use crate::AppState;
use crate::sse::from_channel;

/// `POST /chat` — stream a chat turn. The response is `text/event-stream`;
/// each `data:` line is a JSON [`StreamEvent`].
#[utoipa::path(
    post,
    path = "/chat",
    tag = "chat",
    request_body = ChatRequest,
    responses(
        (status = 200, description = "SSE stream of StreamEvent", body = StreamEvent, content_type = "text/event-stream"),
    ),
)]
pub async fn chat_stream(
    State(state): State<AppState>,
    Json(req): Json<ChatRequest>,
) -> Sse<impl Stream<Item = Result<axum::response::sse::Event, Infallible>>> {
    let (tx, rx) = tokio::sync::mpsc::channel::<StreamEvent>(64);

    let user_text = text_of(req.messages.last().unwrap_or(&Message {
        id: uuid::Uuid::nil(),
        role: Role::User,
        parts: vec![],
        model: None,
        created_at: chrono::Utc::now(),
    }));

    let skills = Arc::new(SkillRegistry::load_defaults());
    // The tool registry is empty for now; Phase 7+ will populate it from
    // the session's project context, gated by mode.
    let tools = Arc::new(ToolRegistry::new());

    let harness = Harness::new(req.model, req.mode, skills, tools);
    let _ = state; // reserved for future use (e.g. project context)

    tokio::spawn(async move {
        if let Err(e) = harness.run_placeholder(&user_text, tx.clone()).await {
            tracing::error!(error = ?e, "harness error");
            let _ = tx
                .send(StreamEvent::Error {
                    message: e.to_string(),
                })
                .await;
        }
    });

    from_channel(rx)
}
