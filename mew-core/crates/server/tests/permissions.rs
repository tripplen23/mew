//! Tests for the persistent always-allow permissions store (scoped rules).

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
    assert!(store.allowed.is_empty());

    std::fs::write(dir.join("bad.yaml"), "::: not yaml :::").unwrap();
    let store = PermissionStore::load_from(&dir.join("bad.yaml")).unwrap();
    assert!(store.allowed.is_empty());
}

#[test]
fn scoped_and_whole_tool_rules_parse_and_unknown_dropped() {
    let dir = fresh_dir();
    let path = dir.join("permissions.yaml");
    std::fs::write(&path, "- bash: ls\n- write_file\n- hoverboard: fly\n").unwrap();

    let store = PermissionStore::load_from(&path).unwrap();
    let mut seed = store.as_seed();
    seed.sort();
    assert_eq!(
        seed,
        vec![("bash", Some("ls")), ("write_file", None)],
        "whole-tool and scoped rules load; unknown tools drop"
    );
}

#[test]
fn allow_forever_persists_scoped_rules_and_reloads() {
    let dir = fresh_dir();
    let path = dir.join("permissions.yaml");

    let mut store = PermissionStore::load_from(&path).unwrap();
    store.allow_forever_to(&path, "bash", Some("ls")).unwrap();
    store.allow_forever_to(&path, "bash", Some("ls")).unwrap(); // idempotent
    store.allow_forever_to(&path, "bash", None).unwrap(); // whole tool
    store
        .allow_forever_to(&path, "write_file", Some("docs/x.md"))
        .unwrap();

    let reloaded = PermissionStore::load_from(&path).unwrap();
    let mut seed = reloaded.as_seed();
    seed.sort();
    assert_eq!(
        seed,
        vec![
            ("bash", None),
            ("bash", Some("ls")),
            ("write_file", Some("docs/x.md")),
        ],
        "rules survive a reload (restart parity)"
    );
}
