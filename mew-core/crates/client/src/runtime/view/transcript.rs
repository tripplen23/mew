//! Transcript rendering for the session screen.
//!
//! This module owns message-to-lines conversion plus scroll measurement. The
//! view writes the measured scroll bounds back into [`SessionState`] so key
//! handling can clamp PageUp/PageDown without doing layout work.

use ratatui::Frame;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Paragraph, Wrap};

use mewcode_protocol::{MessagePart, Role};

use super::super::model::{CompactionView, SessionState, TurnItem};
use super::entry::render_entry_lines;
use super::markdown::render_markdown;
use super::session::render_mentions;
use super::spinner::spinner_frame;
use super::theme::Theme;
use super::tool_card::{
    render_diff, render_tool_call_header, render_tool_result_body, render_tool_result_header,
};
use mewcode_protocol::{ToolCall, ToolDisplay, ToolResult};

/// Render the transcript panel and update its scroll bounds.
pub(super) fn render_transcript(
    frame: &mut Frame,
    chunk: Rect,
    s: &mut SessionState,
    theme: Theme,
) {
    let mut lines: Vec<Line> = Vec::new();
    let is_entry = s.session.is_none();
    match &s.session {
        Some(session) => {
            let mut msg_idx = 0;
            let mut comp_idx = 0;
            while msg_idx < session.messages.len() || comp_idx < s.compaction.committed.len() {
                let next_comp = s.compaction.committed.get(comp_idx);
                let next_msg = session.messages.get(msg_idx);

                let comp_first = next_comp
                    .map(|c| c.after_message_count == msg_idx)
                    .unwrap_or(false);

                if comp_first {
                    if let Some(entry) = next_comp {
                        lines.extend(render_compaction_section(&entry.view, theme, chunk.width));
                    }
                    comp_idx += 1;
                } else if let Some(msg) = next_msg {
                    lines.extend(render_message(msg, theme));
                    lines.push(Line::from(""));
                    msg_idx += 1;
                } else {
                    break;
                }
            }
            for entry in &s.compaction.committed[comp_idx..] {
                lines.extend(render_compaction_section(&entry.view, theme, chunk.width));
            }
        }
        None => {
            lines.extend(render_entry_lines(s, theme, chunk));
        }
    }
    if let Some(st) = &s.streaming {
        lines.push(Line::from(Span::styled(
            format!("{} assistant", spinner_frame(st.started_at.elapsed())),
            Style::default()
                .fg(theme.mew_gold)
                .add_modifier(Modifier::BOLD),
        )));
        for item in &st.items {
            match item {
                TurnItem::Text(text) | TurnItem::Progress(text) => {
                    if !text.is_empty() {
                        lines.extend(render_markdown(text));
                    }
                }
                TurnItem::Tool(view) => {
                    let call = ToolCall {
                        id: view.id.clone(),
                        name: view.name.clone(),
                        input: view.input.clone(),
                    };
                    lines.push(render_tool_call_header(&call));
                    match &view.display {
                        Some(ToolDisplay::Diff(diff)) => lines.extend(render_diff(diff)),
                        None => {
                            if let Some(output) = &view.output {
                                let res = ToolResult {
                                    call_id: view.id.clone(),
                                    name: view.name.clone(),
                                    output: output.clone(),
                                    is_error: false,
                                    display: None,
                                };
                                lines.extend(render_tool_result_body(&res));
                            }
                        }
                    }
                }
                TurnItem::Compaction(view) => {
                    lines.extend(render_compaction_section(view, theme, chunk.width));
                }
            }
        }
    }

    let mut para = Paragraph::new(Text::from(lines))
        .style(Style::default().fg(theme.text).bg(theme.ink_bg))
        .wrap(Wrap { trim: false });
    if is_entry {
        para = para.alignment(Alignment::Center);
    }
    let total = para.line_count(chunk.width).min(u16::MAX as usize) as u16;

    s.viewport = chunk.height;
    s.max_scroll = total.saturating_sub(chunk.height);
    if is_entry {
        s.scroll = 0;
    } else if s.follow {
        s.scroll = s.max_scroll;
    } else {
        s.scroll = s.scroll.min(s.max_scroll);
    }

    frame.render_widget(para.scroll((s.scroll, 0)), chunk);
}

fn render_message(msg: &mewcode_protocol::Message, theme: Theme) -> Vec<Line<'static>> {
    let (label, label_style) = match msg.role {
        Role::User => ("you", Style::default().fg(theme.hot_pink)),
        Role::Assistant => ("assistant", Style::default().fg(theme.mew_gold)),
        Role::Tool => ("tool", Style::default().fg(theme.lavender)),
    };
    let mut out = vec![Line::from(Span::styled(
        label.to_string(),
        label_style.add_modifier(Modifier::BOLD),
    ))];

    let mut last_tool_call: Option<&ToolCall> = None;
    for part in &msg.parts {
        match part {
            MessagePart::Text { text } => {
                last_tool_call = None;
                if msg.role == Role::User {
                    for line_text in text.split('\n') {
                        out.push(Line::from(render_mentions(line_text, theme)));
                    }
                } else {
                    out.extend(render_markdown(text));
                }
            }
            MessagePart::ToolCall(call) => {
                last_tool_call = Some(call);
                out.push(render_tool_call_header(call));
            }
            MessagePart::ToolResult(res) => {
                let paired = last_tool_call.map(|c| c.id == res.call_id).unwrap_or(false);
                last_tool_call = None;
                if !paired {
                    out.push(render_tool_result_header(res));
                }
                // A tool that supplied render-only display data (a diff) shows
                // a colored inline diff instead of the generic JSON summary;
                // every other tool keeps the existing body.
                match &res.display {
                    Some(ToolDisplay::Diff(diff)) => out.extend(render_diff(diff)),
                    None => out.extend(render_tool_result_body(res)),
                }
            }
            MessagePart::FileMention { path } => {
                last_tool_call = None;
                let color = if path.ends_with('/') {
                    theme.psy_cyan
                } else {
                    theme.mew_gold
                };
                out.push(Line::from(Span::styled(
                    format!("@{path}"),
                    Style::default().fg(color),
                )));
            }
        }
    }
    out
}

fn render_compaction_section(
    view: &CompactionView,
    theme: Theme,
    width: u16,
) -> Vec<Line<'static>> {
    let mut out = Vec::new();
    let label = " Compaction ";
    let total_dashes = (width as usize).saturating_sub(label.len());
    let left_len = total_dashes / 2;
    let right_len = total_dashes - left_len;
    let header = format!("{}{}{}", "─".repeat(left_len), label, "─".repeat(right_len));
    out.push(Line::from(Span::styled(
        header,
        Style::default()
            .fg(theme.psy_cyan)
            .add_modifier(Modifier::BOLD),
    )));
    out.push(Line::from(""));

    let secs = view.thought_duration_ms as f64 / 1000.0;
    out.push(Line::from(Span::styled(
        format!("+ Thought: {secs:.1}s"),
        Style::default().fg(theme.mew_gold),
    )));
    out.push(Line::from(""));

    out.extend(render_markdown(&view.summary));
    out.push(Line::from(""));
    out
}
