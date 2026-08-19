//! Tests for the `todo_write` / `todo_read` tools (`I001`, `I002`, `I005`).
//! Guards: replace-all semantics, status validation, cap, content validation,
//! read round-trip, and the display-record payload pushed for the dock.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use mewcode_engine::context::{MAX_TODOS, TodoStore};
use mewcode_engine::tools::{DisplaySink, ProjectContext, TodoReadTool, TodoWriteTool};
use mewcode_protocol::ToolDisplay;
use mewcode_protocol::tool::ToolContracts;
use serde_json::json;
use uuid::Uuid;

static COUNTER: AtomicUsize = AtomicUsize::new(0);

fn fresh() -> (TodoStore, DisplaySink) {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("mewcode-todotool-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let store = TodoStore::for_session(dir, &Uuid::new_v4());
    (store, Arc::new(Mutex::new(Vec::new())))
}

fn write_tool(store: TodoStore) -> (TodoWriteTool, DisplaySink) {
    let sink: DisplaySink = Arc::new(Mutex::new(Vec::new()));
    let ctx = ProjectContext::new(std::env::temp_dir()).with_display(sink.clone());
    (TodoWriteTool::new(store, ctx), sink)
}

#[tokio::test]
async fn write_replaces_entire_list_and_returns_summary() {
    let (store, _) = fresh();
    let (tool, _) = write_tool(store.clone());

    let out = tool
        .execute(json!({
            "todos": [
                { "content": "one", "status": "in_progress" },
                { "content": "two", "status": "completed" }
            ]
        }))
        .await
        .unwrap();
    let text = out.0.as_str().unwrap();
    assert!(text.contains("2 todos"));
    assert!(text.contains("1 in progress"));
    assert!(text.contains("1 completed"));

    // Replace-all: a smaller second write drops the first items.
    tool.execute(json!({ "todos": [{ "content": "only", "status": "pending" }] }))
        .await
        .unwrap();
    let loaded = store.load();
    assert_eq!(loaded.len(), 1);
    assert_eq!(loaded[0].content, "only");
}

#[tokio::test]
async fn write_pushes_todo_display_for_dock() {
    let (store, _) = fresh();
    let (tool, sink) = write_tool(store);
    let args = json!({ "todos": [{ "content": "x", "status": "pending" }] });
    tool.execute(args.clone()).await.unwrap();

    let records = sink.lock().unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].args, args);
    match &records[0].display {
        ToolDisplay::Todo(display) => assert_eq!(display.todos.len(), 1),
        other => panic!("expected ToolDisplay::Todo, got {other:?}"),
    }
}

#[tokio::test]
async fn write_rejects_bad_status_and_empty_content() {
    let (store, _) = fresh();
    let (tool, _) = write_tool(store);

    let bad_status = tool
        .execute(json!({ "todos": [{ "content": "x", "status": "halfway" }] }))
        .await;
    assert!(bad_status.is_err());

    let empty = tool
        .execute(json!({ "todos": [{ "content": "   ", "status": "pending" }] }))
        .await;
    assert!(empty.is_err());
}

#[tokio::test]
async fn write_rejects_over_cap() {
    let (store, _) = fresh();
    let (tool, _) = write_tool(store);
    let items: Vec<_> = (0..=MAX_TODOS)
        .map(|i| json!({ "content": format!("task {i}"), "status": "pending" }))
        .collect();
    let out = tool.execute(json!({ "todos": items })).await;
    assert!(out.is_err());
    let err = format!("{:?}", out.unwrap_err());
    assert!(err.contains("too many todos"));
}

#[tokio::test]
async fn read_returns_empty_then_written_list() {
    let (store, _) = fresh();
    let read = TodoReadTool::new(store.clone());
    let out = read.execute(json!({})).await.unwrap();
    assert_eq!(out.0.as_array().unwrap().len(), 0);

    let (write, _) = write_tool(store.clone());
    write
        .execute(json!({ "todos": [{ "content": "task", "status": "pending" }] }))
        .await
        .unwrap();
    let out = read.execute(json!({})).await.unwrap();
    let list = out.0.as_array().unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0]["content"], "task");
    assert!(!list[0]["id"].is_null(), "ids assigned on save");
}
