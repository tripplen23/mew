//! HTTP-level + unit tests for turn abort (`POST /sessions/{id}/abort`):
//! the flag registry, the route, and the select-race that cancels a turn.

use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use mewcode_engine::context::MemoryStore as FactStore;
use mewcode_protocol::StreamEvent;
use mewcode_protocol::routes::SESSION_ABORT;
use mewcode_server::services::chat::run_turn_abortable;
use mewcode_server::store::memory::MemoryStore;
use mewcode_server::{AppState, ServerConfig, build_app};
use tokio::sync::mpsc;
use tower::ServiceExt;

fn test_config() -> ServerConfig {
    ServerConfig {
        host: "127.0.0.1".into(),
        port: 0,
        opencode_go_api_key: Some("test-key".into()),
        openai_api_key: None,
        default_model: None,
        log: "off".into(),
        skills: Default::default(),
        github: Default::default(),
        mcp: Default::default(),
    }
}

fn test_state() -> AppState {
    let fact_store = FactStore::new(std::env::temp_dir().join(uuid::Uuid::new_v4().to_string()));
    AppState::new(test_config(), Arc::new(MemoryStore::default()), fact_store)
}

fn abort_path(id: &uuid::Uuid) -> String {
    SESSION_ABORT.replace("{id}", &id.to_string())
}

async fn post(app: &axum::Router, path: &str) -> StatusCode {
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(path)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    resp.status()
}

#[tokio::test]
async fn abort_route_404_without_live_turn_and_202_with() {
    let state = test_state();
    let app = build_app(state.clone());
    let id = uuid::Uuid::new_v4();

    // No turn registered → abort is a no-op (404).
    assert_eq!(post(&app, &abort_path(&id)).await, StatusCode::NOT_FOUND);

    // Register a live turn → abort delivers (202) and the flag is raised.
    let flag = state.register_abort(id).await;
    assert!(!flag.load(std::sync::atomic::Ordering::Acquire));
    assert_eq!(post(&app, &abort_path(&id)).await, StatusCode::ACCEPTED);
    assert!(flag.load(std::sync::atomic::Ordering::Acquire));

    // After the turn unregisters, abort is a no-op again.
    state.unregister_abort(id).await;
    assert_eq!(post(&app, &abort_path(&id)).await, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn request_abort_returns_false_for_unknown_session() {
    let state = test_state();
    assert!(!state.request_abort(uuid::Uuid::new_v4()).await);
}

#[tokio::test]
async fn abortable_turn_returns_none_and_forwards_aborted() {
    let state = test_state();
    let id = uuid::Uuid::new_v4();
    let flag = state.register_abort(id).await;
    let (tx, mut rx) = mpsc::channel::<StreamEvent>(8);

    // Raise the flag before starting the race: the abort branch wins and the
    // never-completing turn is dropped.
    assert!(state.request_abort(id).await);
    let outcome = run_turn_abortable(flag.clone(), tx, std::future::pending()).await;
    assert!(outcome.is_none(), "aborted turn must not return a result");

    // The client-facing terminal event is forwarded through the channel.
    let event = tokio::time::timeout(Duration::from_secs(2), rx.recv())
        .await
        .expect("Aborted event within timeout")
        .expect("Aborted event present");
    assert_eq!(event, StreamEvent::Aborted);
}

#[tokio::test]
async fn abortable_turn_completes_when_flag_is_never_raised() {
    let state = test_state();
    let id = uuid::Uuid::new_v4();
    let flag = state.register_abort(id).await;
    let (tx, mut rx) = mpsc::channel::<StreamEvent>(8);

    let outcome = run_turn_abortable(flag, tx, async { Ok(()) }).await;
    assert!(
        matches!(outcome.unwrap(), Ok(())),
        "finished turn returns Ok"
    );
    assert!(
        rx.try_recv().is_err(),
        "a completed turn must not emit any event"
    );
}
