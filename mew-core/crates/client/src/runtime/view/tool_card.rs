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
//! The four `render_*` functions and two helpers are `pub` so that integration
//! tests in `crates/client/tests/tool_card.rs` can drive them through the public.
//! The helpers are `#[doc(hidden)]`.

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use similar::{ChangeTag, TextDiff};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use mewcode_protocol::{DiffDisplay, ToolCall, ToolResult};

const MAX_ARGS_CHARS: usize = 60;
const MAX_RESULT_LINES: usize = 2;
const MAX_RESULT_LINE_CHARS: usize = 80;
const SUMMARISED_VALUE_MAX_CHARS: usize = 24;
const ELLIPSIS: &str = "…";
const MAX_DIFF_LINES: usize = 50;
const MAX_DIFF_CONTENT_COLS: usize = 76;
const DIFF_ADD_FG: Color = Color::Rgb(181, 240, 194);
const DIFF_ADD_BG: Color = Color::Rgb(19, 51, 33);
const DIFF_DEL_FG: Color = Color::Rgb(245, 181, 187);
const DIFF_DEL_BG: Color = Color::Rgb(61, 25, 31);
const DIFF_GUTTER_FG: Color = Color::Rgb(120, 128, 140);

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

/// A single rendered diff row
struct DiffRow {
    added: bool,
    line_no: u64,
    content: String,
}

/// Render an inline diff card from a tool's render-only [`DiffDisplay`]. Works
/// for any file-mutating tool that supplies one (`edit_file`, `write_file`);
/// the payload arrives on the display channel and never touches the model's
/// context.
///
/// Styling mirrors a real diff viewer: a `⎿ path  +A -B` header, then only the
/// changed lines, each drawn as a filled band — dim-green for additions,
/// dim-red for deletions — with a right-aligned line-number gutter. Rows are
/// padded to a common width so the bands align, bounded to [`MAX_DIFF_LINES`]
/// rows and [`MAX_DIFF_CONTENT_COLS`] columns.
pub fn render_diff(diff: &DiffDisplay) -> Vec<Line<'static>> {
    let base = diff.start_line.unwrap_or(1);
    let text_diff = TextDiff::from_lines(&diff.old, &diff.new);

    let mut rows: Vec<DiffRow> = Vec::new();
    let (mut adds, mut dels) = (0usize, 0usize);
    for change in text_diff.iter_all_changes() {
        let line = change.value();
        let content = line.trim_end_matches(['\n', '\r']).to_string();
        match change.tag() {
            ChangeTag::Insert => {
                adds += 1;
                rows.push(DiffRow {
                    added: true,
                    line_no: base + change.new_index().unwrap_or(0) as u64,
                    content,
                });
            }
            ChangeTag::Delete => {
                dels += 1;
                rows.push(DiffRow {
                    added: false,
                    line_no: base + change.old_index().unwrap_or(0) as u64,
                    content,
                });
            }
            ChangeTag::Equal => {}
        }
    }

    let mut out = vec![diff_header(&diff.path, adds, dels)];

    let elided = rows.len() > MAX_DIFF_LINES;
    rows.truncate(MAX_DIFF_LINES);

    // Uniform gutter + content widths so the colored bands line up.
    let gutter_w = rows
        .iter()
        .map(|r| decimal_width(r.line_no))
        .max()
        .unwrap_or(1)
        .max(2);
    let content_w = rows
        .iter()
        .map(|r| display_width(&r.content).min(MAX_DIFF_CONTENT_COLS))
        .max()
        .unwrap_or(0);

    for row in &rows {
        let (fg, bg) = if row.added {
            (DIFF_ADD_FG, DIFF_ADD_BG)
        } else {
            (DIFF_DEL_FG, DIFF_DEL_BG)
        };
        let marker = if row.added { '+' } else { '-' };
        let content = pad_display(
            &display_clip(&row.content, MAX_DIFF_CONTENT_COLS),
            content_w,
        );
        out.push(Line::from(vec![
            Span::styled(
                format!(" {:>gutter_w$} ", row.line_no),
                Style::default().fg(DIFF_GUTTER_FG).bg(bg),
            ),
            Span::styled(
                format!("{marker} {content} "),
                Style::default().fg(fg).bg(bg),
            ),
        ]));
    }
    if elided || diff.truncated {
        out.push(Line::from(Span::styled(
            format!("  {ELLIPSIS}"),
            Style::default().fg(Color::DarkGray),
        )));
    }
    out
}

/// The `⎿ path  +A -B` header line above a diff, with the counts colored.
fn diff_header(path: &str, adds: usize, dels: usize) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            format!("⎿ {} ", display_clip(path, MAX_RESULT_LINE_CHARS)),
            Style::default().fg(Color::Gray),
        ),
        Span::styled(format!(" +{adds}"), Style::default().fg(DIFF_ADD_FG)),
        Span::styled(format!(" -{dels}"), Style::default().fg(DIFF_DEL_FG)),
    ])
}

/// Number of decimal digits in `n` (min 1).
fn decimal_width(n: u64) -> usize {
    if n == 0 { 1 } else { (n.ilog10() as usize) + 1 }
}

/// Display width of `s` (unicode-aware).
fn display_width(s: &str) -> usize {
    UnicodeWidthStr::width(s)
}

/// Clip `s` to at most `max` display columns, appending `…` if cut.
fn display_clip(s: &str, max: usize) -> String {
    if display_width(s) <= max {
        return s.to_string();
    }
    let budget = max.saturating_sub(1);
    let mut width = 0;
    let mut out = String::new();
    for ch in s.chars() {
        let cw = UnicodeWidthChar::width(ch).unwrap_or(0);
        if width + cw > budget {
            break;
        }
        width += cw;
        out.push(ch);
    }
    out.push('…');
    out
}

/// Right-pad `s` with spaces to `width` display columns (no-op if already wider).
fn pad_display(s: &str, width: usize) -> String {
    let w = display_width(s);
    if w >= width {
        return s.to_string();
    }
    let mut s = s.to_string();
    s.push_str(&" ".repeat(width - w));
    s
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
