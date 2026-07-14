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
    assert!(formatted.starts_with("# Memory"));
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
