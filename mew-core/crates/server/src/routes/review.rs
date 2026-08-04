//! `POST /review` — run the `review-pr` skill against a caller-supplied
//! diff and return the findings. The engine path the GitHub App bot will
//! use; the caller (CLI, CI, bot) is responsible for fetching the diff.

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use mewcode_protocol::Mode;
use mewcode_protocol::event::{ChatRequest, ReviewRequest, ReviewResponse, StreamEvent};
use mewcode_protocol::{Message, MessagePart, ModelId};

use crate::AppState;
use crate::services;

/// `POST /review` — review a diff headless.
///
/// Creates a throwaway `Plan` session (read-only tools; the reviewing
/// model cannot modify the working tree), streams a chat turn through
/// the normal harness, and returns the collected findings as JSON.
#[utoipa::path(
    post,
    path = "/review",
    tag = "chat",
    request_body = ReviewRequest,
    responses(
        (status = 200, description = "Review findings", body = ReviewResponse),
        (status = 500, description = "Review failed"),
    ),
)]
pub async fn review(
    State(state): State<AppState>,
    Json(req): Json<ReviewRequest>,
) -> (StatusCode, Json<ReviewResponse>) {
    let session = match state
        .store
        .create_session(crate::store::NewSession {
            title: "mew review".into(),
            model: ModelId::DEFAULT,
            mode: Mode::Plan,
        })
        .await
    {
        Ok(session) => session,
        Err(error) => {
            tracing::error!(%error, "review: failed to create session");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ReviewResponse {
                    findings: format!("failed to start review: {error}"),
                }),
            );
        }
    };

    let mut prompt = format!(
        "Review this diff (attached below) — follow the `review-pr` skill.\n\n```diff\n{}\n```",
        req.diff
    );
    if let Some(extra) = req.extra {
        prompt.push_str(&format!("\n\nExtra focus: {extra}"));
    }

    let chat_req = ChatRequest {
        session_id: session.id,
        model: ModelId::DEFAULT,
        provider: None,
        mode: Mode::Plan,
        messages: vec![Message::user(vec![MessagePart::Text { text: prompt }])],
    };

    let mut rx = services::chat::start_chat_stream(state, chat_req).await;
    let mut findings = String::new();
    while let Some(event) = rx.recv().await {
        match event {
            StreamEvent::TextDelta { delta } => findings.push_str(&delta),
            StreamEvent::Error { message, .. } => {
                tracing::error!(%message, "review: chat turn failed");
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ReviewResponse { findings }),
                );
            }
            _ => {}
        }
    }
    (StatusCode::OK, Json(ReviewResponse { findings }))
}
