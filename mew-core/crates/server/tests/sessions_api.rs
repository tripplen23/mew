//! HTTP-level integration tests for the session CRUD handlers.
//!
//! Drives the real axum app (`build_app`) in-process via `tower`'s `oneshot`
//! against the in-memory store, exercising status codes, bodies, ordering, and
//! the create -> get -> delete -> 404 lifecycle.

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use mewcode_engine::memory::MemoryStore as FactStore;
use mewcode_protocol::routes::SESSIONS;
use mewcode_protocol::{Mode, ModelId};
use mewcode_server::store::memory::MemoryStore;
use mewcode_server::store::{Session, SessionSummary};
use mewcode_server::{AppState, ServerConfig, build_app};
use serde_json::{Value, json};
use tower::ServiceExt;

/// A throwaway server config; the session handlers never touch the API key.
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
    let state = AppState::new(test_config(), Arc::new(MemoryStore::default()), fact_store);
    build_app(state)
}

/// `GET /sessions/{id}` path for a given id.
fn session_path(id: &uuid::Uuid) -> String {
    format!("{SESSIONS}/{id}")
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

/// Build a `POST /sessions` request with the given JSON body.
fn post_session(body: Value) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(SESSIONS)
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .expect("request should build")
}

#[tokio::test]
async fn full_lifecycle_create_get_delete_then_not_found() {
    let app = app();

    // POST /sessions -> 201, no messages.
    let (status, bytes) = send(
        app.clone(),
        post_session(json!({ "title": "Lifecycle", "model": "glm-5.1", "mode": "PLAN" })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let created: Session = serde_json::from_slice(&bytes).expect("create body is a Session");
    assert_eq!(created.title, "Lifecycle");
    assert_eq!(created.model, ModelId::Glm51);
    assert_eq!(created.mode, Mode::Plan);
    assert_eq!(created.messages.len(), 0);

    // GET /sessions/{id} -> hydrated session matches.
    let (status, bytes) = send(
        app.clone(),
        Request::builder()
            .uri(session_path(&created.id))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let fetched: Session = serde_json::from_slice(&bytes).expect("get body is a Session");
    assert_eq!(fetched.id, created.id);
    assert_eq!(fetched.title, "Lifecycle");
    assert_eq!(fetched.model, ModelId::Glm51);
    assert_eq!(fetched.mode, Mode::Plan);

    // DELETE /sessions/{id} -> 204, empty body.
    let (status, bytes) = send(
        app.clone(),
        Request::builder()
            .method("DELETE")
            .uri(session_path(&created.id))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert!(bytes.is_empty(), "204 response should have an empty body");

    // GET /sessions/{id} -> 404 after deletion.
    let (status, _) = send(
        app,
        Request::builder()
            .uri(session_path(&created.id))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn empty_or_whitespace_title_is_rejected() {
    // Empty title.
    let (status, _) = send(app(), post_session(json!({ "title": "" }))).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // Whitespace-only title.
    let (status, _) = send(app(), post_session(json!({ "title": "   \t  " }))).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn list_returns_summaries_newest_first_without_messages() {
    let app = app();

    let mut created_ids = Vec::new();
    for title in ["first", "second", "third"] {
        let (status, bytes) = send(app.clone(), post_session(json!({ "title": title }))).await;
        assert_eq!(status, StatusCode::CREATED);
        let session: Session = serde_json::from_slice(&bytes).unwrap();
        created_ids.push(session.id);
    }

    let (status, bytes) = send(
        app,
        Request::builder()
            .uri(SESSIONS)
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let summaries: Vec<SessionSummary> =
        serde_json::from_slice(&bytes).expect("list body is summaries");

    // Newest-first: reverse of creation order.
    let listed: Vec<uuid::Uuid> = summaries.iter().map(|s| s.id).collect();
    let expected: Vec<uuid::Uuid> = created_ids.into_iter().rev().collect();
    assert_eq!(listed, expected);

    // Summaries carry no message history (the field is absent in the wire shape).
    let value: Value = serde_json::from_slice(&bytes).unwrap();
    for entry in value.as_array().expect("list is a JSON array") {
        assert!(
            entry.get("messages").is_none(),
            "summary should not include messages: {entry}"
        );
    }
}

#[tokio::test]
async fn missing_id_get_and_delete_return_not_found() {
    let app = app();
    let id = uuid::Uuid::new_v4();

    let (status, _) = send(
        app.clone(),
        Request::builder()
            .uri(session_path(&id))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let (status, _) = send(
        app,
        Request::builder()
            .method("DELETE")
            .uri(session_path(&id))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}
