//! Harness integration for todos on the system-prompt path (`I004`, `P005`).
//! Guards: `<todos>` injected fresh from disk each turn, vanishes with no
//! store, and compaction does not touch the todo file or the injected block.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use mewcode_engine::Harness;
use mewcode_engine::context::TodoStore;
use mewcode_engine::skills::SkillRegistry;
use mewcode_engine::tools::ToolRegistry;
use mewcode_protocol::{Mode, ModelId, TodoItem, TodoStatus};
use uuid::Uuid;

static COUNTER: AtomicUsize = AtomicUsize::new(0);

fn fresh_store() -> TodoStore {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir =
        std::env::temp_dir().join(format!("mewcode-harness-todos-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    TodoStore::for_session(dir, &Uuid::new_v4())
}

fn harness(store: Option<TodoStore>) -> Harness {
    let skills = Arc::new(SkillRegistry::new());
    let tools = Arc::new(ToolRegistry::new());
    let mut h = Harness::new(ModelId::DEFAULT, Mode::Build, skills, tools);
    if let Some(store) = store {
        h = h.with_todos(store);
    }
    h
}

#[test]
fn todos_section_injected_when_present() {
    let store = fresh_store();
    store
        .save(&vec![TodoItem {
            id: None,
            content: "finish the dock".into(),
            status: TodoStatus::InProgress,
        }])
        .unwrap();
    let prompt = harness(Some(store)).compose_system_prompt();
    assert!(prompt.contains("<todos>"));
    assert!(prompt.contains("finish the dock"));
    assert!(prompt.contains("[~] (1) in_progress"));
}

#[test]
fn no_todos_section_when_store_empty_or_absent() {
    // Empty store → no section.
    let empty = harness(Some(fresh_store())).compose_system_prompt();
    assert!(!empty.contains("<todos>"));
    // No store at all → no section.
    let none = harness(None).compose_system_prompt();
    assert!(!none.contains("<todos>"));
}

#[test]
fn todos_survive_across_prompt_composition() {
    // Simulates the post-compaction turn: the file is re-read fresh, so the
    // remaining task is still present even though the transcript shrank.
    let store = fresh_store();
    store
        .save(&vec![TodoItem {
            id: None,
            content: "remaining".into(),
            status: TodoStatus::Pending,
        }])
        .unwrap();
    harness(Some(store.clone())).compose_system_prompt();
    // A "later" turn (new Harness, same store) still sees the list.
    let later = harness(Some(store)).compose_system_prompt();
    assert!(later.contains("<todos>"));
    assert!(later.contains("remaining"));
}
