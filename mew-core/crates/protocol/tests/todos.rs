//! Integration tests for the todo-list wire types shared by engine and client.

use mewcode_protocol::{
    TodoDisplay, TodoItem, TodoList, TodoStatus, ToolAnnotations, ToolDisplay, ToolName,
    tool::names,
};

#[test]
fn todo_status_serialises_lowercase() {
    assert_eq!(
        serde_json::to_string(&TodoStatus::InProgress).unwrap(),
        "\"in_progress\""
    );
    assert_eq!(
        serde_json::from_str::<TodoStatus>("\"completed\"").unwrap(),
        TodoStatus::Completed
    );
}

#[test]
fn todo_list_round_trips() {
    let list: TodoList = vec![
        TodoItem {
            id: Some("1".into()),
            content: "Add todo tool".to_string(),
            status: TodoStatus::InProgress,
        },
        TodoItem {
            id: None,
            content: "Render dock".to_string(),
            status: TodoStatus::Pending,
        },
    ];
    let json = serde_json::to_string(&ToolDisplay::Todo(TodoDisplay {
        todos: list.clone(),
    }))
    .unwrap();
    // Tagged `kind`, snake_case — the wire label the client matches on.
    assert!(json.contains("\"kind\":\"todo\""));
    let back: ToolDisplay = serde_json::from_str(&json).unwrap();
    assert_eq!(back, ToolDisplay::Todo(TodoDisplay { todos: list }));
}

#[test]
fn todo_tools_are_scratch_names() {
    assert_eq!(
        ToolName::parse(names::TODO_WRITE),
        Some(ToolName(names::TODO_WRITE))
    );
    assert_eq!(
        ToolName::parse(names::TODO_READ),
        Some(ToolName(names::TODO_READ))
    );
    let annotations = ToolAnnotations::SCRATCH;
    assert!(annotations.approval_exempt);
    assert!(!annotations.read_only);
}
