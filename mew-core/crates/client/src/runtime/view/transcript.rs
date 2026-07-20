//! Transcript rendering for the session screen.
//!
//! This module owns message-to-lines conversion plus scroll measurement. The
//! view writes the measured scroll bounds back into [`SessionState`] so key
//! handling can clamp PageUp/PageDown without doing layout work.

use ratatui::Frame;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, BorderType, Paragraph, Wrap};

use mewcode_protocol::{MessagePart, Role};

use super::super::model::{SessionState, TurnItem};
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
    let title = s
        .session
        .as_ref()
        .map(|sess| sess.title.as_str())
        .unwrap_or(" mewcode ");
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.lavender).bg(theme.ink_bg))
        .style(Style::default().bg(theme.ink_bg))
        .title(Span::styled(
            format!(" {title} "),
            Style::default()
                .fg(theme.hot_pink)
                .bg(theme.ink_bg)
                .add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(chunk);
    let is_entry = s.session.is_none();
    match &s.session {
        Some(session) => {
            for msg in &session.messages {
                lines.extend(render_message(msg, theme));
                lines.push(Line::from(""));
            }
        }
        None => {
            lines.extend(render_entry_lines(s, theme, inner));
        }
    }
    if let Some(st) = &s.streaming {
        lines.push(Line::from(Span::styled(
            format!("{} assistant", spinner_frame(st.started_at.elapsed())),
            Style::default()
                .fg(theme.mew_gold)
                .add_modifier(Modifier::BOLD),
        )));
        // Render the in-flight turn in arrival order
        // so the live view matches the runtime stream
        for item in &st.items {
            match item {
                TurnItem::Text(text) => {
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
            }
        }
    }

    let mut para = Paragraph::new(Text::from(lines))
        .style(Style::default().fg(theme.text).bg(theme.ink_bg))
        .wrap(Wrap { trim: false });
    if is_entry {
        para = para.alignment(Alignment::Center);
    }
    let total = para.line_count(inner.width).min(u16::MAX as usize) as u16;

    s.viewport = inner.height;
    s.max_scroll = total.saturating_sub(inner.height);
    if is_entry {
        s.scroll = 0;
    } else if s.follow {
        s.scroll = s.max_scroll;
    } else {
        s.scroll = s.scroll.min(s.max_scroll);
    }

    frame.render_widget(para.block(block).scroll((s.scroll, 0)), chunk);
}

fn render_message(msg: &mewcode_protocol::Message, theme: Theme) -> Vec<Line<'static>> {
    let (label, label_style) = match msg.role {
        Role::User => ("you", Style::default().fg(theme.hot_pink)),
        Role::Assistant => ("assistant", Style::default().fg(theme.psy_cyan)),
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
