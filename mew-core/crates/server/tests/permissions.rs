//! Tests for the persistent always-allow permissions store.

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use mewcode_server::permission::PermissionStore;

static COUNTER: AtomicUsize = AtomicUsize::new(0);

fn fresh_dir() -> PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("mewcode-perm-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn missing_file_loads_empty_and_corrupt_loads_empty() {
    let dir = fresh_dir();
    let store = PermissionStore::load_from(&dir.join("nope.yaml")).unwrap();
    assert!(store.allowed_tools.is_empty());

    std::fs::write(dir.join("bad.yaml"), "::: not yaml :::").unwrap();
    let store = PermissionStore::load_from(&dir.join("bad.yaml")).unwrap();
    assert!(store.allowed_tools.is_empty());
}

#[test]
fn unknown_tool_names_are_dropped_on_load() {
    let dir = fresh_dir();
    let path = dir.join("permissions.yaml");
    std::fs::write(&path, "- bash\n- hoverboard\n- write_file\n").unwrap();

    let store = PermissionStore::load_from(&path).unwrap();
    let mut names = store.as_static_names();
    names.sort();
    assert_eq!(names, vec!["bash", "write_file"]);
}

#[test]
fn allow_forever_persists_and_reloads() {
    let dir = fresh_dir();
    let path = dir.join("permissions.yaml");

    let mut store = PermissionStore::load_from(&path).unwrap();
    store.allow_forever_to(&path, "bash").unwrap();
    store.allow_forever_to(&path, "bash").unwrap(); // idempotent
    store.allow_forever_to(&path, "write_file").unwrap();

    let reloaded = PermissionStore::load_from(&path).unwrap();
    let mut names = reloaded.as_static_names();
    names.sort();
    assert_eq!(
        names,
        vec!["bash", "write_file"],
        "rules survive a reload (restart parity)"
    );
}
