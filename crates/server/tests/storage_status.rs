//! Property 9: Status truthfulness.
//!
//! `GET /storage/status` always reports the backend that was actually selected
//! at startup, and its `data_dir` matches that backend: a memory-backed app
//! reports `"memory"` / `null`; an `FsStore`-over-tempdir app reports
//! `"filesystem"` and exactly the tempdir path. The endpoint reads only store
//! metadata, never session or message data.
//!
//! Driven app-level (via `tower`'s `oneshot` against `build_app`) so the
//! property covers routing and JSON serialization, matching the harness in
//! `sessions_api.rs`.
//!
//! **Validates: Requirements 5.2, 5.3**

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use mewcode_protocol::routes::STORAGE_STATUS;
use mewcode_server::store::fs::FsStore;
use mewcode_server::store::memory::MemoryStore;
use mewcode_server::store::SessionStore;
use mewcode_server::{AppState, ServerConfig, build_app};
use proptest::prelude::*;
use serde_json::Value;
use tempfile::TempDir;
use tower::ServiceExt;

/// A throwaway server config; the storage-status handler never touches it.
fn test_config() -> ServerConfig {
    ServerConfig {
        host: "127.0.0.1".into(),
        port: 0,
        opencode_go_api_key: "test-key".into(),
        default_model: None,
        log: "off".into(),
    }
}

/// `GET /storage/status` against an app over `store`, returning the JSON body.
async fn status_body(store: Arc<dyn SessionStore>) -> Value {
    let app = build_app(AppState::new(test_config(), store));
    let resp = app
        .oneshot(
            Request::builder()
                .uri(STORAGE_STATUS)
                .body(Body::empty())
                .expect("request builds"),
        )
        .await
        .expect("router responds");
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = resp
        .into_body()
        .collect()
        .await
        .expect("body collects")
        .to_bytes();
    serde_json::from_slice(&bytes).expect("body is JSON")
}

#[tokio::test]
async fn memory_backend_reports_memory_and_null_data_dir() {
    let body = status_body(Arc::new(MemoryStore::default())).await;
    assert_eq!(body.get("backend").and_then(Value::as_str), Some("memory"));
    assert!(
        body.get("data_dir").is_some_and(Value::is_null),
        "memory backend must report a null data_dir, got: {body}"
    );
}

#[tokio::test]
async fn filesystem_backend_reports_filesystem_and_tempdir_path() {
    let tmp = TempDir::new().expect("create temp data dir");
    let store = FsStore::new(tmp.path().to_path_buf()).expect("init FsStore");
    let body = status_body(Arc::new(store)).await;

    assert_eq!(
        body.get("backend").and_then(Value::as_str),
        Some("filesystem")
    );
    assert_eq!(
        body.get("data_dir").and_then(Value::as_str),
        Some(tmp.path().to_string_lossy().as_ref()),
        "filesystem backend must report the resolved data dir"
    );
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(24))]

    /// For any (filesystem-safe) subdirectory name, an `FsStore` rooted there
    /// reports `"filesystem"` and exactly that path; the backend label is never
    /// anything but the active backend.
    #[test]
    fn fs_status_truthfully_reports_its_data_dir(sub in "[A-Za-z0-9_-]{1,32}") {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("build runtime");

        rt.block_on(async move {
            let tmp = TempDir::new().expect("create temp data dir");
            let dir = tmp.path().join(sub);
            let store = FsStore::new(dir.clone()).expect("init FsStore");
            let body = status_body(Arc::new(store)).await;
            let expected = dir.to_string_lossy().into_owned();

            prop_assert_eq!(
                body.get("backend").and_then(Value::as_str),
                Some("filesystem")
            );
            prop_assert_eq!(
                body.get("data_dir").and_then(Value::as_str),
                Some(expected.as_str())
            );
            Ok(())
        })?;
    }
}
