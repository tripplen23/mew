//! Server liveness after a failed turn.
//!
//! Drives the real axum app (`build_app`) in-process via `tower`'s `oneshot`.
//! A turn is forced to fail deterministically by clearing `OPENCODE_GO_API_KEY`,
//! so the harness fails at the credential boundary (`MissingApiKey`) before any
//! provider is built or request issued. We then assert:
//!   1. `POST /chat` yields exactly one `StreamEvent::Error` and nothing after
//!      it (no `Start`, no `Finish`).
//!   2. A SECOND request still gets served — proving the process did not
//!      terminate after the failed turn.
//!
//! The whole scenario lives in ONE test because `OPENCODE_GO_API_KEY`
//! is process-global; a single serial test avoids any env race with itself.

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use chrono::Utc;
use http_body_util::BodyExt;
use mewcode_engine::memory::MemoryStore as FactStore;
use mewcode_protocol::env::OPENCODE_GO_API_KEY;
use mewcode_protocol::event::ChatRequest;
use mewcode_protocol::routes::{CHAT, HEALTH};
use mewcode_protocol::{Message, MessagePart, Mode, ModelId, Role, StreamEvent};
use mewcode_server::store::memory::MemoryStore;
use mewcode_server::{AppState, ServerConfig, build_app};
use tower::ServiceExt;
use uuid::Uuid;

/// A throwaway server config; the chat handler resolves its credential from the
/// process env, not from here, so the key value is irrelevant to the failure.
fn test_config() -> ServerConfig {
    ServerConfig {
        host: "127.0.0.1".into(),
        port: 0,
        opencode_go_api_key: "test-key".into(),
        openai_api_key: None,
        default_model: None,
        log: "off".into(),
        skills: Default::default(),
    }
}

/// Build a fresh app backed by an empty in-memory store.
fn app() -> axum::Router {
    let fact_store = FactStore::new(std::env::temp_dir().join(uuid::Uuid::new_v4().to_string()));
    build_app(AppState::new(
        test_config(),
        Arc::new(MemoryStore::default()),
        fact_store,
    ))
}

/// A minimal `ChatRequest` carrying a single user message, so the turn gets
/// past the "no user message" guard and fails at the credential boundary.
fn chat_request() -> ChatRequest {
    ChatRequest {
        session_id: Uuid::new_v4(),
        model: ModelId::default(),
        provider: None,
        mode: Mode::default(),
        messages: vec![Message {
            id: Uuid::new_v4(),
            role: Role::User,
            parts: vec![MessagePart::Text {
                text: "hello".into(),
            }],
            model: None,
            created_at: Utc::now(),
        }],
    }
}

/// `POST /chat` with the given request body.
fn post_chat(req: &ChatRequest) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(CHAT)
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::to_string(req).expect("request serialises"),
        ))
        .expect("request should build")
}

/// Send a request through the app and return `(status, body_bytes)`.
async fn send(app: axum::Router, req: Request<Body>) -> (StatusCode, Vec<u8>) {
    let resp = app.oneshot(req).await.expect("router should respond");
    let status = resp.status();
    let bytes = resp
        .into_body()
        .collect()
        .await
        .expect("body should collect")
        .to_bytes()
        .to_vec();
    (status, bytes)
}

/// Parse an SSE body into its ordered `StreamEvent`s by decoding each `data:`
/// line's JSON payload.
fn parse_sse(bytes: &[u8]) -> Vec<StreamEvent> {
    let text = std::str::from_utf8(bytes).expect("SSE body is utf-8");
    text.lines()
        .filter_map(|line| line.strip_prefix("data:"))
        .map(|payload| {
            serde_json::from_str::<StreamEvent>(payload.trim())
                .expect("each data line is a StreamEvent")
        })
        .collect()
}

#[tokio::test]
async fn failed_turn_emits_one_error_and_server_keeps_serving() {
    // Force a deterministic turn failure: with no API key, the harness fails at
    // the credential boundary before building any provider. Save/restore so we
    // don't perturb other tests sharing this process.
    // SAFETY: single-threaded section of this test; no other thread reads the
    // var concurrently within this test binary.
    let prior = std::env::var(OPENCODE_GO_API_KEY).ok();
    unsafe {
        std::env::remove_var(OPENCODE_GO_API_KEY);
    }

    // (1) A failing POST /chat: the HTTP response itself succeeds (SSE opens),
    // but the streamed body carries exactly one Error and nothing else.
    let (status, body) = send(app(), post_chat(&chat_request())).await;
    assert_eq!(status, StatusCode::OK, "SSE response should open with 200");

    let events = parse_sse(&body);
    assert_eq!(
        events.len(),
        1,
        "a failed turn must produce exactly one event, got: {events:?}"
    );
    assert!(
        matches!(events[0], StreamEvent::Error { .. }),
        "the single event must be Error, got: {:?}",
        events[0]
    );
    // No success/terminal events may follow the error.
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, StreamEvent::Start { .. } | StreamEvent::Finish { .. })),
        "no Start/Finish may accompany a failed turn"
    );

    // (2) The server is still alive: a subsequent request is served normally.
    // A second /chat fails the same way (still exactly one Error), and an
    // unrelated /health probe returns 200 — proving the process survived.
    let (status, body) = send(app(), post_chat(&chat_request())).await;
    assert_eq!(status, StatusCode::OK, "server still serves /chat");
    let events = parse_sse(&body);
    assert_eq!(
        events.len(),
        1,
        "the second failed turn is also exactly one event"
    );
    assert!(matches!(events[0], StreamEvent::Error { .. }));

    let (status, _) = send(
        app(),
        Request::builder()
            .uri(HEALTH)
            .body(Body::empty())
            .expect("health request builds"),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "server stays live after failures");

    // Restore the prior environment.
    unsafe {
        match prior {
            Some(v) => std::env::set_var(OPENCODE_GO_API_KEY, v),
            None => std::env::remove_var(OPENCODE_GO_API_KEY),
        }
    }
}
