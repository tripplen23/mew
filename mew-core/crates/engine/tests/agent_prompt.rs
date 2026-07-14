//! Integration tests for the system-prompt builder.
//!
//! These tests exercise `build_system_prompt` through the public API
//! the way a real agent harness would, so they catch accidental
//! changes to the prompt shape, mode gating, skill injection, and
//! tool-descriptor rendering.

use std::fs;
use std::sync::Arc;

use mewcode_engine::agent::build_system_prompt;
use mewcode_engine::skills::{SkillRegistry, SkillSource};
use mewcode_engine::tools::{
    GlobTool, ListDirectoryTool, ProjectContext, ReadFileTool, SkillViewTool, ToolRegistry,
    default_registry,
};
use mewcode_protocol::Mode;

#[test]
fn build_mode_includes_tool_descriptors() {
    let skills = Arc::new(SkillRegistry::new());
    let skills_for_registry = skills.clone();
    let tools = Arc::new(default_registry(
        ProjectContext::new(std::env::temp_dir()),
        skills_for_registry,
        None,
        Mode::Build,
    ));
    let prompt = build_system_prompt(Mode::Build, &skills, &tools);
    // Build mode registry: read_file, list_directory, glob, grep, skill_view,
    // write_file, edit_file, bash, mewcode_memory.
    assert!(prompt.contains("### `read_file`"));
    assert!(prompt.contains("### `list_directory`"));
    assert!(prompt.contains("### `glob`"));
    assert!(prompt.contains("### `grep`"));
    assert!(prompt.contains("### `skill_view`"));
    assert!(prompt.contains("### `write_file`"));
    assert!(prompt.contains("### `edit_file`"));
    assert!(prompt.contains("### `bash`"));
}

#[test]
fn plan_mode_excludes_write_tool_descriptors() {
    let skills = Arc::new(SkillRegistry::new());
    let ctx = ProjectContext::new(std::env::temp_dir());
    let mut tools = ToolRegistry::new();
    // Only register the read-only tools + skill_view (the PLAN set).
    tools.register(Arc::new(ReadFileTool::new(ctx.clone())));
    tools.register(Arc::new(ListDirectoryTool::new(ctx.clone())));
    tools.register(Arc::new(GlobTool::new(ctx)));
    tools.register(Arc::new(SkillViewTool::new(skills.clone())));

    let prompt = build_system_prompt(Mode::Plan, &skills, &tools);
    assert!(prompt.contains("### `read_file`"));
    assert!(!prompt.contains("### `write_file`"));
    assert!(!prompt.contains("### `edit_file`"));
}

#[test]
fn skills_are_injected_when_present() {
    let tmp = std::env::temp_dir().join(format!("mewcode-prompt-test-{}", std::process::id()));
    let _ = fs::remove_dir_all(&tmp);
    fs::create_dir_all(tmp.join("review-pr")).unwrap();
    fs::write(
        tmp.join("review-pr/SKILL.md"),
        "---\nname: review-pr\ndescription: Review a PR.\n---\nbody",
    )
    .unwrap();

    let mut skills = SkillRegistry::new();
    skills.load_dir(&tmp, SkillSource::Global);
    let tools = ToolRegistry::new();

    let prompt = build_system_prompt(Mode::Build, &skills, &tools);
    assert!(prompt.contains("**review-pr**"));
    assert!(prompt.contains("Review a PR."));
    assert!(prompt.contains("Available skills"));
}

#[test]
fn no_skills_means_no_catalog_section() {
    let skills = SkillRegistry::new();
    let tools = ToolRegistry::new();
    let prompt = build_system_prompt(Mode::Build, &skills, &tools);
    assert!(!prompt.contains("Available skills"));
}

#[test]
fn tool_descriptors_are_injected_when_present() {
    let skills = SkillRegistry::new();
    let mut tools = ToolRegistry::new();
    tools.register(Arc::new(ReadFileTool::new(ProjectContext::new(
        std::env::temp_dir(),
    ))));

    let prompt = build_system_prompt(Mode::Build, &skills, &tools);
    assert!(prompt.contains("## Tool reference"));
    assert!(prompt.contains("### `read_file`"));
    assert!(prompt.contains("**Safety:** read-only, idempotent"));
    assert!(prompt.contains("**Input schema:**"));
    assert!(prompt.contains("\"path\""));
    assert!(prompt.contains("**Examples:**"));
}

#[test]
fn empty_registry_yields_no_tool_block() {
    let skills = SkillRegistry::new();
    let tools = ToolRegistry::new();
    let prompt = build_system_prompt(Mode::Build, &skills, &tools);
    assert!(!prompt.contains("## Tool reference"));
}

#[test]
fn tools_are_sorted_alphabetically() {
    let skills = SkillRegistry::new();
    let ctx = ProjectContext::new(std::env::temp_dir());
    let skills_arc = Arc::new(SkillRegistry::new());
    let mut tools = ToolRegistry::new();
    // Register in non-alphabetical order to prove the sort works.
    tools.register(Arc::new(SkillViewTool::new(skills_arc)));
    tools.register(Arc::new(ReadFileTool::new(ctx.clone())));
    tools.register(Arc::new(GlobTool::new(ctx.clone())));
    tools.register(Arc::new(ListDirectoryTool::new(ctx)));

    let prompt = build_system_prompt(Mode::Build, &skills, &tools);
    let glob_pos = prompt.find("### `glob`").unwrap();
    let list_pos = prompt.find("### `list_directory`").unwrap();
    let read_pos = prompt.find("### `read_file`").unwrap();
    let use_pos = prompt.find("### `skill_view`").unwrap();
    assert!(glob_pos < list_pos);
    assert!(list_pos < read_pos);
    assert!(read_pos < use_pos);
}
