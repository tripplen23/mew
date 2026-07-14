//! Integration tests for `SkillRegistry::load(&SkillLoadConfig)` and
//! the L0/L1/L2 read APIs (`list_for_tool`, `view_subfile`).
//!
//! These cover the precedence rules (bundled < external < project),
//! the silent-skip contract for missing seed paths, and the L2
//! sub-file safety checks.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use mewcode_engine::skills::{SkillLoadConfig, SkillRegistry, SkillSource};
use mewcode_protocol::{PROJECT_SKILLS_DIR, SkillError};

static COUNTER: AtomicU64 = AtomicU64::new(0);

fn fresh_dir(label: &str) -> PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::SeqCst);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("mewcode-skills-{label}-{n}-{nanos}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn write_skill(parent: &Path, name: &str, description: &str) -> PathBuf {
    let dir = parent.join(name);
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("SKILL.md"),
        format!("---\nname: {name}\ndescription: {description}\n---\n\nbody of {name}\n"),
    )
    .unwrap();
    dir
}

#[test]
fn load_discovers_project_skills() {
    let project = fresh_dir("project");
    let project_skills = project.join(PROJECT_SKILLS_DIR);
    std::fs::create_dir_all(&project_skills).unwrap();
    write_skill(&project_skills, "alpha", "first skill");
    write_skill(&project_skills, "beta", "second skill");

    let cfg = SkillLoadConfig {
        bundled_dir: None,
        external_dirs: vec![],
        project_search_start: Some(project),
        include_dev_dir: false,
    };
    let reg = SkillRegistry::load(&cfg);
    assert_eq!(reg.len(), 2);
    assert!(reg.get("alpha").is_some());
    assert!(reg.get("beta").is_some());
}

#[test]
fn project_shadows_external() {
    let external = fresh_dir("external");
    let project = fresh_dir("project");
    write_skill(&external, "shared", "from external");

    let project_skills = project.join(PROJECT_SKILLS_DIR);
    std::fs::create_dir_all(&project_skills).unwrap();
    write_skill(&project_skills, "shared", "from project");

    let cfg = SkillLoadConfig {
        bundled_dir: None,
        external_dirs: vec![external],
        project_search_start: Some(project),
        include_dev_dir: false,
    };
    let reg = SkillRegistry::load(&cfg);
    let loaded = reg.get("shared").unwrap();
    assert_eq!(loaded.skill.description, "from project");
    assert_eq!(loaded.source, SkillSource::Project);
}

#[test]
fn external_shadows_bundled() {
    let bundled = fresh_dir("bundled");
    let external = fresh_dir("external");
    write_skill(&bundled, "shared", "from bundled");
    write_skill(&external, "shared", "from external");

    let cfg = SkillLoadConfig {
        bundled_dir: Some(bundled),
        external_dirs: vec![external],
        project_search_start: None,
        include_dev_dir: false,
    };
    let reg = SkillRegistry::load(&cfg);
    let loaded = reg.get("shared").unwrap();
    assert_eq!(loaded.skill.description, "from external");
    assert_eq!(loaded.source, SkillSource::External);
}

#[test]
fn project_shadows_global() {
    // `~/.config/mewcode/skills` is the global install; the
    // project dir shadows it. We can't easily mock the home
    // directory, so this test uses `external_dirs` as a stand-in
    // for global and asserts the project-shadows-earlier
    // contract holds. (CodeRabbit review on PR #11.)
    let external = fresh_dir("global-stand-in");
    let project = fresh_dir("project");
    write_skill(&external, "shared", "from global");
    let project_skills = project.join(PROJECT_SKILLS_DIR);
    std::fs::create_dir_all(&project_skills).unwrap();
    write_skill(&project_skills, "shared", "from project");

    let cfg = SkillLoadConfig {
        bundled_dir: None,
        // External loaded first, so project (loaded later) should win.
        external_dirs: vec![external],
        project_search_start: Some(project),
        include_dev_dir: false,
    };
    let reg = SkillRegistry::load(&cfg);
    let loaded = reg.get("shared").unwrap();
    assert_eq!(loaded.skill.description, "from project");
    assert_eq!(loaded.source, SkillSource::Project);
}

#[test]
fn missing_paths_are_silently_skipped() {
    let bundled = PathBuf::from("/this/path/does/not/exist/anywhere");
    let external = PathBuf::from("/also/not/here");
    let cfg = SkillLoadConfig {
        bundled_dir: Some(bundled.clone()),
        external_dirs: vec![external.clone()],
        project_search_start: None,
        include_dev_dir: false,
    };
    let reg = SkillRegistry::load(&cfg);
    // No crash; registry is empty.
    assert!(reg.is_empty());
    // The contract: every seed path we couldn't find is in
    // `missing_paths`. The test stays deterministic even if the
    // dev machine happens to have `~/.config/mewcode/skills`.
    let missing = reg.missing_paths();
    assert!(
        missing.contains(&bundled),
        "expected {bundled:?} in missing_paths, got {missing:?}"
    );
    assert!(
        missing.contains(&external),
        "expected {external:?} in missing_paths, got {missing:?}"
    );
}

#[test]
fn view_subfile_returns_subfile() {
    let project = fresh_dir("subfile");
    let project_skills = project.join(PROJECT_SKILLS_DIR);
    std::fs::create_dir_all(&project_skills).unwrap();
    let dir = write_skill(&project_skills, "alpha", "alpha skill");
    let refs = dir.join("references");
    fs::create_dir_all(&refs).unwrap();
    fs::write(refs.join("checklist.md"), "- step 1\n- step 2\n").unwrap();

    let cfg = SkillLoadConfig {
        bundled_dir: None,
        external_dirs: vec![],
        project_search_start: Some(project),
        include_dev_dir: false,
    };
    let reg = SkillRegistry::load(&cfg);

    let (path, content) = reg
        .view_subfile("alpha", "references/checklist.md")
        .unwrap();
    assert_eq!(path, PathBuf::from("references/checklist.md"));
    assert!(content.contains("step 1"));
}

#[test]
fn view_subfile_rejects_traversal() {
    let project = fresh_dir("traverse");
    let project_skills = project.join(PROJECT_SKILLS_DIR);
    std::fs::create_dir_all(&project_skills).unwrap();
    write_skill(&project_skills, "alpha", "alpha skill");

    let cfg = SkillLoadConfig {
        bundled_dir: None,
        external_dirs: vec![],
        project_search_start: Some(project),
        include_dev_dir: false,
    };
    let reg = SkillRegistry::load(&cfg);

    let err = reg
        .view_subfile("alpha", "../escape.md")
        .expect_err("must reject ..");
    assert!(matches!(err, SkillError::InvalidSubpath { .. }));
}

#[test]
fn view_subfile_rejects_skill_md() {
    // SKILL.md is the L1 body; reading it as an L2 sub-file
    // would bypass the response budget. (CodeRabbit review.)
    let project = fresh_dir("skillmd");
    let project_skills = project.join(PROJECT_SKILLS_DIR);
    std::fs::create_dir_all(&project_skills).unwrap();
    write_skill(&project_skills, "alpha", "alpha skill");

    let cfg = SkillLoadConfig {
        bundled_dir: None,
        external_dirs: vec![],
        project_search_start: Some(project),
        include_dev_dir: false,
    };
    let reg = SkillRegistry::load(&cfg);

    let err = reg
        .view_subfile("alpha", "SKILL.md")
        .expect_err("SKILL.md must be rejected as sub-file");
    match err {
        SkillError::InvalidSubpath { reason, .. } => {
            assert!(
                reason.contains("L1"),
                "reason should mention L1; got {reason}"
            );
        }
        other => panic!("expected InvalidSubpath, got {other:?}"),
    }
}

#[test]
fn view_subfile_rejects_dot_prefixed_skill_md() {
    // `./SKILL.md` and `.//SKILL.md` must be rejected like
    // `SKILL.md` — stripping the leading `.` would let the
    // request reach the L2 read and bypass the L1 response
    // budget.
    let project = fresh_dir("dot-skillmd");
    let project_skills = project.join(PROJECT_SKILLS_DIR);
    std::fs::create_dir_all(&project_skills).unwrap();
    write_skill(&project_skills, "alpha", "alpha skill");

    let cfg = SkillLoadConfig {
        bundled_dir: None,
        external_dirs: vec![],
        project_search_start: Some(project),
        include_dev_dir: false,
    };
    let reg = SkillRegistry::load(&cfg);

    for variant in ["./SKILL.md", ".//SKILL.md"] {
        let err = reg
            .view_subfile("alpha", variant)
            .expect_err("dot-prefixed SKILL.md must be rejected as sub-file");
        match err {
            SkillError::InvalidSubpath { reason, .. } => {
                assert!(
                    reason.contains("L1"),
                    "reason should mention L1 for {variant}; got {reason}"
                );
            }
            other => panic!("expected InvalidSubpath for {variant}, got {other:?}"),
        }
    }
}

#[test]
fn list_for_tool_returns_sorted_entries() {
    let project = fresh_dir("list");
    let project_skills = project.join(PROJECT_SKILLS_DIR);
    std::fs::create_dir_all(&project_skills).unwrap();
    write_skill(&project_skills, "zulu", "z");
    write_skill(&project_skills, "alpha", "a");

    let cfg = SkillLoadConfig {
        bundled_dir: None,
        external_dirs: vec![],
        project_search_start: Some(project),
        include_dev_dir: false,
    };
    let reg = SkillRegistry::load(&cfg);
    let entries = reg.list_for_tool();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].name, "alpha");
    assert_eq!(entries[1].name, "zulu");
}

#[test]
fn list_for_tool_assets_keep_subdir_prefix() {
    // CodeRabbit review (PR #11): the recursive asset walker
    // used to strip the conventional subdir prefix, so
    // `references/checklist.md` was advertised as `checklist.md`.
    // After the fix, paths stay relative to the skill root.
    let project = fresh_dir("assets");
    let project_skills = project.join(PROJECT_SKILLS_DIR);
    std::fs::create_dir_all(&project_skills).unwrap();
    let dir = write_skill(&project_skills, "alpha", "alpha skill");
    let refs = dir.join("references");
    fs::create_dir_all(&refs).unwrap();
    fs::write(refs.join("checklist.md"), "- step 1\n").unwrap();
    let templates = dir.join("templates");
    fs::create_dir_all(&templates).unwrap();
    fs::write(templates.join("comment.md"), "> template\n").unwrap();

    let cfg = SkillLoadConfig {
        bundled_dir: None,
        external_dirs: vec![],
        project_search_start: Some(project),
        include_dev_dir: false,
    };
    let reg = SkillRegistry::load(&cfg);
    let entry = reg.list_for_tool().into_iter().next().unwrap();
    assert!(
        entry.assets.iter().any(|a| a == "references/checklist.md"),
        "expected `references/checklist.md`, got {:?}",
        entry.assets
    );
    assert!(
        entry.assets.iter().any(|a| a == "templates/comment.md"),
        "expected `templates/comment.md`, got {:?}",
        entry.assets
    );
}
