//! Tool card renderers: the compact one-line `🛠️ name(args)` and `⎿` body
//! lines drawn for `MessagePart::ToolCall` / `MessagePart::ToolResult` in
//! the chat transcript.
//!
//! These tests live in `tests/` (not in a `#[cfg(test)] mod tests` block
//! inside `view/tool_card.rs`) per `CONTRIBUTING.md` §"Tests". The renderers
//! and their helpers are `pub` so a downstream test crate can drive them
//! through the public surface — see the module-level note in
//! `runtime/view/tool_card.rs` for the visibility rationale.

use mewcode_client::net::Session;
use mewcode_client::runtime::model::{App, Screen, SessionState};
use mewcode_client::runtime::view::{
    render, render_diff, render_tool_call_header, render_tool_result_body,
    render_tool_result_header, summarise_json, truncate_one_line,
};
use mewcode_protocol::{
    DiffDisplay, Message, MessagePart, Mode, ModelId, ToolCall, ToolDisplay, ToolResult,
};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use serde_json::json;
use uuid::Uuid;

fn call(name: &str, input: serde_json::Value) -> ToolCall {
    ToolCall {
        id: "id".into(),
        name: name.into(),
        input,
    }
}

fn result(name: &str, output: serde_json::Value, is_error: bool) -> ToolResult {
    ToolResult {
        call_id: "id".into(),
        name: name.into(),
        output,
        is_error,
        display: None,
    }
}

/// Flatten all `Span`s in a `Line` into one `String`. Test-only helper —
/// the underlying renderers are typed so callers don't have to flatten.
fn line_text(line: &ratatui::text::Line) -> String {
    line.spans.iter().map(|s| s.content.as_ref()).collect()
}

#[test]
fn header_renders_name_and_args_summary() {
    let line = render_tool_call_header(&call("readFile", json!({"path": "src/lib.rs"})));
    let text = line_text(&line);
    assert!(text.contains("readFile"), "missing name: {text:?}");
    assert!(text.contains("src/lib.rs"), "missing arg: {text:?}");
    assert!(text.starts_with("🛠️ "), "missing glyph: {text:?}");
}

#[test]
fn header_truncates_object_with_many_keys() {
    // A wide object is the one case the inner summariser doesn't
    // collapse (we sort the keys and show them all). If the result
    // exceeds the outer cap, the outer truncate must add `…`.
    let mut obj = serde_json::Map::new();
    for i in 0..20 {
        obj.insert(
            format!("key_{i:02}"),
            serde_json::Value::String(format!("value_{i}")),
        );
    }
    let line = render_tool_call_header(&call("wide", serde_json::Value::Object(obj)));
    let text = line_text(&line);
    // Outer truncate may insert `…` mid-string (inside the closing
    // brace), so check for the ellipsis anywhere in the text rather
    // than at the very end.
    assert!(
        text.contains('…'),
        "expected outer truncate to add `…`: {text:?}"
    );
}

#[test]
fn header_signals_long_value_via_summariser_ellipsis() {
    // Single very long string is collapsed to `…` by the inner
    // summariser, which is the first line of defence against dumping
    // huge values into the transcript.
    let long = "x".repeat(200);
    let line = render_tool_call_header(&call("bash", json!({"cmd": long})));
    let text = line_text(&line);
    assert!(
        text.contains('…'),
        "expected summariser ellipsis for long value: {text:?}"
    );
}

#[test]
fn result_body_uses_red_for_error() {
    let body = render_tool_result_body(&result("bash", json!("permission denied"), true));
    assert!(!body.is_empty(), "expected at least one body line");
    let text = line_text(&body[0]);
    assert!(text.contains("error"), "missing error label: {text:?}");
}

#[test]
fn result_body_is_empty_for_null() {
    let body = render_tool_result_body(&result("noop", serde_json::Value::Null, false));
    assert!(body.is_empty());
}

#[test]
fn result_body_signals_omitted_lines() {
    // A 3+ line result renders only the first MAX_RESULT_LINES (2)
    // lines. The last visible line must carry `…` so the user knows
    // more output existed beyond the preview, instead of mistaking the
    // truncation for a complete two-line result.
    let body = render_tool_result_body(&result(
        "bash",
        json!("line 1\nline 2\nline 3\nline 4"),
        false,
    ));
    assert_eq!(body.len(), 2, "expected exactly 2 visible lines: {body:?}");
    let last = line_text(&body[1]);
    assert!(
        last.ends_with('…'),
        "last visible line must mark omitted output: {last:?}"
    );
}

#[test]
fn result_body_two_lines_keeps_no_ellipsis() {
    // The `…` signal only fires when more lines were dropped. A result
    // that fits within MAX_RESULT_LINES must render cleanly.
    let body = render_tool_result_body(&result("bash", json!("line 1\nline 2"), false));
    assert_eq!(body.len(), 2);
    let all: String = body.iter().map(line_text).collect();
    assert!(
        !all.contains('…'),
        "no ellipsis when nothing was dropped: {all:?}"
    );
}

#[test]
fn result_header_marks_error() {
    let line = render_tool_result_header(&result("bash", json!("nope"), true));
    let text = line_text(&line);
    assert!(text.contains("error"), "missing error: {text:?}");
}

#[test]
fn summarise_scalars_round_trip() {
    assert_eq!(summarise_json(&json!("hi")), "hi");
    assert_eq!(summarise_json(&json!(42)), "42");
    assert_eq!(summarise_json(&json!(true)), "true");
    assert_eq!(summarise_json(&serde_json::Value::Null), "");
}

#[test]
fn summarise_array_shows_count() {
    assert_eq!(summarise_json(&json!([1, 2, 3])), "[3 items]");
    assert_eq!(summarise_json(&json!(["only"])), "[1 item]");
}

#[test]
fn summarise_object_sorts_keys() {
    let s = summarise_json(&json!({"b": 1, "a": "hi"}));
    assert!(s.starts_with('{') && s.ends_with('}'));
    let ai = s.find("a:").unwrap();
    let bi = s.find("b:").unwrap();
    assert!(ai < bi, "keys should be sorted: {s}");
}

#[test]
fn truncate_one_line_appends_ellipsis() {
    let s = "x".repeat(100);
    let out = truncate_one_line(&s, 10);
    assert!(out.ends_with('…'), "expected trailing ellipsis: {out:?}");
    assert!(
        out.chars().filter(|c| *c == 'x').count() == 9,
        "expected 9 x chars (cap=10 minus the ellipsis): {out:?}"
    );
    assert_eq!(out.chars().count(), 10, "result must respect the cap");
}

#[test]
fn truncate_one_line_respects_exact_cap() {
    // Boundary: input is exactly `max_chars` characters — no truncation
    // needed, no ellipsis appended.
    let s = "x".repeat(60);
    let out = truncate_one_line(&s, 60);
    assert_eq!(out.chars().count(), 60);
    assert!(!out.ends_with('…'));
}

#[test]
fn truncate_one_line_overflows_by_one_returns_cap() {
    // The previous implementation took `max_chars` chars and then appended
    // `…`, producing `max_chars + 1` characters and breaking the width
    // contract. This test pins the fix.
    let s = "x".repeat(61);
    let out = truncate_one_line(&s, 60);
    assert_eq!(out.chars().count(), 60, "cap must be hard: {out:?}");
    assert!(out.ends_with('…'));
}

#[test]
fn truncate_one_line_zero_cap_returns_single_ellipsis() {
    // Edge case: cap of zero is degenerate, but the caller asked for a
    // hard limit. Returning a single `…` is the most truthful signal.
    assert_eq!(truncate_one_line("anything", 0), "…");
}

#[test]
fn truncate_one_line_passthrough_when_short() {
    assert_eq!(truncate_one_line("hello", 10), "hello");
    assert_eq!(truncate_one_line("", 10), "");
}

#[test]
fn truncate_one_line_signals_embedded_newline_cut() {
    // When the input has more than one line but the first line fits
    // within the cap, the function must still mark the cut with `…`
    // so the caller knows the visible prefix came from a multiline
    // string. Without this signal, a 5-line argument whose first line
    // is short would render as if it were a complete single-line
    // value.
    let s = "short\nsecond line is much longer than the cap";
    let out = truncate_one_line(s, 30);
    assert!(
        out.ends_with('…'),
        "expected ellipsis for multiline cut: {out:?}"
    );
}

#[test]
fn truncate_one_line_single_line_passthrough_stays_clean() {
    // Single-line input that fits must not get a spurious `…` — the
    // embedded-newline signal should only fire when there really is
    // a next line.
    let out = truncate_one_line("hello", 30);
    assert_eq!(out, "hello");
    assert!(!out.ends_with('…'));
}

// --- pairing regression tests ---------------------------------------------
//
// `render_message` must not emit a redundant `▸ name ok/error` header when a
// `ToolResult` immediately follows its `ToolCall` in the same message. The
// committed assistant message produced by `commit_assistant_message` (in
// `update/stream.rs`) is always `ToolCall(call) → ToolResult(result)` with
// matching ids, so this is the shape that matters in practice.

fn draw_session(messages: Vec<Message>) -> String {
    let session = Session {
        id: Uuid::new_v4(),
        title: "pairingtest".to_string(),
        model: ModelId::default(),
        mode: Mode::default(),
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
        messages,
    };
    let mut app = App::new();
    app.screen = Screen::Session(SessionState::new(session));
    let mut terminal = Terminal::new(TestBackend::new(120, 30)).unwrap();
    terminal.draw(|frame| render(frame, &mut app)).unwrap();
    terminal.backend().to_string()
}

#[test]
fn transcript_renders_user_file_mentions() {
    let msg = Message::user(vec![MessagePart::Text {
        text: "read @src/main.rs".to_string(),
    }]);

    let buf = draw_session(vec![msg]);

    assert!(
        buf.contains("@src/main.rs"),
        "expected file mention: {buf:?}"
    );
}

#[test]
fn paired_tool_call_and_result_render_one_card() {
    let call_id = "call-1".to_string();
    let msg = Message::assistant(
        vec![
            MessagePart::ToolCall(ToolCall {
                id: call_id.clone(),
                name: "bash".into(),
                input: json!({"cmd": "ls"}),
            }),
            MessagePart::ToolResult(ToolResult {
                call_id: call_id.clone(),
                name: "bash".into(),
                output: json!("file.txt\n"),
                is_error: false,
                display: None,
            }),
        ],
        ModelId::default().as_str(),
    );
    let buf = draw_session(vec![msg]);
    assert!(
        buf.contains("🛠️ bash"),
        "expected tool call header: {buf:?}"
    );
    assert!(
        buf.contains("⎿ file.txt"),
        "expected result body preview: {buf:?}"
    );
    assert!(
        !buf.contains("▸ bash"),
        "paired result must not emit a redundant `▸` header: {buf:?}"
    );
}

#[test]
fn standalone_tool_result_keeps_the_arrow_header() {
    // A result whose call_id doesn't match the preceding call (or where there
    // is no preceding call at all) still gets the standalone `▸ name ok` /
    // `▸ name error` header. This keeps the rare case where a tool result
    // surfaces without its call (e.g. a replayed transcript) readable.
    let msg = Message::assistant(
        vec![MessagePart::ToolResult(ToolResult {
            call_id: "orphan".into(),
            name: "bash".into(),
            output: json!("late result"),
            is_error: false,
            display: None,
        })],
        ModelId::default().as_str(),
    );
    let buf = draw_session(vec![msg]);
    assert!(
        buf.contains("▸ bash"),
        "expected standalone header: {buf:?}"
    );
    assert!(buf.contains("⎿ late result"), "expected body: {buf:?}");
}

// --- edit_file diff card --------------------------------------------------

fn diff_result(
    name: &str,
    path: &str,
    start_line: Option<u64>,
    old: &str,
    new: &str,
    call_id: &str,
) -> ToolResult {
    ToolResult {
        call_id: call_id.into(),
        name: name.into(),
        output: json!({ "path": path }),
        is_error: false,
        display: Some(ToolDisplay::Diff(DiffDisplay::new(
            path, start_line, old, new,
        ))),
    }
}

#[test]
fn diff_display_renders_removed_and_added_lines() {
    // The unified diff renderer: removed lines as `-`, added as `+`, with a
    // `@@ start_line @@` header. Works for any tool that supplies a DiffDisplay.
    let diff = DiffDisplay::new("src/lib.rs", Some(7), "fn old_name()\n", "fn new_name()\n");
    let body = render_diff(&diff);
    let text: String = body.iter().map(line_text).collect::<Vec<_>>().join("\n");
    assert!(
        text.contains("- fn old_name()"),
        "expected removed line: {text:?}"
    );
    assert!(
        text.contains("+ fn new_name()"),
        "expected added line: {text:?}"
    );
    assert!(
        text.contains("src/lib.rs") && text.contains("+1") && text.contains("-1"),
        "expected header with path and change counts: {text:?}"
    );
    // The removed line maps to file line 7 (start_line base) in the gutter.
    assert!(
        text.contains(" 7 "),
        "expected line-number gutter: {text:?}"
    );
}

#[test]
fn new_file_diff_is_all_additions() {
    // A write_file on a new file: empty old side -> every line is an addition.
    let diff = DiffDisplay::new("new.rs", None, "", "line one\nline two\n");
    let text: String = render_diff(&diff)
        .iter()
        .map(line_text)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        text.contains("+ line one") && text.contains("+ line two"),
        "{text:?}"
    );
    assert!(
        !text.contains("- "),
        "new file has no removed lines: {text:?}"
    );
}

#[test]
fn edit_file_result_with_display_renders_diff_not_summary() {
    // End to end through the transcript: a paired edit_file result carrying a
    // DiffDisplay shows the colored diff instead of the `{path: …}` summary.
    let call_id = "edit-1".to_string();
    let msg = Message::assistant(
        vec![
            MessagePart::ToolCall(ToolCall {
                id: call_id.clone(),
                name: "edit_file".into(),
                input: json!({"path": "src/lib.rs", "old_string": "a", "new_string": "b"}),
            }),
            MessagePart::ToolResult(diff_result(
                "edit_file",
                "src/lib.rs",
                Some(7),
                "fn old_name()\n",
                "fn new_name()\n",
                &call_id,
            )),
        ],
        ModelId::default().as_str(),
    );
    let buf = draw_session(vec![msg]);
    assert!(
        buf.contains("🛠️ edit_file"),
        "expected call header: {buf:?}"
    );
    assert!(
        buf.contains("- fn old_name()"),
        "expected removed line: {buf:?}"
    );
    assert!(
        buf.contains("+ fn new_name()"),
        "expected added line: {buf:?}"
    );
    assert!(
        buf.contains("src/lib.rs"),
        "expected diff header with path: {buf:?}"
    );
}

#[test]
fn tool_result_without_display_keeps_generic_body() {
    // Regression: a tool with no DiffDisplay still renders the JSON summary.
    let body = render_tool_result_body(&result("write_file", json!({"bytes_written": 12}), false));
    let text: String = body.iter().map(line_text).collect::<Vec<_>>().join("\n");
    assert!(
        text.contains("bytes_written"),
        "expected generic summary: {text:?}"
    );
}

#[test]
fn tool_result_after_text_is_standalone() {
    // If a `Text` part appears between the call and the result, the result
    // must be treated as standalone (the call's card has already been
    // "closed" by the text).
    let call_id = "call-2".to_string();
    let msg = Message::assistant(
        vec![
            MessagePart::ToolCall(ToolCall {
                id: call_id.clone(),
                name: "bash".into(),
                input: json!({"cmd": "ls"}),
            }),
            MessagePart::Text {
                text: "between".into(),
            },
            MessagePart::ToolResult(ToolResult {
                call_id: call_id.clone(),
                name: "bash".into(),
                output: json!("late"),
                is_error: false,
                display: None,
            }),
        ],
        ModelId::default().as_str(),
    );
    let buf = draw_session(vec![msg]);
    assert!(buf.contains("🛠️ bash"), "expected call header: {buf:?}");
    assert!(
        buf.contains("between"),
        "expected interleaved text: {buf:?}"
    );
    assert!(
        buf.contains("▸ bash"),
        "result after text must be standalone: {buf:?}"
    );
    assert!(buf.contains("⎿ late"), "expected body: {buf:?}");
}
