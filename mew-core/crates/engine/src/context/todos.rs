//! Durable per-session todo store — the agent's task list for one session.
//!
//! Each session owns one JSON file under `<data_dir>/todos/`:
//! `<data_dir>/todos/<session_id>.json` — a single JSON array of
//! [`TodoItem`](mewcode_protocol::TodoItem). The file is independent of the
//! message transcript, so context compaction never touches it and the agent
//! can continue its remaining tasks after every turn (and after a restart).
//!
//! The whole list is replaced on each `todo_write` (Claude Code / OpenCode
//! semantics). Items without an `id` get a stable position-based id
//! (`"1"`, `"2"`, …) at save time, so the model can reference a task across
//! turns even when it only ever wrote content+status.

use std::fs;
use std::path::{Path, PathBuf};

use mewcode_protocol::TodoList;

/// Subdirectory (under the data dir) for per-session todo files.
const TODOS_DIR: &str = "todos";

/// Upper bound on the number of todo items a list may hold. Keeps a run of
/// `todo_write` calls (and the rendered dock) bounded.
pub const MAX_TODOS: usize = 32;

/// A durable per-session task list stored as one JSON file.
#[derive(Debug, Clone)]
pub struct TodoStore {
    path: PathBuf,
}

impl TodoStore {
    /// Build a store for `session_id` under `data_dir/todos/`.
    pub fn for_session(data_dir: impl Into<PathBuf>, session_id: &uuid::Uuid) -> Self {
        let path = data_dir
            .into()
            .join(TODOS_DIR)
            .join(format!("{session_id}.json"));
        Self { path }
    }

    /// Load the current list. Missing or corrupt files load as empty so a
    /// first write or a torn file cannot wedge the agent.
    pub fn load(&self) -> TodoList {
        fs::read_to_string(&self.path)
            .ok()
            .and_then(|raw| serde_json::from_str(&raw).ok())
            .unwrap_or_default()
    }

    /// Replace the whole list, atomically (temp file + rename, so a reader
    /// sees either the old list or the new, never a partial write). Missing
    /// ids become stable position-based ids ("1", "2", …) at save time.
    pub fn save(&self, todos: &TodoList) -> std::io::Result<()> {
        let assigned: TodoList = todos
            .iter()
            .enumerate()
            .map(|(i, item)| {
                let mut item = item.clone();
                if item.id.is_none() {
                    item.id = Some((i + 1).to_string());
                }
                item
            })
            .collect();
        let serialized = serde_json::to_string(&assigned)?;
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        let tmp = self.path.with_extension("json.tmp");
        fs::write(&tmp, serialized)?;
        fs::rename(&tmp, &self.path)
    }

    /// Format the current list as a system-prompt `<todos>` section. `None`
    /// when the list is empty or absent, so empty sessions inject nothing.
    pub fn format(&self) -> Option<String> {
        let todos = self.load();
        if todos.is_empty() {
            return None;
        }
        let mut body = String::new();
        for item in &todos {
            let (glyph, state) = match item.status {
                mewcode_protocol::TodoStatus::Completed => ("[x]", "completed"),
                mewcode_protocol::TodoStatus::InProgress => ("[~]", "in_progress"),
                mewcode_protocol::TodoStatus::Pending => ("[ ]", "pending"),
            };
            let id = item.id.as_deref().unwrap_or("-");
            body.push_str(&format!("{glyph} ({id}) {state}: {}\n", item.content));
        }
        Some(format!("<todos>\n{}</todos>", body.trim_end()))
    }

    /// Absolute path of the todo file (useful to tests).
    pub fn path(&self) -> &Path {
        &self.path
    }
}
