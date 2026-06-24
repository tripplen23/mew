//! Chat turns are persisted to the session store (so reopening a session shows
//! its history).
//!
//! The user's message is appended up front, before the turn runs, so it
//! survives even a failed turn. We exploit that here for a deterministic check:
//! with `OPENCODE_GO_API_KEY` cleared the turn fails at the credential boundary
//! (no live LLM needed), yet the user message must still be in the store
//! afterwards. Before this fix the chat route streamed without ever writing to
//! the store, so reopened sessions were always empty.

use std::sync::Arc;

use axum::body::Body;
use axum::http::Request;
use http_body_util::BodyExt;
use mewcode_engine::memory::MemoryStore as FactStore;
use mewcode_protocol::env::OPENCODE_GO_API_KEY;
use mewcode_protocol::event::ChatRequest;
use mewcode_protocol::routes::CHAT;
use mewcode_protocol::{Message, MessagePart, Mode, ModelId, Role};
use mewcode_server::store::memory::MemoryStore;
use mewcode_server::store::{NewSession, SessionStore};
use mewcode_server::{AppState, ServerConfig, build_app};
use tower::ServiceExt;
use uuid::Uuid;

fn test_config() -> ServerConfig {
    ServerConfig {
        host: "127.0.0.1".into(),
        port: 0,
        opencode_go_api_key: "test-key".into(),
        default_model: None,
        log: "off".into(),
        skills: Default::default(),
    }
}

fn user_turn(session_id: Uuid, text: &str) -> ChatRequest {
    ChatRequest {
        session_id,
        model: ModelId::default(),
        mode: Mode::default(),
        messages: vec![Message {
            id: Uuid::new_v4(),
            role: Role::User,
            parts: vec![MessagePart::Text { text: text.into() }],
            model: None,
            created_at: chrono::Utc::now(),
        }],
    }
}

#[tokio::test]
async fn chat_persists_the_user_message_to_the_session() {
    // Force the turn to fail fast so no live LLM is needed; the user message is
    // appended before the turn runs, so it must persist regardless.
    // SAFETY: this test binary contains a single test, so nothing else reads
    // the env var concurrently.
    let prior = std::env::var(OPENCODE_GO_API_KEY).ok();
    unsafe {
        std::env::remove_var(OPENCODE_GO_API_KEY);
    }

    let store = Arc::new(MemoryStore::default());
    let fact_store = FactStore::new(std::env::temp_dir().join(uuid::Uuid::new_v4().to_string()));
    let state = AppState::new(test_config(), store.clone(), fact_store);

    // A real session to attach the turn to.
    let session = store
        .create_session(NewSession {
            title: "persist me".into(),
            model: ModelId::default(),
            mode: Mode::default(),
        })
        .await
        .expect("create session");

    // Drive one (failing) chat turn through the real app.
    let resp = build_app(state.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(CHAT)
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_string(&user_turn(session.id, "remember this")).unwrap(),
                ))
                .unwrap(),
        )
        .await
        .expect("router responds");
    // Drain the SSE body so the streaming/persistence tasks run to completion.
    let _ = resp.into_body().collect().await.expect("collect body");

    // The user message is now stored, so reopening the session shows it.
    let reopened = store.get_session(session.id).await.expect("get session");
    let texts: Vec<&str> = reopened
        .messages
        .iter()
        .filter(|m| m.role == Role::User)
        .flat_map(|m| {
            m.parts.iter().filter_map(|p| match p {
                MessagePart::Text { text } => Some(text.as_str()),
                _ => None,
            })
        })
        .collect();
    assert!(
        texts.contains(&"remember this"),
        "the user message should be persisted, got: {:?}",
        reopened.messages
    );

    unsafe {
        match prior {
            Some(v) => std::env::set_var(OPENCODE_GO_API_KEY, v),
            None => std::env::remove_var(OPENCODE_GO_API_KEY),
        }
    }
}
