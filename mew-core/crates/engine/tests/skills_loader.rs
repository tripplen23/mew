//! Integration tests for the skill loader and registry.
//!
//! Exercises the public API of `SkillRegistry` — load skills from
//! disk, look them up, render the catalog, resolve bodies. The
//! `TempDir` helper is inlined here because external tests cannot
//! share private items with the crate.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use mewcode_engine::skills::{MAX_CATALOG_DESCRIPTION_CHARS, SkillRegistry, SkillSource};
use mewcode_engine::tools::SkillViewTool;
use mewcode_protocol::ToolContracts;
use serde_json::json;

static COUNTER: AtomicUsize = AtomicUsize::new(0);

struct TempDir(PathBuf);
impl TempDir {
    fn path(&self) -> &Path {
        &self.0
    }
}
impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn tempdir() -> TempDir {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    let path = PathBuf::from(format!("/tmp/mewcode-skills-test-{pid}-{n}"));
    std::fs::create_dir_all(&path).unwrap();
    TempDir(path)
}

fn write_skill(dir: &Path, name: &str, description: &str) {
    write_skill_frontmatter(dir, name, description, "");
}

fn write_skill_frontmatter(dir: &Path, name: &str, description: &str, extra: &str) {
    let skill_dir = dir.join(name);
    fs::create_dir_all(&skill_dir).unwrap();
    let body = format!(
        "---\nname: {name}\ndescription: {description}\n{extra}---\n\n# {name}\n\nDo the {name} thing.\n"
    );
    fs::write(skill_dir.join("SKILL.md"), body).unwrap();
}

#[test]
fn loads_skills_from_directory() {
    let tmp = tempdir();
    write_skill(tmp.path(), "review-pr", "Review a pull request.");
    write_skill(tmp.path(), "write-migration", "Write a SQL migration.");

    let mut reg = SkillRegistry::new();
    reg.load_dir(tmp.path(), SkillSource::Global);

    assert_eq!(reg.len(), 2);
    let entry = reg.get("review-pr").unwrap();
    assert_eq!(entry.skill.name, "review-pr");
    assert!(entry.skill.body.contains("Do the review-pr thing."));
    assert_eq!(entry.source, SkillSource::Global);
}

#[test]
fn project_overrides_global() {
    let tmp = tempdir();
    let global = tmp.path().join("global");
    let project = tmp.path().join("project");
    fs::create_dir_all(&global).unwrap();
    fs::create_dir_all(&project).unwrap();
    write_skill(&global, "review-pr", "GLOBAL description.");
    write_skill(&project, "review-pr", "PROJECT description.");

    let mut reg = SkillRegistry::new();
    reg.load_dir(&global, SkillSource::Global);
    reg.load_dir(&project, SkillSource::Project);

    let entry = reg.get("review-pr").unwrap();
    assert!(entry.skill.description.contains("PROJECT"));
    assert_eq!(entry.source, SkillSource::Project);
}

#[test]
fn catalog_lists_every_skill() {
    let tmp = tempdir();
    write_skill(tmp.path(), "alpha", "First skill.");
    write_skill(tmp.path(), "beta", "Second skill.");

    let mut reg = SkillRegistry::new();
    reg.load_dir(tmp.path(), SkillSource::Global);

    let cat = reg.catalog_for_system_prompt();
    assert!(cat.contains("**alpha**"));
    assert!(cat.contains("**beta**"));
    assert!(cat.contains("First skill."));
    assert!(cat.contains("Second skill."));
    assert!(cat.contains("skill_view"));
}

#[test]
fn empty_catalog_returns_empty_string() {
    let reg = SkillRegistry::new();
    assert_eq!(reg.catalog_for_system_prompt(), "");
}

#[test]
fn missing_directory_is_recorded() {
    let tmp = tempdir();
    let nope = tmp.path().join("does-not-exist");
    let mut reg = SkillRegistry::new();
    reg.load_dir(&nope, SkillSource::Global);
    assert_eq!(reg.len(), 0);
    assert_eq!(reg.missing_paths(), &[nope]);
}

#[test]
fn view_body_returns_full_prompt() {
    let tmp = tempdir();
    write_skill(tmp.path(), "x", "desc");
    let mut reg = SkillRegistry::new();
    reg.load_dir(tmp.path(), SkillSource::Global);

    let body = reg.view_body("x").unwrap();
    assert!(body.contains("# x"));
}

#[test]
fn view_body_missing_skill_returns_not_found() {
    let reg = SkillRegistry::new();
    let err = reg.view_body("does-not-exist").expect_err("missing");
    assert!(matches!(err, mewcode_protocol::SkillError::NotFound { .. }));
}

#[test]
fn disable_model_invocation_skill_hidden_from_model_facing_surfaces() {
    let tmp = tempdir();
    write_skill(tmp.path(), "alpha", "A normal skill.");
    write_skill_frontmatter(
        tmp.path(),
        "secret",
        "A user-only skill.",
        "disable-model-invocation: true\n",
    );

    let mut reg = SkillRegistry::new();
    reg.load_dir(tmp.path(), SkillSource::Global);
    assert_eq!(reg.len(), 2);

    let cat = reg.catalog_for_system_prompt();
    assert!(cat.contains("**alpha**"));
    assert!(!cat.contains("**secret**"));

    let tool_names: Vec<_> = reg.list_for_tool().into_iter().map(|e| e.name).collect();
    assert_eq!(tool_names, vec!["alpha"]);

    let user_names: Vec<_> = reg.list_for_user().into_iter().map(|e| e.name).collect();
    assert_eq!(user_names, vec!["alpha", "secret"]);
}

#[test]
fn user_invocable_false_skill_hidden_from_user_picker() {
    let tmp = tempdir();
    write_skill(tmp.path(), "alpha", "A normal skill.");
    write_skill_frontmatter(
        tmp.path(),
        "internal",
        "A model-only skill.",
        "user-invocable: false\n",
    );

    let mut reg = SkillRegistry::new();
    reg.load_dir(tmp.path(), SkillSource::Global);

    let cat = reg.catalog_for_system_prompt();
    assert!(cat.contains("**alpha**"));
    assert!(cat.contains("**internal**"));

    let tool_names: Vec<_> = reg.list_for_tool().into_iter().map(|e| e.name).collect();
    assert_eq!(tool_names, vec!["alpha", "internal"]);

    let user_names: Vec<_> = reg.list_for_user().into_iter().map(|e| e.name).collect();
    assert_eq!(user_names, vec!["alpha"]);
}

#[test]
fn catalog_truncates_descriptions_and_omits_skills_over_budget() {
    let tmp = tempdir();
    let desc = "x".repeat(400);
    for i in 0..80 {
        write_skill(tmp.path(), &format!("skill-{i:02}"), &desc);
    }

    let mut reg = SkillRegistry::new();
    reg.load_dir(tmp.path(), SkillSource::Global);

    let cat = reg.catalog_for_system_prompt();
    assert!(cat.contains('…'), "descriptions should be truncated");
    assert!(
        cat.contains("more skills installed"),
        "over-budget catalog should warn"
    );
    assert!(cat.ends_with("</skills>\n"));
    assert!(
        cat.len() < 8_200,
        "catalog ({}) should stay near the 8k budget",
        cat.len()
    );
    assert!(!cat.contains("skill-79"), "over-budget skills are dropped");
    let kept: Vec<_> = (0..80)
        .filter(|i| cat.contains(&format!("skill-{i:02}")))
        .collect();
    assert!(
        kept.len() < 80 && kept.len() >= 50,
        "kept {} of 80",
        kept.len()
    );
    assert!(
        !cat.contains(&"x".repeat(MAX_CATALOG_DESCRIPTION_CHARS + 50)),
        "truncated descriptions must not keep the full tail"
    );
}

#[tokio::test]
async fn skill_view_rejects_model_invocation_only_skill() {
    let tmp = tempdir();
    write_skill_frontmatter(
        tmp.path(),
        "secret",
        "A user-only skill.",
        "disable-model-invocation: true\n",
    );

    let mut reg = SkillRegistry::new();
    reg.load_dir(tmp.path(), SkillSource::Global);
    let tool = SkillViewTool::new(Arc::new(reg));

    let out = tool.execute(json!({ "name": "secret" })).await;
    let err = out.expect_err("model must be rejected");
    assert!(
        err.to_string().contains("user-invocable only"),
        "unexpected error: {err}"
    );

    let ok = tool.execute(json!({ "name": "does-not-exist" })).await;
    assert!(ok.is_err(), "missing skill stays not-found");
}
