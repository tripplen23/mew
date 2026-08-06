//! `POST /webhook/github` behavior: signature verification, mention
//! detection, and the accept/ignore contract. The review itself is
//! detached and requires live GitHub credentials, so it is not exercised
//! here; the accept/ignore responses are the observable contract.

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use hmac::{Hmac, Mac};
use mewcode_engine::context::MemoryStore as FactStore;
use mewcode_protocol::routes::GITHUB_WEBHOOK;
use mewcode_server::routes::verify_signature;
use mewcode_server::services::github_bot::mention_request;
use mewcode_server::store::memory::MemoryStore;
use mewcode_server::{AppState, ServerConfig, build_app};
use serde_json::{Value, json};
use sha2::Sha256;
use tower::ServiceExt;

const SECRET: &str = "test-webhook-secret";

type HmacSha256 = Hmac<Sha256>;

fn signature(body: &[u8], secret: &str) -> String {
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).expect("HMAC accepts any key");
    mac.update(body);
    let digest = mac
        .finalize()
        .into_bytes()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    format!("sha256={digest}")
}

fn test_config() -> ServerConfig {
    ServerConfig {
        host: "127.0.0.1".into(),
        port: 0,
        opencode_go_api_key: None,
        openai_api_key: None,
        default_model: None,
        log: "off".into(),
        skills: Default::default(),
        github: mewcode_server::config::GithubServerConfig {
            webhook_secret: Some(SECRET.into()),
            app_id: Some(12345),
            private_key_path: Some("/nonexistent-test-key.pem".into()),
        },
    }
}

async fn app() -> axum::Router {
    let fact_store = FactStore::new(std::env::temp_dir().join(uuid::Uuid::new_v4().to_string()));
    let store = Arc::new(MemoryStore::default());
    let state = AppState::new(test_config(), store, fact_store);
    build_app(state)
}

async fn app_without_github() -> axum::Router {
    let fact_store = FactStore::new(std::env::temp_dir().join(uuid::Uuid::new_v4().to_string()));
    let store = Arc::new(MemoryStore::default());
    let mut config = test_config();
    config.github = Default::default();
    let state = AppState::new(config, store, fact_store);
    build_app(state)
}

fn webhook_post(payload: &Value, secret: Option<&str>) -> Request<Body> {
    let body = serde_json::to_vec(payload).expect("payload serialises");
    let mut builder = Request::builder()
        .method("POST")
        .uri(GITHUB_WEBHOOK)
        .header("content-type", "application/json")
        .header("x-github-event", "issue_comment")
        .header("x-github-delivery", uuid::Uuid::new_v4().to_string());
    if let Some(secret) = secret {
        builder = builder.header("x-hub-signature-256", signature(&body, secret));
    }
    builder
        .body(Body::from(body))
        .expect("request should build")
}

fn mention_payload(body: &str) -> Value {
    json!({
        "action": "created",
        "issue": { "number": 42, "state": "open", "pull_request": {} },
        "comment": { "body": body },
        "repository": { "full_name": "tripplen23/mew" }
    })
}

async fn body_json(response: axum::response::Response) -> (StatusCode, Value) {
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("body readable");
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

#[tokio::test]
async fn webhook_accepts_mention_with_valid_signature() {
    let payload = mention_payload("please review @mew");
    let (status, body) = body_json(
        app()
            .await
            .oneshot(webhook_post(&payload, Some(SECRET)))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, json!({ "accepted": true }));
}

#[tokio::test]
async fn webhook_rejects_bad_signature() {
    let payload = mention_payload("please review @mew");
    let (status, body) = body_json(
        app()
            .await
            .oneshot(webhook_post(&payload, Some("wrong-secret")))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body, json!({ "accepted": false }));
}

#[tokio::test]
async fn webhook_rejects_missing_signature() {
    let payload = mention_payload("please review @mew");
    let (status, body) = body_json(
        app()
            .await
            .oneshot(webhook_post(&payload, None))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body, json!({ "accepted": false }));
}

#[tokio::test]
async fn webhook_disabled_without_full_github_config() {
    let payload = mention_payload("please review @mew");
    let response = app_without_github()
        .await
        .oneshot(webhook_post(&payload, Some(SECRET)))
        .await
        .unwrap();
    let (status, body) = body_json(response).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body, json!({ "accepted": false }));
}

#[tokio::test]
async fn webhook_deduplicates_delivery_ids() {
    let payload = mention_payload("please review @mew");
    let bytes = serde_json::to_vec(&payload).expect("payload serialises");
    let uri = GITHUB_WEBHOOK.to_string();
    let delivery = "11111111-1111-1111-1111-111111111111";
    let mut request = Request::builder()
        .method("POST")
        .uri(uri.clone())
        .header("content-type", "application/json")
        .header("x-github-event", "issue_comment")
        .header("x-github-delivery", delivery)
        .header("x-hub-signature-256", signature(&bytes, SECRET))
        .body(Body::from(bytes.clone()))
        .expect("request should build");
    let app = app().await;
    let (status, body) = body_json(app.clone().oneshot(request).await.unwrap()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, json!({ "accepted": true }));
    request = Request::builder()
        .method("POST")
        .uri(uri)
        .header("content-type", "application/json")
        .header("x-github-event", "issue_comment")
        .header("x-github-delivery", delivery)
        .header("x-hub-signature-256", signature(&bytes, SECRET))
        .body(Body::from(bytes))
        .expect("request should build");
    let (status, body) = body_json(app.oneshot(request).await.unwrap()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, json!({ "accepted": false, "duplicate": true }));
}

#[tokio::test]
async fn webhook_acknowledges_ping_without_repository() {
    let payload = json!({ "zen": "keep it simple" });
    let (status, body) = body_json(
        app()
            .await
            .oneshot(webhook_post(&payload, Some(SECRET)))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, json!({ "accepted": false }));
}

#[tokio::test]
async fn webhook_ignores_non_mention_comment() {
    let payload = mention_payload("looks good to me");
    let (status, body) = body_json(
        app()
            .await
            .oneshot(webhook_post(&payload, Some(SECRET)))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, json!({ "accepted": false }));
}

#[tokio::test]
async fn webhook_acknowledges_unparseable_payload() {
    let body = b"not json at all";
    let request = Request::builder()
        .method("POST")
        .uri(GITHUB_WEBHOOK)
        .header("content-type", "application/json")
        .header("x-github-event", "issue_comment")
        .header("x-hub-signature-256", signature(body, SECRET))
        .body(Body::from(body.to_vec()))
        .expect("request should build");
    let (status, body) = body_json(app().await.oneshot(request).await.unwrap()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, json!({ "accepted": false }));
}

#[tokio::test]
async fn verify_signature_matches_github_format() {
    let body = b"{\"hello\":\"world\"}";
    assert!(verify_signature(SECRET, body, &signature(body, SECRET)));
    assert!(!verify_signature(SECRET, body, &signature(body, "other")));
    assert!(!verify_signature(SECRET, body, "sha256=zz"));
    assert!(!verify_signature(SECRET, body, "nope"));
}

#[test]
fn mention_request_matches_mew_mention() {
    let payload = mention_payload("@Mew review this please");
    assert_eq!(mention_request("issue_comment", &payload), Some(42));
}

#[test]
fn mention_request_matches_mewcli_mention() {
    let payload = mention_payload("please review @mewcli");
    assert_eq!(mention_request("issue_comment", &payload), Some(42));
}

#[test]
fn mention_request_rejects_similar_handles_and_emails() {
    assert_eq!(
        mention_request("issue_comment", &mention_payload("@mewbot review")),
        None
    );
    assert_eq!(
        mention_request("issue_comment", &mention_payload("user@mew.example")),
        None
    );
    assert_eq!(
        mention_request("issue_comment", &mention_payload("@mew.example")),
        None
    );
    assert_eq!(
        mention_request("issue_comment", &mention_payload("mention @mewtoo")),
        None
    );
}

#[test]
fn mention_request_ignores_code_fences() {
    let body = "```\n@mew\n```";
    assert_eq!(
        mention_request("issue_comment", &mention_payload(body)),
        None
    );
    let body = "```\nrun @mew --help\n```\nplease review @mewcli";
    assert_eq!(
        mention_request("issue_comment", &mention_payload(body)),
        Some(42)
    );
}

#[test]
fn mention_request_rejects_closed_prs() {
    let mut payload = mention_payload("@mew");
    payload["issue"]["state"] = Value::from("closed");
    assert_eq!(mention_request("issue_comment", &payload), None);
}

#[test]
fn mention_request_rejects_issues_and_other_events() {
    let mut issue = mention_payload("@mew");
    issue["issue"]["pull_request"] = Value::Null;
    assert_eq!(mention_request("issue_comment", &issue), None);
    assert_eq!(
        mention_request("pull_request", &mention_payload("@mew")),
        None
    );
    assert_eq!(
        mention_request("issue_comment", &mention_payload("@meow")),
        None
    );
}
