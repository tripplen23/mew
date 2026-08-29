//! HTTP-level tests for the uniform error body contract: every non-success
//! response is JSON `{"error": "<message>"}` — including axum rejections
//! (malformed body, bad path param, unmatched route), which axum would
//! otherwise render as plain text.

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use mewcode_engine::context::MemoryStore as FactStore;
use mewcode_protocol::routes::SESSIONS;
use mewcode_server::store::memory::MemoryStore;
use mewcode_server::{AppState, ServerConfig, build_app};
use serde_json::Value;
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

fn app() -> axum::Router {
    let fact_store = FactStore::new(std::env::temp_dir().join(uuid::Uuid::new_v4().to_string()));
    let state = AppState::new(test_config(), Arc::new(MemoryStore::default()), fact_store);
    build_app(state)
}

async fn send(app: &axum::Router, req: Request<Body>) -> (StatusCode, Value) {
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, value)
}

#[tokio::test]
async fn malformed_json_body_is_json_error() {
    let app = app();
    let (status, body) = send(
        &app,
        Request::builder()
            .method("POST")
            .uri(SESSIONS)
            .header("content-type", "application/json")
            .body(Body::from("{not json"))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(
        body.get("error").and_then(Value::as_str).is_some(),
        "expected {{\"error\": ...}}, got {body}",
    );
}

#[tokio::test]
async fn bad_path_param_is_json_error() {
    let app = app();
    let (status, body) = send(
        &app,
        Request::builder()
            .method("GET")
            .uri(format!("{SESSIONS}/not-a-uuid"))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(
        body.get("error").and_then(Value::as_str).is_some(),
        "expected {{\"error\": ...}}, got {body}",
    );
}

#[tokio::test]
async fn unmatched_route_is_json_error() {
    let app = app();
    let (status, body) = send(
        &app,
        Request::builder()
            .method("GET")
            .uri("/does/not/exist")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(
        body.get("error").and_then(Value::as_str).is_some(),
        "expected {{\"error\": ...}}, got {body}",
    );
}

#[tokio::test]
async fn app_error_is_unchanged_json() {
    let app = app();
    let (status, body) = send(
        &app,
        Request::builder()
            .method("GET")
            .uri(format!("{SESSIONS}/{}", uuid::Uuid::new_v4()))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"], "not found");
}

#[tokio::test]
async fn method_not_allowed_keeps_allow_header() {
    let app = app();
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(SESSIONS)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::METHOD_NOT_ALLOWED);
    assert!(
        resp.headers().get(axum::http::header::ALLOW).is_some(),
        "Allow header dropped by jsonify_errors rewrite"
    );
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let value: Value = serde_json::from_slice(&bytes).unwrap();
    assert!(
        value.get("error").is_some(),
        "expected {{\"error\": ...}}, got {value}"
    );
}
