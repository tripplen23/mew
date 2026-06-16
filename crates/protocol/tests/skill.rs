//! Integration tests for `mewcode_protocol::skill`.

use std::path::Path;

use mewcode_protocol::{Skill, SkillError, parse_skill_md};

const SAMPLE: &str = r#"---
name: review-pr
description: Review a pull request for correctness, style, and tests.
---

# How to review a PR

1. Read the diff carefully.
2. Flag missing tests.
3. Be polite.
"#;

#[test]
fn parses_frontmatter_and_body() {
    let skill = parse_skill_md(SAMPLE, Path::new("SKILL.md")).unwrap();
    assert_eq!(skill.name, "review-pr");
    assert!(skill.description.contains("Review a pull request"));
    assert!(skill.body.contains("How to review a PR"));
    assert!(skill.body.contains("Be polite"));
}

#[test]
fn missing_frontmatter_is_error() {
    let err = parse_skill_md("no frontmatter here", Path::new("SKILL.md")).unwrap_err();
    assert!(matches!(err, SkillError::MalformedFrontmatter { .. }));
}

#[test]
fn missing_name_is_error() {
    let raw = "---\ndescription: foo\n---\nbody\n";
    let err = parse_skill_md(raw, Path::new("SKILL.md")).unwrap_err();
    assert!(matches!(
        err,
        SkillError::MissingField { field: "name", .. }
    ));
}

#[test]
fn catalog_entry_includes_name_and_description() {
    let skill: Skill = parse_skill_md(SAMPLE, Path::new("SKILL.md")).unwrap();
    let entry = skill.catalog_entry();
    assert!(entry.contains("review-pr"));
    assert!(entry.contains("Review a pull request"));
    assert!(entry.contains("use_skill"));
}

#[test]
fn handles_bom_at_start() {
    // not a perfect fixture, but the BOM-strip should not panic
    let _ = parse_skill_md("\u{feff}---name: x\n---\nbody", Path::new("SKILL.md"));
}
