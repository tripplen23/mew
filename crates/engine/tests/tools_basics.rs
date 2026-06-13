//! Integration tests for the engine's tool implementations.
//!
//! Exercises the public API of every tool the engine ships today.
//! The `tmp()` helper is inlined here because external tests cannot
//! share private items with the crate.

use std::sync::atomic::{AtomicUsize, Ordering};

use mewcode_engine::skills::{SkillRegistry, SkillSource};
use mewcode_engine::tools::{
    GlobTool, ListDirectoryTool, ProjectContext, ReadFileTool, UseSkillTool,
};
use mewcode_protocol::tool::ToolContracts;
use mewcode_protocol::ToolError;
use serde_json::json;

static COUNTER: AtomicUsize = AtomicUsize::new(0);

fn tmp() -> ProjectContext {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    let dir = std::env::temp_dir()
        .join(format!("mewcode-tool-test-{pid}-{n}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(dir.join("src/lib.rs"), "pub fn hello() {}").unwrap();
    std::fs::write(dir.join("Cargo.toml"), "[package]\nname=\"x\"\n").unwrap();
    ProjectContext::new(dir)
}

#[tokio::test]
async fn read_file_returns_content() {
    let ctx = tmp();
    let tool = ReadFileTool::new(ctx.clone());
    let out = tool.execute(json!({ "path": "Cargo.toml" })).await.unwrap();
    let v = &out.0;
    assert_eq!(v["content"], "[package]\nname=\"x\"\n");
}

#[tokio::test]
async fn read_file_rejects_path_outside_root() {
    let ctx = tmp();
    let tool = ReadFileTool::new(ctx);
    let err = tool
        .execute(json!({ "path": "../outside.txt" }))
        .await
        .unwrap_err();
    assert!(matches!(err, ToolError::Rejected { .. }));
}

#[tokio::test]
async fn list_directory_sorts_dirs_first() {
    let ctx = tmp();
    let tool = ListDirectoryTool::new(ctx);
    let out = tool.execute(json!({})).await.unwrap();
    let entries = out.0["entries"].as_array().unwrap();
    assert!(entries.iter().any(|e| e["name"] == "src"));
    assert!(entries.iter().any(|e| e["name"] == "Cargo.toml"));
}

#[tokio::test]
async fn glob_finds_matching_files() {
    let ctx = tmp();
    let tool = GlobTool::new(ctx);
    let out = tool.execute(json!({ "pattern": "**/*.rs" })).await.unwrap();
    let files = out.0["files"].as_array().unwrap();
    assert!(files.iter().any(|f| f.as_str().unwrap().ends_with("lib.rs")));
}

#[tokio::test]
async fn use_skill_returns_body() {
    let tmpdir = std::env::temp_dir()
        .join(format!("mewcode-skill-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmpdir);
    std::fs::create_dir_all(tmpdir.join("alpha")).unwrap();
    std::fs::write(
        tmpdir.join("alpha/SKILL.md"),
        "---\nname: alpha\ndescription: an alpha skill\n---\n# Alpha\n\nbody content",
    )
    .unwrap();

    let mut reg = SkillRegistry::new();
    reg.load_dir(&tmpdir, SkillSource::Global);
    let skills = std::sync::Arc::new(reg);

    let tool = UseSkillTool::new(skills);
    let out = tool.execute(json!({ "name": "alpha" })).await.unwrap();
    assert!(out.0["body"].as_str().unwrap().contains("body content"));
}
