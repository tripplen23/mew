//! `POST /review` behavior: failure path and throwaway-session cleanup.
//!
//! Drives the real axum app in-process via `tower`'s `oneshot`, following
//! the same pattern as `chat_failure.rs`. The LLM provider is unavailable in
//! tests, so the happy path (real findings) is not covered here; the
//! credential-boundary failure is deterministic and covers the cleanup that
//! also runs on the success path.

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use mewcode_engine::context::MemoryStore as FactStore;
use mewcode_protocol::env::OPENCODE_GO_API_KEY;
use mewcode_protocol::event::ReviewRequest;
use mewcode_protocol::routes::REVIEW;
use mewcode_server::routes::longest_backtick_run;
use mewcode_server::store::SessionStore as _;
use mewcode_server::store::memory::MemoryStore;
use mewcode_server::{AppState, ServerConfig, build_app};
use tower::ServiceExt;

fn test_config() -> ServerConfig {
    ServerConfig {
        host: "127.0.0.1".into(),
        port: 0,
        opencode_go_api_key: None,
        openai_api_key: None,
        default_model: None,
        log: "off".into(),
        skills: Default::default(),
    }
}

async fn app() -> (axum::Router, Arc<MemoryStore>) {
    let fact_store = FactStore::new(std::env::temp_dir().join(uuid::Uuid::new_v4().to_string()));
    let store = Arc::new(MemoryStore::default());
    let state = AppState::new(test_config(), store.clone(), fact_store);
    // Clear real `~/.config/mew/credentials.yaml` so the missing-key
    // boundary is deterministic.
    state.credentials.lock().await.credentials.clear();
    (build_app(state), store)
}

fn post_review(req: &ReviewRequest) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(REVIEW)
        .header("content-type", "application/json")
        .body(Body::from(
            serde_json::to_string(req).expect("request serialises"),
        ))
        .expect("request should build")
}

#[tokio::test]
async fn failed_review_returns_500_and_cleans_up_session() {
    // Force a deterministic turn failure: with no API key, the harness fails
    // at the credential boundary before any provider is built.
    // SAFETY: single-threaded section of this test; no other thread reads
    // the var concurrently within this test binary.
    let prior = std::env::var(OPENCODE_GO_API_KEY).ok();
    unsafe {
        std::env::remove_var(OPENCODE_GO_API_KEY);
    }

    let (router, store) = app().await;
    let req = post_review(&ReviewRequest {
        diff: "diff --git a/a.rs b/a.rs\n@@ -1 +1 @@\n-fn main() {}\n+fn main() { println!(\"hi\"); }\n"
            .into(),
        extra: None,
    });
    let resp = router.oneshot(req).await.expect("router responds");
    assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
    let bytes = resp
        .into_body()
        .collect()
        .await
        .expect("body collects")
        .to_bytes()
        .to_vec();
    let body: serde_json::Value = serde_json::from_slice(&bytes).expect("json body");
    assert_eq!(body["findings"], "");

    // The throwaway session must not linger in the store.
    assert_eq!(
        store.list_sessions().await.expect("list sessions").len(),
        0,
        "throwaway review session must be deleted"
    );

    if let Some(key) = prior {
        unsafe {
            std::env::set_var(OPENCODE_GO_API_KEY, key);
        }
    }
}

#[test]
fn longest_backtick_run_counts_runs() {
    // The prompt fence must be longer than any backtick run in the diff so a
    // malicious diff cannot close the fence early and inject instructions.
    assert_eq!(longest_backtick_run("no ticks"), 0);
    assert_eq!(longest_backtick_run("a ` b"), 1);
    assert_eq!(longest_backtick_run("```"), 3);
    assert_eq!(longest_backtick_run("a `` b ``` c"), 3);
    assert_eq!(longest_backtick_run("``````"), 6);
}
