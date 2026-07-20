//! Integration tests for `mewcode_protocol::tool`.

use mewcode_protocol::tool::names;
use mewcode_protocol::{
    Mode, ToolError, ToolErrorPayload, ToolName, allowed_tools_for_mode, truncate_with_marker,
};

#[test]
fn truncation_marker_is_helpful() {
    let big = "x".repeat(200);
    let truncated = truncate_with_marker(&big, 10);
    assert!(truncated.contains("[truncated:"));
    assert!(truncated.contains("200 total chars"));
    assert!(truncated.contains("showing first 10"));
}

#[test]
fn truncation_is_noop_when_short() {
    let s = "hello";
    assert_eq!(truncate_with_marker(s, 100), "hello");
}

#[test]
fn tool_name_parsing_round_trips() {
    for n in ToolName::ALL {
        assert_eq!(ToolName::parse(n.0), Some(*n));
    }
    assert_eq!(ToolName::parse("nope"), None);
}

#[test]
fn mode_allowed_tools_gates_write_tools() {
    let plan = allowed_tools_for_mode(Mode::Plan);
    let build = allowed_tools_for_mode(Mode::Build);
    assert!(plan.contains(&names::READ_FILE));
    assert!(!plan.contains(&names::WRITE_FILE));
    assert!(build.contains(&names::WRITE_FILE));
}

#[test]
fn error_payload_is_actionable() {
    let e = ToolError::invalid_input(
        "oldString not found in file",
        "double-check the exact whitespace of oldString, or read the file first",
    );
    let payload = ToolErrorPayload::from(&e);
    assert_eq!(payload.kind, "invalid_input");
    assert!(payload.hint.unwrap().contains("double-check"));
    assert!(!payload.retryable);
}
