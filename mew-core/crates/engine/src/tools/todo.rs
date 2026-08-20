//! `todo_write` and `todo_read` tools — the agent's per-session task list.
//!
//! `todo_write` replaces the whole list (Claude Code / OpenCode semantics):
//! the model sends the full array every time, so stale items are dropped
//! automatically. `todo_read` returns the current list.
//!
//! Both tools are session-scratch: they mutate `<data_dir>/todos/<session>.json`,
//! never the project, so they are available in Plan and Build modes and never
//! pause for approval.

use async_trait::async_trait;
use mewcode_protocol::tool::names;
use mewcode_protocol::{
    TodoDisplay, TodoItem, TodoList, TodoStatus, ToolAnnotations, ToolContracts, ToolDescriptor,
    ToolDisplay, ToolError, ToolExample, ToolOutput,
};
use serde_json::{Value, json};

use crate::context::{MAX_TODOS, TodoStore};
use crate::tools::ProjectContext;

const SUMMARY_LABELS: [&str; 3] = ["pending", "in progress", "completed"];

/// `todo_write` tool — replace the session's task list.
pub struct TodoWriteTool {
    store: TodoStore,
    ctx: ProjectContext,
}

impl TodoWriteTool {
    /// Build the tool against the session todo store and project context
    /// (the context only carries the display sink).
    pub fn new(store: TodoStore, ctx: ProjectContext) -> Self {
        Self { store, ctx }
    }

    fn summarize(&self, todos: &TodoList) -> String {
        let mut counts = [0usize; 3];
        for item in todos {
            let idx = match item.status {
                TodoStatus::Pending => 0,
                TodoStatus::InProgress => 1,
                TodoStatus::Completed => 2,
            };
            counts[idx] += 1;
        }
        let parts: Vec<String> = counts
            .iter()
            .zip(SUMMARY_LABELS)
            .filter(|(count, _)| **count > 0)
            .map(|(count, label)| format!("{count} {label}"))
            .collect();
        format!("{} todos · {}", todos.len(), parts.join(", "))
    }
}

fn parse_status(s: &str) -> Option<TodoStatus> {
    match s {
        "pending" => Some(TodoStatus::Pending),
        "in_progress" => Some(TodoStatus::InProgress),
        "completed" => Some(TodoStatus::Completed),
        _ => None,
    }
}

fn parse_list(input: &Value) -> Result<TodoList, ToolError> {
    let raw = input
        .get("todos")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            ToolError::invalid_input(
                "missing `todos` array",
                "pass the full task list, e.g. {\"todos\": [{\"content\": \"...\", \"status\": \"pending\"}]}",
            )
        })?;
    if raw.len() > MAX_TODOS {
        return Err(ToolError::invalid_input(
            format!("too many todos ({} > {MAX_TODOS})", raw.len()),
            "keep the task list at or under 32 items; fold smaller tasks together",
        ));
    }
    let mut out = TodoList::with_capacity(raw.len());
    for entry in raw {
        let obj = entry.as_object().ok_or_else(|| {
            ToolError::invalid_input(
                "each todo must be an object",
                "use {\"content\": \"...\", \"status\": \"pending\"}",
            )
        })?;
        let content = obj
            .get("content")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| {
                ToolError::invalid_input(
                    "each todo needs non-empty `content`",
                    "describe the task in one short sentence",
                )
            })?;
        let status_str = obj.get("status").and_then(Value::as_str).ok_or_else(|| {
            ToolError::invalid_input(
                "each todo needs a `status`",
                "use one of: pending, in_progress, completed",
            )
        })?;
        let status = parse_status(status_str).ok_or_else(|| {
            ToolError::invalid_input(
                format!("unknown status {status_str:?}"),
                "use one of: pending, in_progress, completed",
            )
        })?;
        out.push(TodoItem {
            id: obj.get("id").and_then(Value::as_str).map(str::to_string),
            content: content.to_string(),
            status,
        });
    }
    Ok(out)
}

#[async_trait]
impl ToolContracts for TodoWriteTool {
    fn name(&self) -> &'static str {
        names::TODO_WRITE
    }

    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            name: names::TODO_WRITE.to_string(),
            description: "Create and manage the task list for the current session. Replaces the entire list on every call.

**When to use:** To track multi-step work (3+ steps), show progress, and keep the session on task. Call with the FULL list every time — omitted items disappear.
**Status vocabulary:** `pending` → `in_progress` → `completed`. Keep exactly one item `in_progress` at a time. Mark completed only after the work is actually done.
**Why it matters:** This list survives context compaction, so you continue the remaining tasks after an overflow instead of regenerating a plan from scratch."
                .to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "todos": {
                        "type": "array",
                        "description": "The complete task list to persist (replaces the previous list). Max 32 items.",
                        "items": {
                            "type": "object",
                            "properties": {
                                "content": { "type": "string", "description": "Short task description." },
                                "status": { "type": "string", "enum": ["pending", "in_progress", "completed"] },
                                "id": { "type": "string", "description": "Optional stable id (assigned if absent; returned by reads)." }
                            },
                            "required": ["content", "status"],
                            "additionalProperties": false
                        }
                    }
                },
                "required": ["todos"],
                "additionalProperties": false,
            }),
            annotations: ToolAnnotations::SCRATCH,
            examples: vec![
                ToolExample {
                    description: "Plant an initial three-step list.".to_string(),
                    input: json!({
                        "todos": [
                            { "content": "Add todo tools", "status": "in_progress" },
                            { "content": "Render the dock", "status": "pending" },
                            { "content": "Verify end to end", "status": "pending" }
                        ]
                    }),
                },
                ToolExample {
                    description: "Mark the first task done, keep the rest.".to_string(),
                    input: json!({
                        "todos": [
                            { "id": "1", "content": "Add todo tools", "status": "completed" },
                            { "id": "2", "content": "Render the dock", "status": "in_progress" },
                            { "id": "3", "content": "Verify end to end", "status": "pending" }
                        ]
                    }),
                },
            ],
            max_response_chars: 2_000,
        }
    }

    async fn execute(&self, input: Value) -> Result<ToolOutput, ToolError> {
        let todos = parse_list(&input)?;
        let saved = self.store.save(&todos)?;
        // Render-only payload to the client dock, via the display sink. Never
        // enters the model's context window. Carries the *persisted* list
        // (ids assigned) so the dock matches `todo_read` and session hydrate.
        self.ctx.push_display(
            input,
            ToolDisplay::Todo(TodoDisplay {
                todos: saved.clone(),
            }),
        );
        Ok(ToolOutput::text(self.summarize(&saved)))
    }
}

/// `todo_read` tool — return the session's current task list.
pub struct TodoReadTool {
    store: TodoStore,
}

impl TodoReadTool {
    /// Build the tool against the session todo store.
    pub fn new(store: TodoStore) -> Self {
        Self { store }
    }
}

#[async_trait]
impl ToolContracts for TodoReadTool {
    fn name(&self) -> &'static str {
        names::TODO_READ
    }

    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            name: names::TODO_READ.to_string(),
            description: "Read the current task list for this session. Returns the persisted list (with stable ids) or an empty list when no todos exist.

**When to use:** At the start of a turn, after a context compaction, or before planning next steps — you need to know what remains before deciding what to do."
                .to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {},
                "additionalProperties": false,
            }),
            annotations: ToolAnnotations::READ_ONLY,
            examples: vec![ToolExample {
                description: "Check what remains before continuing.".to_string(),
                input: json!({}),
            }],
            max_response_chars: 4_000,
        }
    }

    async fn execute(&self, _input: Value) -> Result<ToolOutput, ToolError> {
        let todos = self.store.load();
        Ok(ToolOutput(
            serde_json::to_value(todos).unwrap_or(Value::Null),
        ))
    }
}
