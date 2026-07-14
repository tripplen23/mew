//! Smoke test that the skills bundled with the repo load correctly
//! via the default loader.
//!
//! This is `#[ignore]`'d by default because it depends on the working directory.
//! Run with `cargo test -p mewcode-engine --test skill_smoke -- --ignored --nocapture`.

use mewcode_engine::skills::SkillRegistry;

#[test]
#[ignore]
fn bundled_skills_load() {
    let cwd = std::env::current_dir().unwrap();
    println!("cwd: {}", cwd.display());

    let reg = SkillRegistry::load_defaults();
    println!("loaded {} skills", reg.len());
    for s in reg.skills() {
        println!(
            "  - {} ({:?}): {}",
            s.skill.name, s.source, s.skill.description
        );
    }
    assert!(
        !reg.is_empty(),
        "expected to find at least one bundled skill"
    );
    assert!(
        reg.get("review-pr").is_some(),
        "expected the `review-pr` skill"
    );
    assert!(
        reg.get("write-rust-error").is_some(),
        "expected the `write-rust-error` skill"
    );
}
