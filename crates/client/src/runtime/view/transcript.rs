//! Transcript rendering for the session screen.
//!
//! This module owns message-to-lines conversion plus scroll measurement. The
//! view writes the measured scroll bounds back into [`SessionState`] so key
//! handling can clamp PageUp/PageDown without doing layout work.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Paragraph, Wrap};

use mewcode_protocol::{MessagePart, Role};

use super::super::model::SessionState;
use super::markdown::render_markdown;
use super::spinner::spinner_frame;
use super::tool_card::{
    render_tool_call_header, render_tool_result_body, render_tool_result_header,
};

/// Render the transcript panel and update its scroll bounds.
pub(super) fn render_transcript(frame: &mut Frame, chunk: Rect, s: &mut SessionState) {
    let mut lines: Vec<Line> = Vec::new();
    match &s.session {
        Some(session) => {
            for msg in &session.messages {
                lines.extend(render_message(msg));
                lines.push(Line::from(""));
            }
        }
        None => {
            lines.push(Line::from(Span::styled(
                if let Some(started) = s.creation_started_at {
                    format!("{} starting session…", spinner_frame(started.elapsed()))
                } else {
                    "Type a message to start a new session.".to_string()
                },
                Style::default().fg(Color::DarkGray),
            )));
        }
    }
    if let Some(st) = &s.streaming {
        lines.push(Line::from(Span::styled(
            format!("{} assistant", spinner_frame(st.started_at.elapsed())),
            Style::default().fg(Color::Yellow),
        )));
        if !st.buffer.is_empty() {
            lines.extend(render_markdown(&st.buffer));
        }
    }

    let title = s
        .session
        .as_ref()
        .map(|sess| sess.title.as_str())
        .unwrap_or(" mewcode ");
    let block = Block::bordered().title(title);
    let inner = block.inner(chunk);
    let para = Paragraph::new(Text::from(lines)).wrap(Wrap { trim: false });
    let total = para.line_count(inner.width).min(u16::MAX as usize) as u16;

    s.viewport = inner.height;
    s.max_scroll = total.saturating_sub(inner.height);
    if s.follow {
        s.scroll = s.max_scroll;
    } else {
        s.scroll = s.scroll.min(s.max_scroll);
    }

    frame.render_widget(para.block(block).scroll((s.scroll, 0)), chunk);
}

fn render_message(msg: &mewcode_protocol::Message) -> Vec<Line<'static>> {
    let (label, label_style) = match msg.role {
        Role::User => ("you", Style::default().fg(Color::Green)),
        Role::Assistant => ("assistant", Style::default().fg(Color::Cyan)),
        Role::Tool => ("tool", Style::default().fg(Color::Magenta)),
    };
    let mut out = vec![Line::from(Span::styled(
        label.to_string(),
        label_style.add_modifier(Modifier::BOLD),
    ))];

    let mut last_tool_call_id: Option<&str> = None;
    for part in &msg.parts {
        match part {
            MessagePart::Text { text } => {
                last_tool_call_id = None;
                out.extend(render_markdown(text));
            }
            MessagePart::ToolCall(call) => {
                last_tool_call_id = Some(&call.id);
                out.push(render_tool_call_header(call));
            }
            MessagePart::ToolResult(res) => {
                let paired = last_tool_call_id == Some(&res.call_id);
                last_tool_call_id = None;
                if !paired {
                    out.push(render_tool_result_header(res));
                }
                out.extend(render_tool_result_body(res));
            }
            MessagePart::FileMention { path } => {
                last_tool_call_id = None;
                out.push(Line::from(Span::styled(
                    format!("@{path}"),
                    Style::default().fg(Color::Blue),
                )));
            }
        }
    }
    out
}
