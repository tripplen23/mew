//! End-to-end test for the tool-calling loop.
//!
//! Verifies that:
//! 1. The `RigToolAdapter` correctly bridges `ToolContracts` → `ToolDyn`
//! 2. `rig_tools()` produces a non-empty tool list from a real registry
//! 3. The adapter's `call()` dispatches to `execute()` and returns the
//!    output as a JSON string
//! 4. The adapter's `definition()` returns a `ToolDefinition` matching
//!    the tool's descriptor

use std::sync::Arc;

use mewcode_engine::memory::MemoryStore;
use mewcode_engine::skills::SkillRegistry;
use mewcode_engine::tools::adapter::{RigToolAdapter, rig_tools};
use mewcode_engine::tools::{MewcodeMemoryTool, ProjectContext, ReadFileTool, default_registry};
use mewcode_protocol::tool::ToolContracts;
use rig_core::tool::ToolDyn;

// `tempfile::TempDir::new` uses `mkdtemp(3)` for atomic uniqueness. The
// previous `SystemTime::as_nanos()` approach could collide under parallel
// test threads, letting one test's `remove_dir_all` wipe another's file.

fn fresh_data_dir() -> (tempfile::TempDir, std::path::PathBuf) {
    let tmp = tempfile::TempDir::new().expect("create temp dir");
    let path = tmp.path().to_path_buf();
    (tmp, path)
}

fn fresh_project() -> (tempfile::TempDir, std::path::PathBuf) {
    let tmp = tempfile::TempDir::new().expect("create temp project");
    let dir = tmp.path().to_path_buf();
    std::fs::create_dir(dir.join("src")).unwrap();
    std::fs::write(dir.join("src/lib.rs"), "pub fn hello() -> u32 { 42 }").unwrap();
    std::fs::write(
        dir.join("Cargo.toml"),
        "[package]\nname = \"test\"\nversion = \"0.1\"\n",
    )
    .unwrap();
    (tmp, dir)
}

#[tokio::test]
async fn rig_tools_from_default_registry_is_non_empty() {
    let (_tmp_dir, data_dir) = fresh_data_dir();
    let store = MemoryStore::new(data_dir);
    let (_tmp_proj, project) = fresh_project();
    let skills = Arc::new(SkillRegistry::load_defaults());
    let ctx = ProjectContext::new(project);
    let registry = default_registry(ctx, skills, Some(store), mewcode_protocol::Mode::Build);

    let tools = rig_tools(&registry);
    assert!(
        !tools.is_empty(),
        "rig_tools should produce tools from default_registry"
    );
    assert!(
        tools.iter().any(|t| t.name() == "read_file"),
        "should include read_file"
    );
    assert!(
        tools.iter().any(|t| t.name() == "mewcode_memory"),
        "should include mewcode_memory"
    );
}

#[tokio::test]
async fn adapter_call_dispatches_to_execute() {
    let (_tmp, data_dir) = fresh_data_dir();
    let store = MemoryStore::new(data_dir);
    let tool = Arc::new(MewcodeMemoryTool::new(store)) as Arc<dyn ToolContracts>;
    let adapter = RigToolAdapter::new(tool);

    // Write a fact via the adapter (same interface Rig's agent uses)
    let result = adapter
        .call(r#"{"action":"write","content":"User prefers Rust."}"#.to_string())
        .await
        .expect("adapter call should succeed");

    let parsed: serde_json::Value =
        serde_json::from_str(&result).expect("output should be valid JSON");
    assert_eq!(parsed["status"], "written");
    assert_eq!(parsed["profile"], "default");

    // Read it back via the adapter
    let result = adapter
        .call(r#"{"action":"read"}"#.to_string())
        .await
        .expect("adapter read should succeed");

    let parsed: serde_json::Value =
        serde_json::from_str(&result).expect("output should be valid JSON");
    assert_eq!(parsed["content"], "User prefers Rust.");
}

#[tokio::test]
async fn adapter_definition_matches_descriptor() {
    let (_tmp, data_dir) = fresh_data_dir();
    let store = MemoryStore::new(data_dir);
    let tool = Arc::new(MewcodeMemoryTool::new(store)) as Arc<dyn ToolContracts>;
    let descriptor = tool.descriptor();
    let adapter = RigToolAdapter::new(tool);

    let def = adapter.definition(String::new()).await;
    assert_eq!(def.name, descriptor.name);
    assert_eq!(def.parameters, descriptor.input_schema);
    assert_eq!(def.description, descriptor.description);
    assert!(!def.description.is_empty());
}

#[tokio::test]
async fn adapter_call_read_file_returns_content() {
    let (_tmp, project) = fresh_project();
    let ctx = ProjectContext::new(project);
    let tool = Arc::new(ReadFileTool::new(ctx)) as Arc<dyn ToolContracts>;
    let adapter = RigToolAdapter::new(tool);

    let result = adapter
        .call(r#"{"path":"src/lib.rs"}"#.to_string())
        .await
        .expect("read_file adapter call should succeed");

    let parsed: serde_json::Value =
        serde_json::from_str(&result).expect("output should be valid JSON");
    let content = parsed["content"]
        .as_str()
        .expect("content should be a string");
    assert!(
        content.contains("pub fn hello"),
        "should contain file contents"
    );
}

#[tokio::test]
async fn adapter_call_with_invalid_args_returns_error_payload() {
    let (_tmp, data_dir) = fresh_data_dir();
    let store = MemoryStore::new(data_dir);
    let tool = Arc::new(MewcodeMemoryTool::new(store)) as Arc<dyn ToolContracts>;
    let adapter = RigToolAdapter::new(tool);

    // Missing required `action` field
    let result = adapter
        .call(r#"{}"#.to_string())
        .await
        .expect("adapter should not panic on bad input");

    let parsed: serde_json::Value =
        serde_json::from_str(&result).expect("error should be valid JSON");
    assert_eq!(parsed["error"], true);
    assert_eq!(parsed["kind"], "invalid_input");
}

#[tokio::test]
async fn adapter_call_with_malformed_json_returns_error_payload() {
    let (_tmp, data_dir) = fresh_data_dir();
    let store = MemoryStore::new(data_dir);
    let tool = Arc::new(MewcodeMemoryTool::new(store)) as Arc<dyn ToolContracts>;
    let adapter = RigToolAdapter::new(tool);

    // Not valid JSON at all
    let result = adapter
        .call("not json at all".to_string())
        .await
        .expect("adapter should not panic on malformed JSON");

    let parsed: serde_json::Value =
        serde_json::from_str(&result).expect("error should be valid JSON");
    assert_eq!(parsed["error"], true);
    assert_eq!(parsed["kind"], "invalid_input");
    assert!(
        parsed["message"]
            .as_str()
            .unwrap_or("")
            .contains("invalid JSON"),
        "error message should mention invalid JSON"
    );
}
