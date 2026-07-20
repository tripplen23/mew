//! Unit tests for the Phase 12 tools: write_file, edit_file, bash, grep,
//! and the PLAN mode gate in `default_registry`.

use std::sync::Arc;

use mewcode_engine::tools::{
    BashTool, EditFileTool, GrepTool, ProjectContext, WriteFileTool, default_registry,
};
use mewcode_protocol::tool::ToolContracts;
use mewcode_protocol::{Mode, ToolError};
use serde_json::json;

fn fresh_project() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "mewcode-phase12-{}-{}",
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

// ---------------------------------------------------------------------------
// write_file
// ---------------------------------------------------------------------------

#[tokio::test]
async fn write_file_creates_new_file() {
    let project = fresh_project();
    let tool = WriteFileTool::new(ProjectContext::new(project.clone()));

    let result = tool
        .execute(json!({
            "path": "hello.txt",
            "content": "Hello, mewcode!"
        }))
        .await
        .expect("write should succeed");

    assert_eq!(result.0["path"], "hello.txt");
    assert_eq!(result.0["bytes_written"], 15);
    assert_eq!(result.0["overwritten"], false);

    let written = std::fs::read_to_string(project.join("hello.txt")).unwrap();
    assert_eq!(written, "Hello, mewcode!");

    let _ = std::fs::remove_dir_all(&project);
}

#[tokio::test]
async fn write_file_creates_parent_dirs() {
    let project = fresh_project();
    let tool = WriteFileTool::new(ProjectContext::new(project.clone()));

    tool.execute(json!({
        "path": "src/nested/deep.rs",
        "content": "fn main() {}"
    }))
    .await
    .expect("write should succeed");

    assert!(project.join("src/nested/deep.rs").exists());

    let _ = std::fs::remove_dir_all(&project);
}

#[tokio::test]
async fn write_file_refuses_overwrite_non_empty_without_flag() {
    let project = fresh_project();
    std::fs::write(project.join("existing.txt"), "original content").unwrap();

    let tool = WriteFileTool::new(ProjectContext::new(project.clone()));

    let err = tool
        .execute(json!({
            "path": "existing.txt",
            "content": "new content"
        }))
        .await
        .expect_err("should refuse overwrite");

    assert!(matches!(err, ToolError::Rejected { .. }));
    assert!(err.to_string().contains("already exists"));

    // File should be unchanged.
    let content = std::fs::read_to_string(project.join("existing.txt")).unwrap();
    assert_eq!(content, "original content");

    let _ = std::fs::remove_dir_all(&project);
}

#[tokio::test]
async fn write_file_overwrites_with_flag() {
    let project = fresh_project();
    std::fs::write(project.join("existing.txt"), "original content").unwrap();

    let tool = WriteFileTool::new(ProjectContext::new(project.clone()));

    let result = tool
        .execute(json!({
            "path": "existing.txt",
            "content": "replaced",
            "overwrite": true
        }))
        .await
        .expect("overwrite should succeed");

    assert_eq!(result.0["overwritten"], true);

    let content = std::fs::read_to_string(project.join("existing.txt")).unwrap();
    assert_eq!(content, "replaced");

    let _ = std::fs::remove_dir_all(&project);
}

#[tokio::test]
async fn write_file_refuses_path_escape() {
    let project = fresh_project();
    let tool = WriteFileTool::new(ProjectContext::new(project.clone()));

    let err = tool
        .execute(json!({
            "path": "../../etc/passwd",
            "content": "bad"
        }))
        .await
        .expect_err("should reject path escape");

    assert!(matches!(err, ToolError::Rejected { .. }));

    let _ = std::fs::remove_dir_all(&project);
}

// ---------------------------------------------------------------------------
// edit_file
// ---------------------------------------------------------------------------

#[tokio::test]
async fn edit_file_replaces_unique_string() {
    let project = fresh_project();
    std::fs::write(
        project.join("code.rs"),
        "fn old_name() -> u32 { 42 }\nfn other() -> u32 { 99 }\n",
    )
    .unwrap();

    let tool = EditFileTool::new(ProjectContext::new(project.clone()));

    let result = tool
        .execute(json!({
            "path": "code.rs",
            "old_string": "fn old_name()",
            "new_string": "fn new_name()"
        }))
        .await
        .expect("edit should succeed");

    assert_eq!(result.0["bytes_replaced"], 13);
    assert_eq!(result.0["start_line"], 1);

    let content = std::fs::read_to_string(project.join("code.rs")).unwrap();
    assert!(content.contains("fn new_name() -> u32 { 42 }"));
    assert!(content.contains("fn other() -> u32 { 99 }"));

    let _ = std::fs::remove_dir_all(&project);
}

#[tokio::test]
async fn edit_file_rejects_nonexistent_file() {
    let project = fresh_project();
    let tool = EditFileTool::new(ProjectContext::new(project.clone()));

    let err = tool
        .execute(json!({
            "path": "missing.rs",
            "old_string": "foo",
            "new_string": "bar"
        }))
        .await
        .expect_err("should reject missing file");

    assert!(matches!(err, ToolError::Rejected { .. }));
    assert!(err.to_string().contains("does not exist"));

    let _ = std::fs::remove_dir_all(&project);
}

#[tokio::test]
async fn edit_file_rejects_ambiguous_match() {
    let project = fresh_project();
    std::fs::write(project.join("dup.rs"), "let x = 1;\nlet x = 1;\n").unwrap();

    let tool = EditFileTool::new(ProjectContext::new(project.clone()));

    let err = tool
        .execute(json!({
            "path": "dup.rs",
            "old_string": "let x = 1;",
            "new_string": "let x = 2;"
        }))
        .await
        .expect_err("should reject ambiguous match");

    assert!(matches!(err, ToolError::Rejected { .. }));
    assert!(err.to_string().contains("ambiguous"));

    let _ = std::fs::remove_dir_all(&project);
}

#[tokio::test]
async fn edit_file_rejects_string_not_found() {
    let project = fresh_project();
    std::fs::write(project.join("code.rs"), "fn foo() {}\n").unwrap();

    let tool = EditFileTool::new(ProjectContext::new(project.clone()));

    let err = tool
        .execute(json!({
            "path": "code.rs",
            "old_string": "fn bar()",
            "new_string": "fn baz()"
        }))
        .await
        .expect_err("should reject not found");

    assert!(matches!(err, ToolError::Rejected { .. }));
    assert!(err.to_string().contains("not found"));

    let _ = std::fs::remove_dir_all(&project);
}

// ---------------------------------------------------------------------------
// bash
// ---------------------------------------------------------------------------

#[tokio::test]
async fn bash_runs_simple_command() {
    let project = fresh_project();
    let tool = BashTool::new(ProjectContext::new(project.clone()));

    let result = tool
        .execute(json!({
            "command": "echo hello_world"
        }))
        .await
        .expect("bash should succeed");

    assert_eq!(result.0["exit_code"], 0);
    assert!(result.0["stdout"].as_str().unwrap().contains("hello_world"));

    let _ = std::fs::remove_dir_all(&project);
}

#[tokio::test]
async fn bash_captures_stderr() {
    let project = fresh_project();
    let tool = BashTool::new(ProjectContext::new(project.clone()));

    let result = tool
        .execute(json!({
            "command": "echo err_msg >&2"
        }))
        .await
        .expect("bash should succeed");

    assert_eq!(result.0["exit_code"], 0);
    assert!(result.0["stderr"].as_str().unwrap().contains("err_msg"));

    let _ = std::fs::remove_dir_all(&project);
}

#[tokio::test]
async fn bash_reports_nonzero_exit() {
    let project = fresh_project();
    let tool = BashTool::new(ProjectContext::new(project.clone()));

    let result = tool
        .execute(json!({
            "command": "exit 42"
        }))
        .await
        .expect("bash should succeed (the command fails, not the tool)");

    assert_eq!(result.0["exit_code"], 42);

    let _ = std::fs::remove_dir_all(&project);
}

#[tokio::test]
async fn bash_timeout_kills_command() {
    let project = fresh_project();
    let tool = BashTool::new(ProjectContext::new(project.clone()));

    let err = tool
        .execute(json!({
            "command": "sleep 10",
            "timeout_ms": 100
        }))
        .await
        .expect_err("should timeout");

    assert!(err.to_string().contains("timed out"));

    let _ = std::fs::remove_dir_all(&project);
}

// ---------------------------------------------------------------------------
// grep
// ---------------------------------------------------------------------------

#[tokio::test]
async fn grep_finds_matches() {
    let project = fresh_project();
    std::fs::write(project.join("a.rs"), "fn foo() {}\nfn bar() {}\n").unwrap();
    std::fs::write(project.join("b.rs"), "fn foo() {}\nfn baz() {}\n").unwrap();

    let tool = GrepTool::new(ProjectContext::new(project.clone()));

    let result = tool
        .execute(json!({
            "pattern": "fn foo"
        }))
        .await
        .expect("grep should succeed");

    let matches = result.0["matches"].as_array().unwrap();
    assert_eq!(matches.len(), 2);
    assert_eq!(result.0["match_count"], 2);

    let _ = std::fs::remove_dir_all(&project);
}

#[tokio::test]
async fn grep_respects_max_results() {
    let project = fresh_project();
    // Create a file with many matches.
    let content = (0..50)
        .map(|i| format!("line {i}: match_here"))
        .collect::<Vec<_>>()
        .join("\n");
    std::fs::write(project.join("big.rs"), content).unwrap();

    let tool = GrepTool::new(ProjectContext::new(project.clone()));

    let result = tool
        .execute(json!({
            "pattern": "match_here",
            "max_results": 10
        }))
        .await
        .expect("grep should succeed");

    let matches = result.0["matches"].as_array().unwrap();
    assert_eq!(matches.len(), 10);
    assert_eq!(result.0["truncated"], true);

    let _ = std::fs::remove_dir_all(&project);
}

#[tokio::test]
async fn grep_invalid_regex_returns_error() {
    let project = fresh_project();
    let tool = GrepTool::new(ProjectContext::new(project.clone()));

    let err = tool
        .execute(json!({
            "pattern": "[invalid"
        }))
        .await
        .expect_err("should reject invalid regex");

    assert!(matches!(err, ToolError::InvalidInput { .. }));

    let _ = std::fs::remove_dir_all(&project);
}

#[tokio::test]
async fn grep_skips_binary_files() {
    let project = fresh_project();
    std::fs::write(project.join("text.rs"), "fn foo() {}\n").unwrap();
    std::fs::write(project.join("binary.dat"), b"\xff\xfe\x00\x01fn foo\x00").unwrap();

    let tool = GrepTool::new(ProjectContext::new(project.clone()));

    let result = tool
        .execute(json!({
            "pattern": "fn foo"
        }))
        .await
        .expect("grep should succeed");

    let matches = result.0["matches"].as_array().unwrap();
    // Should only find the match in the text file, not the binary file.
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0]["file"], "text.rs");

    let _ = std::fs::remove_dir_all(&project);
}

// ---------------------------------------------------------------------------
// PLAN mode gate
// ---------------------------------------------------------------------------

#[test]
fn plan_mode_exposes_but_denies_write_tools() {
    let skills = Arc::new(mewcode_engine::skills::SkillRegistry::new());
    let ctx = ProjectContext::new(std::env::temp_dir());
    let data_dir = std::env::temp_dir().join(format!(
        "mewcode-plan-filter-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
    ));
    let _ = std::fs::remove_dir_all(&data_dir);
    std::fs::create_dir_all(&data_dir).unwrap();

    let store = mewcode_engine::memory::MemoryStore::new(data_dir.clone());
    let plan_reg = default_registry(ctx.clone(), skills.clone(), Some(store), Mode::Plan);
    let plan_names: Vec<&str> = plan_reg.names().into_iter().collect();

    // Read-only tools should be present.
    assert!(plan_names.contains(&"read_file"));
    assert!(plan_names.contains(&"list_directory"));
    assert!(plan_names.contains(&"glob"));
    assert!(plan_names.contains(&"grep"));
    assert!(plan_names.contains(&"skill_view"));

    // Write tools are visible so the model receives an explicit denial
    // instead of fabricating or failing provider-side, but execution is blocked.
    assert!(plan_names.contains(&"write_file"));
    assert!(plan_names.contains(&"edit_file"));
    assert!(plan_names.contains(&"bash"));
    assert!(plan_names.contains(&"mewcode_memory"));
}

#[test]
fn build_mode_includes_all_tools() {
    let skills = Arc::new(mewcode_engine::skills::SkillRegistry::new());
    let ctx = ProjectContext::new(std::env::temp_dir());

    let build_reg = default_registry(ctx, skills, None, Mode::Build);
    let build_names: Vec<&str> = build_reg.names().into_iter().collect();

    assert!(build_names.contains(&"read_file"));
    assert!(build_names.contains(&"list_directory"));
    assert!(build_names.contains(&"glob"));
    assert!(build_names.contains(&"grep"));
    assert!(build_names.contains(&"skill_view"));
    assert!(build_names.contains(&"write_file"));
    assert!(build_names.contains(&"edit_file"));
    assert!(build_names.contains(&"bash"));
    // mewcode_memory is only registered when a memory store is provided.
    assert!(!build_names.contains(&"mewcode_memory"));
}

#[tokio::test]
async fn plan_mode_dispatch_rejects_write_tool() {
    let skills = Arc::new(mewcode_engine::skills::SkillRegistry::new());
    let ctx = ProjectContext::new(fresh_project());

    let plan_reg = default_registry(ctx, skills, None, Mode::Plan);

    let output = plan_reg
        .dispatch("write_file", json!({"path": "x.txt", "content": "x"}))
        .await;

    let payload = &output.0;
    assert_eq!(payload["error"], true);
    assert_eq!(payload["kind"], "rejected");
    assert!(payload["message"].as_str().unwrap().contains("Plan mode"));
    assert!(payload["hint"].as_str().unwrap().contains("Build mode"));
}

#[tokio::test]
async fn plan_mode_bash_allows_read_only_commands() {
    let skills = Arc::new(mewcode_engine::skills::SkillRegistry::new());
    let ctx = ProjectContext::new(fresh_project());

    let plan_reg = default_registry(ctx, skills, None, Mode::Plan);
    let output = plan_reg
        .dispatch("bash", json!({"command": "pwd", "timeout_ms": 1000}))
        .await;

    assert_eq!(output.0["exit_code"], 0);
}

#[tokio::test]
async fn plan_mode_bash_rejects_mutating_commands() {
    let skills = Arc::new(mewcode_engine::skills::SkillRegistry::new());
    let ctx = ProjectContext::new(fresh_project());

    let plan_reg = default_registry(ctx, skills, None, Mode::Plan);
    let output = plan_reg
        .dispatch("bash", json!({"command": "echo hi > README.md"}))
        .await;

    assert_eq!(output.0["kind"], "rejected");
    assert!(output.0["hint"].as_str().unwrap().contains("Build mode"));
}

// ---------------------------------------------------------------------------
// Regression tests from deep review
// ---------------------------------------------------------------------------

#[tokio::test]
async fn edit_file_rejects_empty_old_string() {
    let project = fresh_project();
    std::fs::write(project.join("code.rs"), "fn foo() {}\n").unwrap();

    let tool = EditFileTool::new(ProjectContext::new(project.clone()));

    let err = tool
        .execute(json!({
            "path": "code.rs",
            "old_string": "",
            "new_string": "fn bar() {}"
        }))
        .await
        .expect_err("should reject empty old_string");

    assert!(matches!(err, ToolError::InvalidInput { .. }));
    assert!(err.to_string().contains("empty"));

    let _ = std::fs::remove_dir_all(&project);
}

#[tokio::test]
async fn bash_handles_large_output_without_deadlock() {
    // Regression test: producing >64KB of output would deadlock the old
    // implementation that called wait() before reading stdout/stderr.
    let project = fresh_project();
    let tool = BashTool::new(ProjectContext::new(project.clone()));

    // Generate ~200KB of output — exceeds the OS pipe buffer (~64KB).

    let result = tool
        .execute(json!({
            "command": "head -c 200000 /dev/zero",
            "timeout_ms": 10000
        }))
        .await
        .expect("bash should succeed with large output");

    assert_eq!(result.0["exit_code"], 0);
    let stdout = result.0["stdout"].as_str().unwrap();
    assert!(!stdout.is_empty(), "should have captured output");

    let _ = std::fs::remove_dir_all(&project);
}

#[tokio::test]
async fn bash_caps_timeout_at_max() {
    let project = fresh_project();
    let tool = BashTool::new(ProjectContext::new(project.clone()));

    // Request an absurdly large timeout — should be silently capped.
    let result = tool
        .execute(json!({
            "command": "echo ok",
            "timeout_ms": 999999999
        }))
        .await
        .expect("bash should succeed");

    assert_eq!(result.0["exit_code"], 0);

    let _ = std::fs::remove_dir_all(&project);
}

#[tokio::test]
async fn grep_truncates_long_match_lines() {
    let project = fresh_project();
    // Create a file with one very long line containing the search term.
    let long_line = format!("{} MATCH_HERE {}", "x".repeat(300), "y".repeat(100));
    std::fs::write(project.join("long.rs"), &long_line).unwrap();

    let tool = GrepTool::new(ProjectContext::new(project.clone()));

    let result = tool
        .execute(json!({
            "pattern": "MATCH_HERE"
        }))
        .await
        .expect("grep should succeed");

    let matches = result.0["matches"].as_array().unwrap();
    assert_eq!(matches.len(), 1);
    let content = matches[0]["content"].as_str().unwrap();
    assert!(
        content.len() < 300,
        "long line should be truncated — got {} chars",
        content.len()
    );
    assert!(
        content.contains("truncated"),
        "should have truncation marker"
    );

    let _ = std::fs::remove_dir_all(&project);
}

#[tokio::test]
async fn plan_mode_denies_memory_when_store_provided() {
    let skills = Arc::new(mewcode_engine::skills::SkillRegistry::new());
    let ctx = ProjectContext::new(std::env::temp_dir());
    let data_dir = std::env::temp_dir().join(format!(
        "mewcode-plan-mem-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
    ));
    let _ = std::fs::remove_dir_all(&data_dir);
    std::fs::create_dir_all(&data_dir).unwrap();

    let store = mewcode_engine::memory::MemoryStore::new(data_dir.clone());
    let plan_reg = default_registry(ctx, skills, Some(store), Mode::Plan);
    let plan_names: Vec<&str> = plan_reg.names().into_iter().collect();

    assert!(
        plan_names.contains(&"mewcode_memory"),
        "memory tool should be visible but denied in Plan mode — tools: {:?}",
        plan_names
    );
    let output = plan_reg
        .dispatch("mewcode_memory", json!({"action": "write", "content": "x"}))
        .await;
    assert_eq!(output.0["kind"], "rejected");

    let _ = std::fs::remove_dir_all(&data_dir);
}
