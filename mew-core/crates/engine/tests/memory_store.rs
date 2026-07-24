//! Tests for the durable memory store.

use std::path::PathBuf;

use mewcode_engine::memory::MemoryStore;

#[test]
fn empty_memory_returns_none_from_format() {
    let dir = tempdir();
    let store = MemoryStore::new(dir);
    assert!(store.format().is_none());
}

#[test]
fn write_then_read_round_trips() {
    let dir = tempdir();
    let store = MemoryStore::new(dir.clone());

    store.write("User prefers concise responses.\n").unwrap();
    let content = MemoryStore::new(dir).read();
    assert_eq!(content.trim(), "User prefers concise responses.");
}

#[test]
fn write_then_format_includes_memory_heading() {
    let dir = tempdir();
    let store = MemoryStore::new(dir);

    store.write("User prefers concise responses.").unwrap();
    let formatted = store.format().unwrap();
    assert!(formatted.starts_with("<memory>"));
    assert!(formatted.contains("User prefers concise responses."));
}

#[test]
fn with_profile_uses_correct_path() {
    let dir = tempdir();
    let store = MemoryStore::with_profile(dir.clone(), "work");
    let expected = dir.join("memories").join("work.md");
    assert_eq!(store.path(), &expected);
}

#[test]
fn default_profile_path_is_correct() {
    let dir = tempdir();
    let store = MemoryStore::new(dir.clone());
    let expected = dir.join("memories").join("default.md");
    assert_eq!(store.path(), &expected);
}

/// Create a unique temporary directory path for each test call.
/// The directory is created lazily on first write by MemoryStore and
/// left for OS temp cleanup.
fn tempdir() -> PathBuf {
    let p = std::env::temp_dir().join(format!("mew-mem-test-{}", uuid::Uuid::new_v4()));
    let _ = std::fs::remove_dir_all(&p);
    p
}

#[test]
fn for_project_scopes_different_projects_to_different_files() {
    let data_dir = tempdir();
    let project_a = tempdir();
    let project_b = tempdir();
    std::fs::create_dir_all(&project_a).unwrap();
    std::fs::create_dir_all(&project_b).unwrap();

    let store_a = MemoryStore::for_project(data_dir.clone(), &project_a);
    let store_b = MemoryStore::for_project(data_dir.clone(), &project_b);

    assert_ne!(
        store_a.path(),
        store_b.path(),
        "two distinct projects must not share a memory file"
    );

    store_a.write("fact about project A").unwrap();
    store_b.write("fact about project B").unwrap();

    // Round-trip: each project only ever sees its own file.
    assert_eq!(store_a.read().trim(), "fact about project A");
    assert_eq!(store_b.read().trim(), "fact about project B");
}

#[test]
fn for_project_is_stable_across_calls() {
    let data_dir = tempdir();
    let project = tempdir();
    std::fs::create_dir_all(&project).unwrap();

    let store_1 = MemoryStore::for_project(data_dir.clone(), &project);
    let store_2 = MemoryStore::for_project(data_dir.clone(), &project);

    assert_eq!(
        store_1.path(),
        store_2.path(),
        "the same project root must always resolve to the same memory file"
    );
}

#[test]
fn for_project_hashes_raw_path_when_canonicalize_fails() {
    let data_dir = tempdir();
    // A project root that does not exist on disk cannot be canonicalized.
    // The path is hashed to a unique profile instead of falling back to
    // "default", preventing cross-project memory leakage.
    let nonexistent = data_dir.join("this-does-not-exist");

    let store = MemoryStore::for_project(data_dir.clone(), &nonexistent);
    let file_name = store.path().file_name().unwrap().to_str().unwrap();
    assert!(
        file_name.starts_with("unresolved-"),
        "must use hashed name for unresolved paths, got {file_name}"
    );
    assert!(file_name.ends_with(".md"));
}

#[test]
fn for_project_sanitizes_directory_names_that_look_like_tool_paths() {
    // Two differently-named-but-similar project roots must not collapse
    // into the same profile just because sanitizing turns unusual
    // characters into `-`.
    let data_dir = tempdir();
    let weird_name_a = data_dir.join("weird project!!");
    let weird_name_b = data_dir.join("weird project??");
    std::fs::create_dir_all(&weird_name_a).unwrap();
    std::fs::create_dir_all(&weird_name_b).unwrap();

    let store_a = MemoryStore::for_project(data_dir.clone(), &weird_name_a);
    let store_b = MemoryStore::for_project(data_dir.clone(), &weird_name_b);

    // The profile name itself must be a bare filename with no path
    // separators, `..`, or leading dot — the same constraints the
    // `mewcode_memory` tool enforces on user-supplied profile names.
    let profile_a = store_a
        .path()
        .file_stem()
        .unwrap()
        .to_string_lossy()
        .to_string();
    assert!(!profile_a.contains('/'));
    assert!(!profile_a.contains('\\'));
    assert!(!profile_a.contains(".."));
    assert!(!profile_a.starts_with('.'));

    // Different canonical paths still get different memory files even
    // after sanitizing, because the hash suffix differs.
    assert_ne!(store_a.path(), store_b.path());
}

#[test]
fn for_project_data_dir_round_trips() {
    let data_dir = tempdir();
    let project = tempdir();
    std::fs::create_dir_all(&project).unwrap();

    let store = MemoryStore::for_project(data_dir.clone(), &project);
    assert_eq!(store.data_dir(), Some(data_dir.as_path()));
}
