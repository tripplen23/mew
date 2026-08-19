//! Tests for the durable per-session `TodoStore` (`I003`).
//! Guards: per-session path, missing→empty, atomic replace, stable ids,
//! format() shape, and that a raw `TodoItem` round-trips through the store.

use std::sync::atomic::{AtomicUsize, Ordering};

use mewcode_engine::context::TodoStore;
use mewcode_protocol::{TodoItem, TodoStatus};
use uuid::Uuid;

static COUNTER: AtomicUsize = AtomicUsize::new(0);

fn fresh() -> (TodoStore, std::path::PathBuf) {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("mewcode-todos-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let store = TodoStore::for_session(dir.clone(), &Uuid::new_v4());
    (store, dir)
}

fn item(content: &str, status: TodoStatus) -> TodoItem {
    TodoItem {
        id: None,
        content: content.to_string(),
        status,
    }
}

#[test]
fn missing_file_loads_empty() {
    let (store, _) = fresh();
    assert!(store.load().is_empty());
    assert!(store.format().is_none());
}

#[test]
fn save_is_atomic_and_survives_reload() {
    let (store, _) = fresh();
    store
        .save(&vec![
            item("one", TodoStatus::InProgress),
            item("two", TodoStatus::Pending),
        ])
        .unwrap();
    // No temp file left behind — rename completed.
    assert!(!store.path().with_extension("json.tmp").exists());
    let reloaded = TodoStore::for_session(
        store
            .path()
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .to_path_buf(),
        &Uuid::new_v4(),
    );
    // A *different* store path must not share data (per-session isolation):
    assert!(reloaded.load().is_empty());
    // Reload through a fresh store at the same path:
    let same = std::fs::read_to_string(store.path()).unwrap();
    let parsed: Vec<TodoItem> = serde_json::from_str(&same).unwrap();
    assert_eq!(parsed.len(), 2);
    assert_eq!(parsed[0].content, "one");
}

#[test]
fn missing_ids_become_stable_position_ids() {
    let (store, _) = fresh();
    store.save(&vec![item("one", TodoStatus::Pending)]).unwrap();
    let loaded = store.load();
    assert_eq!(loaded[0].id.as_deref(), Some("1"));
    // Re-save with an explicit id keeps it:
    let with_id = vec![TodoItem {
        id: Some("abc".into()),
        content: "keep".into(),
        status: TodoStatus::Completed,
    }];
    store.save(&with_id).unwrap();
    assert_eq!(store.load()[0].id.as_deref(), Some("abc"));
}

#[test]
fn format_renders_todos_block_for_nonempty() {
    let (store, _) = fresh();
    store
        .save(&vec![
            item("a", TodoStatus::InProgress),
            item("b", TodoStatus::Completed),
        ])
        .unwrap();
    let section = store.format().unwrap();
    assert!(section.starts_with("<todos>\n"));
    assert!(section.contains("[x] (2) completed: b"));
    assert!(section.contains("[~] (1) in_progress: a"));
    assert!(section.ends_with("</todos>"));
}

#[test]
fn corrupt_file_loads_as_empty() {
    let (store, _) = fresh();
    std::fs::create_dir_all(store.path().parent().unwrap()).unwrap();
    std::fs::write(store.path(), "not json {").unwrap();
    assert!(store.load().is_empty());
    assert!(store.format().is_none());
}
