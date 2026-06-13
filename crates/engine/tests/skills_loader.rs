//! Integration tests for the skill loader and registry.
//!
//! Exercises the public API of `SkillRegistry` — load skills from
//! disk, look them up, render the catalog, resolve bodies. The
//! `TempDir` helper is inlined here because external tests cannot
//! share private items with the crate.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use mewcode_engine::skills::{SkillRegistry, SkillSource};

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
    let skill_dir = dir.join(name);
    fs::create_dir_all(&skill_dir).unwrap();
    let body = format!(
        "---\nname: {name}\ndescription: {description}\n---\n\n# {name}\n\nDo the {name} thing.\n"
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
    assert!(cat.contains("use_skill"));
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
fn resolve_body_returns_full_prompt() {
    let tmp = tempdir();
    write_skill(tmp.path(), "x", "desc");
    let mut reg = SkillRegistry::new();
    reg.load_dir(tmp.path(), SkillSource::Global);

    let body = reg.resolve_body("x").unwrap();
    assert!(body.contains("# x"));
}
