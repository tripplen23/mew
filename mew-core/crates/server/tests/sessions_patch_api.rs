//! HTTP-level integration tests for `PATCH /sessions/{id}`.
//!
//! Drives the real axum app (`build_app`) in-process via `tower`'s `oneshot`
//! against the in-memory store. Verifies the partial-update contract: only
//! `Some` fields are applied, `None` fields are left unchanged, and a
//! `404` is returned when the id does not exist.

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use mewcode_engine::memory::MemoryStore as FactStore;
use mewcode_protocol::routes::SESSIONS;
use mewcode_protocol::{Mode, ModelId};
use mewcode_server::store::Session;
use mewcode_server::store::memory::MemoryStore;
use mewcode_server::{AppState, ServerConfig, build_app};
use serde_json::{Value, json};
use tower::ServiceExt;

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

fn app() -> axum::Router {
    let fact_store = FactStore::new(std::env::temp_dir().join(uuid::Uuid::new_v4().to_string()));
    let state = AppState::new(test_config(), Arc::new(MemoryStore::default()), fact_store);
    build_app(state)
}

fn session_path(id: &uuid::Uuid) -> String {
    format!("{SESSIONS}/{id}")
}

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

fn post_session(body: Value) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(SESSIONS)
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .expect("request should build")
}

fn patch_session(id: &uuid::Uuid, body: Value) -> Request<Body> {
    Request::builder()
        .method("PATCH")
        .uri(session_path(id))
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .expect("request should build")
}

/// Helper: create a session on the given app, return it.
async fn create_session_on(app: &axum::Router) -> Session {
    let (status, bytes) = send(
        app.clone(),
        post_session(json!({ "title": "Before", "model": "glm-5.1", "mode": "BUILD" })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    serde_json::from_slice(&bytes).expect("create body is a Session")
}

#[tokio::test]
async fn patch_title_only_preserves_model_and_mode() {
    let app = app();
    let created = create_session_on(&app).await;

    let (status, bytes) = send(app, patch_session(&created.id, json!({ "title": "After" }))).await;
    assert_eq!(status, StatusCode::OK);
    let session: Session = serde_json::from_slice(&bytes).expect("patch body is a Session");
    assert_eq!(session.id, created.id);
    assert_eq!(session.title, "After");
    assert_eq!(session.model, ModelId::Glm51);
    assert_eq!(session.mode, Mode::Build);
    assert!(session.updated_at >= created.updated_at);
}

#[tokio::test]
async fn patch_model_only_preserves_title_and_mode() {
    let app = app();
    let created = create_session_on(&app).await;

    let (status, bytes) = send(
        app,
        patch_session(&created.id, json!({ "model": "minimax-m3" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let session: Session = serde_json::from_slice(&bytes).expect("patch body is a Session");
    assert_eq!(session.title, "Before");
    assert_eq!(session.model, ModelId::MiniMaxM3);
    assert_eq!(session.mode, Mode::Build);
}

#[tokio::test]
async fn patch_mode_only_preserves_title_and_model() {
    let app = app();
    let created = create_session_on(&app).await;

    let (status, bytes) = send(app, patch_session(&created.id, json!({ "mode": "PLAN" }))).await;
    assert_eq!(status, StatusCode::OK);
    let session: Session = serde_json::from_slice(&bytes).expect("patch body is a Session");
    assert_eq!(session.title, "Before");
    assert_eq!(session.model, ModelId::Glm51);
    assert_eq!(session.mode, Mode::Plan);
}

#[tokio::test]
async fn patch_all_three_fields_at_once() {
    let app = app();
    let created = create_session_on(&app).await;

    let (status, bytes) = send(
        app,
        patch_session(
            &created.id,
            json!({ "title": "All three", "model": "minimax-m3", "mode": "PLAN" }),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let session: Session = serde_json::from_slice(&bytes).expect("patch body is a Session");
    assert_eq!(session.title, "All three");
    assert_eq!(session.model, ModelId::MiniMaxM3);
    assert_eq!(session.mode, Mode::Plan);
}

#[tokio::test]
async fn patch_unknown_model_returns_bad_request() {
    let app = app();
    let created = create_session_on(&app).await;

    // `ModelId` is an enum; an unknown variant fails serde deserialization,
    // which axum surfaces as 422 (Unprocessable Entity). Either 400 or 422
    // counts as "the bad model was rejected" — accept both.
    let (status, _) = send(
        app,
        patch_session(&created.id, json!({ "model": "not-a-real-model" })),
    )
    .await;
    assert!(
        status == StatusCode::BAD_REQUEST || status == StatusCode::UNPROCESSABLE_ENTITY,
        "expected 400/422, got {status}",
    );
}

#[tokio::test]
async fn patch_unknown_id_returns_not_found() {
    let app = app();
    let id = uuid::Uuid::new_v4();

    let (status, _) = send(app, patch_session(&id, json!({ "title": "ghost" }))).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn patch_whitespace_title_returns_bad_request() {
    let app = app();
    let created = create_session_on(&app).await;

    let (status, _) = send(
        app,
        patch_session(&created.id, json!({ "title": "   \t  " })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn patch_session_returns_messages_sorted_by_created_at() {
    use mewcode_protocol::{Message, MessagePart, Role};
    use mewcode_server::store::SessionStore;

    let fact_store = mewcode_engine::memory::MemoryStore::new(
        std::env::temp_dir().join(uuid::Uuid::new_v4().to_string()),
    );
    let store = Arc::new(MemoryStore::default());
    let state = AppState::new(test_config(), store.clone(), fact_store);
    let app = build_app(state);

    let created = create_session_on(&app).await;

    // Append out of order directly through the store. `patch_session`
    // should hydrate chronologically, matching `get_session`.
    let now = chrono::Utc::now();
    store
        .append_message(
            created.id,
            Message {
                id: uuid::Uuid::new_v4(),
                role: Role::User,
                parts: vec![MessagePart::Text {
                    text: "second".into(),
                }],
                model: None,
                created_at: now + chrono::Duration::seconds(5),
            },
        )
        .await
        .unwrap();
    store
        .append_message(
            created.id,
            Message {
                id: uuid::Uuid::new_v4(),
                role: Role::User,
                parts: vec![MessagePart::Text {
                    text: "first".into(),
                }],
                model: None,
                created_at: now,
            },
        )
        .await
        .unwrap();

    let (status, bytes) = send(
        app,
        patch_session(&created.id, json!({ "title": "renamed" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let session: Session = serde_json::from_slice(&bytes).expect("patch body is a Session");
    let texts: Vec<String> = session
        .messages
        .iter()
        .map(|m| match &m.parts[0] {
            MessagePart::Text { text } => text.clone(),
            _ => panic!("expected text part"),
        })
        .collect();
    assert_eq!(texts, vec!["first", "second"]);
}
