//! Display side-channel (Design B): mutating fs tools record a render-only
//! `ToolDisplay::Diff` in the `ProjectContext` sink during execution, while
//! their model-facing output stays metadata-only.
//!
//! The critical invariant these tests guard is the stop-condition from the
//! migration plan: the diff must NOT leak into the model-facing tool output.

use std::sync::{Arc, Mutex};

use mewcode_engine::tools::{DisplaySink, EditFileTool, ProjectContext, WriteFileTool};
use mewcode_protocol::ToolDisplay;
use mewcode_protocol::tool::ToolContracts;
use serde_json::json;

fn fresh_project() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "mewcode-display-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn only_diff(sink: &DisplaySink) -> mewcode_protocol::DiffDisplay {
    let records = sink.lock().unwrap();
    assert_eq!(records.len(), 1, "expected exactly one display record");
    match &records[0].display {
        ToolDisplay::Diff(d) => d.clone(),
    }
}

#[tokio::test]
async fn write_file_new_file_records_all_additions_diff() {
    let project = fresh_project();
    let sink: DisplaySink = Arc::new(Mutex::new(Vec::new()));
    let ctx = ProjectContext::new(project.clone()).with_display(sink.clone());
    let tool = WriteFileTool::new(ctx);

    let out = tool
        .execute(json!({"path": "new.rs", "content": "fn main() {}\n"}))
        .await
        .expect("write should succeed");

    // Model-facing output is metadata only — no diff/old/new leaked.
    assert_eq!(out.0["path"], "new.rs");
    assert!(out.0.get("old").is_none(), "old must not be in tool output");
    assert!(out.0.get("new").is_none(), "new must not be in tool output");
    assert!(
        out.0.get("diff").is_none(),
        "diff must not be in tool output"
    );

    // Display channel carries the diff: new file => empty old side.
    let diff = only_diff(&sink);
    assert_eq!(diff.path, "new.rs");
    assert_eq!(diff.old, "");
    assert_eq!(diff.new, "fn main() {}\n");

    let _ = std::fs::remove_dir_all(&project);
}

#[tokio::test]
async fn write_file_overwrite_captures_preimage() {
    let project = fresh_project();
    std::fs::write(project.join("f.txt"), "old contents\n").unwrap();
    let sink: DisplaySink = Arc::new(Mutex::new(Vec::new()));
    let ctx = ProjectContext::new(project.clone()).with_display(sink.clone());
    let tool = WriteFileTool::new(ctx);

    tool.execute(json!({"path": "f.txt", "content": "new contents\n", "overwrite": true}))
        .await
        .expect("overwrite should succeed");

    // The pre-image (old side) is captured before the write — the whole point
    // of Design B, since only the engine can see it.
    let diff = only_diff(&sink);
    assert_eq!(diff.old, "old contents\n");
    assert_eq!(diff.new, "new contents\n");

    let _ = std::fs::remove_dir_all(&project);
}

#[tokio::test]
async fn edit_file_records_diff_without_leaking_to_output() {
    let project = fresh_project();
    std::fs::write(project.join("lib.rs"), "fn old_name() {}\n").unwrap();
    let sink: DisplaySink = Arc::new(Mutex::new(Vec::new()));
    let ctx = ProjectContext::new(project.clone()).with_display(sink.clone());
    let tool = EditFileTool::new(ctx);

    let out = tool
        .execute(json!({
            "path": "lib.rs",
            "old_string": "fn old_name()",
            "new_string": "fn new_name()"
        }))
        .await
        .expect("edit should succeed");

    // Model-facing output unchanged (byte metadata only).
    assert_eq!(out.0["bytes_replaced"], 13);
    assert!(out.0.get("old_string").is_none());
    assert!(out.0.get("new_string").is_none());

    let diff = only_diff(&sink);
    assert_eq!(diff.path, "lib.rs");
    assert_eq!(diff.old, "fn old_name()");
    assert_eq!(diff.new, "fn new_name()");
    assert_eq!(diff.start_line, Some(1));

    let _ = std::fs::remove_dir_all(&project);
}

#[tokio::test]
async fn no_sink_means_no_panic_and_no_records() {
    // Tools called without a display sink (e.g. dispatch-only paths) must work.
    let project = fresh_project();
    let tool = WriteFileTool::new(ProjectContext::new(project.clone()));
    tool.execute(json!({"path": "x.txt", "content": "hi"}))
        .await
        .expect("write should succeed without a sink");
    let _ = std::fs::remove_dir_all(&project);
}
