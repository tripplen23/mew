use mewcode_engine::harness::{user_text_with_context, user_text_with_file_context};
use mewcode_engine::skills::{SkillRegistry, SkillSource};
use mewcode_protocol::{Message, MessagePart};

#[test]
fn user_text_with_file_context_reads_at_mentions() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("a.rs"), "fn main() {}\n").unwrap();
    let msg = Message::user(vec![MessagePart::Text {
        text: "explain @a.rs".to_string(),
    }]);

    let (out, has_file_header) = user_text_with_file_context(&[msg], root.path()).unwrap();

    assert!(has_file_header);
    assert!(out.contains("@a.rs"));
    assert!(out.contains("fn main() {}"));
    assert!(out.contains("User message:\nexplain @a.rs"));
}

#[test]
fn user_text_with_context_expands_explicit_skill_invocation() {
    let root = tempfile::tempdir().unwrap();
    let skills_dir = tempfile::tempdir().unwrap();
    let skill_dir = skills_dir.path().join("caveman");
    std::fs::create_dir_all(&skill_dir).unwrap();
    std::fs::write(
        skill_dir.join("SKILL.md"),
        "---\nname: caveman\ndescription: Speak tersely. Use when asked.\n---\n\nSpeak terse.\n",
    )
    .unwrap();
    let mut skills = SkillRegistry::new();
    skills.load_dir(skills_dir.path(), SkillSource::Global);
    let msg = Message::user(vec![MessagePart::Text {
        text: "/caveman summarize this".to_string(),
    }]);

    let out = user_text_with_context(&[msg], Some(root.path()), &skills).unwrap();

    assert!(out.contains("Invoked skill: `caveman`"));
    assert!(out.contains("```skill caveman"));
    assert!(out.contains("Speak terse."));
    assert!(out.contains("User message:\n/caveman summarize this"));
}

#[test]
fn user_text_with_context_leaves_unknown_slash_as_chat() {
    let skills = SkillRegistry::new();
    let msg = Message::user(vec![MessagePart::Text {
        text: "/bogus hello".to_string(),
    }]);

    let out = user_text_with_context(&[msg], None, &skills).unwrap();

    assert_eq!(out, "/bogus hello");
}

fn load_test_skill() -> SkillRegistry {
    let skills_dir = tempfile::tempdir().unwrap();
    let skill_dir = skills_dir.path().join("caveman");
    std::fs::create_dir_all(&skill_dir).unwrap();
    std::fs::write(
        skill_dir.join("SKILL.md"),
        "---\nname: caveman\ndescription: Speak tersely. Use when asked.\n---\n\nSpeak terse.\n\n```\ncode sample\n```\n",
    )
    .unwrap();
    let mut skills = SkillRegistry::new();
    skills.load_dir(skills_dir.path(), SkillSource::Global);
    skills
}

#[test]
fn user_text_with_context_skill_plus_mention_emits_single_user_header() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("a.rs"), "fn main() {}\n").unwrap();
    let skills = load_test_skill();
    let msg = Message::user(vec![MessagePart::Text {
        text: "/caveman summarize @a.rs".to_string(),
    }]);

    let out = user_text_with_context(&[msg], Some(root.path()), &skills).unwrap();

    assert_eq!(out.matches("User message:").count(), 1);
    assert!(out.contains("Referenced files:"));
    assert!(out.contains("fn main() {}"));
    assert!(out.ends_with("/caveman summarize @a.rs"));
}

#[test]
fn user_text_with_context_fences_skill_body_that_contains_fences() {
    let skills = load_test_skill();
    let msg = Message::user(vec![MessagePart::Text {
        text: "/caveman summarize this".to_string(),
    }]);

    let out = user_text_with_context(&[msg], None, &skills).unwrap();

    assert!(out.contains("````skill caveman"));
    assert!(out.contains("```\ncode sample\n```"));
}

#[test]
fn user_text_with_header_string_in_plain_text_still_emits_header() {
    let skills = load_test_skill();
    let msg = Message::user(vec![MessagePart::Text {
        text: "/caveman paste this: User message: hello".to_string(),
    }]);

    let out = user_text_with_context(&[msg], None, &skills).unwrap();

    assert_eq!(out.matches("User message:").count(), 2);
    assert!(out.ends_with("/caveman paste this: User message: hello"));
}

#[test]
fn user_text_with_context_skips_model_only_skill() {
    let skills_dir = tempfile::tempdir().unwrap();
    let skill_dir = skills_dir.path().join("internal");
    std::fs::create_dir_all(&skill_dir).unwrap();
    std::fs::write(
        skill_dir.join("SKILL.md"),
        "---\nname: internal\ndescription: Model only.\nuser-invocable: false\n---\n\nInternal body.\n",
    )
    .unwrap();
    let mut skills = SkillRegistry::new();
    skills.load_dir(skills_dir.path(), SkillSource::Global);
    let msg = Message::user(vec![MessagePart::Text {
        text: "/internal summarize this".to_string(),
    }]);

    let out = user_text_with_context(&[msg], None, &skills).unwrap();

    assert_eq!(out, "/internal summarize this");
}
