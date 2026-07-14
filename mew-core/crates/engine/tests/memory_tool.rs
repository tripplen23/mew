//! Tests for the `mewcode_memory` tool — the agent-facing surface over
//! `MemoryStore`. Today: profile-name validation.

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use mewcode_engine::memory::MemoryStore;
use mewcode_engine::tools::MewcodeMemoryTool;
use mewcode_protocol::ToolError;
use mewcode_protocol::tool::ToolContracts;
use serde_json::json;

static COUNTER: AtomicUsize = AtomicUsize::new(0);

fn fresh_data_dir() -> PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    let dir = std::env::temp_dir().join(format!("mewcode-memtool-test-{pid}-{n}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn make_tool() -> (MewcodeMemoryTool, PathBuf) {
    let dir = fresh_data_dir();
    let store = MemoryStore::new(dir.clone());
    (MewcodeMemoryTool::new(store), dir)
}

#[tokio::test]
async fn rejects_path_traversal_in_profile() {
    let (tool, data_dir) = make_tool();
    let canary = data_dir.parent().unwrap().join("escaped.md");
    let _ = std::fs::remove_file(&canary);

    for bad in [
        "../escaped",
        "..\\escaped",
        "a/b",
        "a\\b",
        "..",
        ".",
        ".hidden",
        "",
    ] {
        let result = tool
            .execute(json!({ "action": "write", "content": "x", "profile": bad }))
            .await;
        assert!(result.is_err(), "profile {bad:?} should be rejected");
        match result.unwrap_err() {
            ToolError::InvalidInput { .. } => {}
            other => panic!("expected InvalidInput for {bad:?}, got {other:?}"),
        }
    }

    // Nothing escaped the memories/ dir.
    assert!(!canary.exists(), "path traversal wrote outside memories/");
}

#[tokio::test]
async fn accepts_simple_profile_names() {
    let (tool, _dir) = make_tool();
    let r = tool
        .execute(json!({
            "action": "write",
            "content": "hi",
            "profile": "work",
        }))
        .await;
    assert!(r.is_ok(), "simple identifier should be accepted");
}

#[tokio::test]
async fn default_profile_round_trip() {
    let (tool, _dir) = make_tool();
    tool.execute(json!({ "action": "write", "content": "fact" }))
        .await
        .unwrap();
    let out = tool.execute(json!({ "action": "read" })).await.unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&out.0.to_string()).unwrap();
    assert_eq!(parsed["content"], "fact");
    assert_eq!(parsed["profile"], "default");
}
