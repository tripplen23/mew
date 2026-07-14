//! A compact one-line card for a tool invocation in the chat transcript.
//!
//! Each card reveals three things at a glance: the **tool name**, a concise
//! one-line summary of its **arguments**, and a terse preview of the **result**.
//! Overlong values are elided gracefully with `…`.
//!
//! ## Future directions
//!
//! Expand/collapse toggles, syntax-highlighted result bodies, and streaming
//! tool cards are natural next steps once the current shape is settled.
//!
//! ## Visibility
//!
//! The three `render_*` functions and two helpers are `pub` so that integration
//! tests in `crates/client/tests/tool_card.rs` can drive them through the public.
//! The helpers are `#[doc(hidden)]`.

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

use mewcode_protocol::{ToolCall, ToolResult};

const MAX_ARGS_CHARS: usize = 60;
const MAX_RESULT_LINES: usize = 2;
const MAX_RESULT_LINE_CHARS: usize = 80;
const SUMMARISED_VALUE_MAX_CHARS: usize = 24;
const ELLIPSIS: &str = "…";

/// Render a `🛠️ ` header line for a tool call. The full arguments are
/// inlined as a one-line summary; long values are truncated with `…`.
pub fn render_tool_call_header(call: &ToolCall) -> Line<'static> {
    let args = truncate_one_line(&summarise_json(&call.input), MAX_ARGS_CHARS);
    Line::from(vec![
        Span::styled("🛠️ ", Style::default().fg(Color::Cyan)),
        Span::styled(
            format!("{}({args})", call.name),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
    ])
}

/// Render the result body lines (indented under the header). Returns an
/// empty `Vec` if the result is `null` or empty.
pub fn render_tool_result_body(res: &ToolResult) -> Vec<Line<'static>> {
    let summary = summarise_json(&res.output);
    if summary.is_empty() {
        return Vec::new();
    }
    let prefix = if res.is_error { "⎿ error: " } else { "⎿ " };
    let color = if res.is_error {
        Color::Red
    } else {
        Color::DarkGray
    };
    let lines: Vec<_> = summary.lines().collect();
    let truncated = lines.len() > MAX_RESULT_LINES;
    let mut out: Vec<Line<'static>> = Vec::new();
    for (i, line) in lines.into_iter().take(MAX_RESULT_LINES).enumerate() {
        let raw = if truncated && i == MAX_RESULT_LINES - 1 {
            format!("{line}{ELLIPSIS}")
        } else {
            line.to_string()
        };
        let text = truncate_one_line(&raw, MAX_RESULT_LINE_CHARS);
        let prefix = if i == 0 { prefix } else { "  " };
        out.push(Line::from(Span::styled(
            format!("{prefix}{text}"),
            Style::default().fg(color),
        )));
    }
    out
}

/// Render a `▸` header for a standalone `ToolResult`.
pub fn render_tool_result_header(res: &ToolResult) -> Line<'static> {
    let color = if res.is_error {
        Color::Red
    } else {
        Color::DarkGray
    };
    Line::from(Span::styled(
        format!(
            "▸ {} {}",
            res.name,
            if res.is_error { "error" } else { "ok" }
        ),
        Style::default().fg(color),
    ))
}

/// Compact one-line summary of a JSON value: stringify scalars directly,
/// objects show `"{k: v, k2: v2}"`, arrays show `"[n items]"`,
/// `null` is the empty string.
#[doc(hidden)]
pub fn summarise_json(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::Null => String::new(),
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Array(a) => {
            format!("[{} item{}]", a.len(), if a.len() == 1 { "" } else { "s" })
        }
        serde_json::Value::Object(o) => {
            let mut parts: Vec<String> = o
                .iter()
                .map(|(k, v)| {
                    let vs = summarise_json(v);
                    if vs.is_empty() {
                        k.to_string()
                    } else if vs.len() > SUMMARISED_VALUE_MAX_CHARS {
                        format!("{k}: {ELLIPSIS}")
                    } else {
                        format!("{k}: {vs}")
                    }
                })
                .collect();
            parts.sort();
            format!("{{{}}}", parts.join(", "))
        }
    }
}

/// Truncate a string on the first line so the result has at most
/// `max_chars` characters. If the input was cut, the last character is
/// replaced by `…` (a single-character ellipsis), keeping the total
/// width at the configured cap. With `max_chars == 0`, the result is
/// always `…` (one char), since the caller's cap is zero.
#[doc(hidden)]
pub fn truncate_one_line(s: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return ELLIPSIS.to_string();
    }
    let mut lines = s.lines();
    let first = lines.next().unwrap_or("");
    let had_more = lines.next().is_some();
    let cap_minus_marker = max_chars - 1;
    if !had_more && first.chars().count() <= max_chars {
        first.to_string()
    } else {
        let cut: String = first.chars().take(cap_minus_marker).collect();
        format!("{cut}{ELLIPSIS}")
    }
}
