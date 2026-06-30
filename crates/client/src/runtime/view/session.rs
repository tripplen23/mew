use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Paragraph, Wrap};

use mewcode_protocol::Mode;
use mewcode_protocol::{MessagePart, ModelId, Role};

use super::super::model::{Overlay, SessionState};
use super::markdown::render_markdown;
use super::overlay::{render_overlay, skills_lines, tools_lines};
use super::park_cursor_in_field;
use super::spinner::spinner_frame;
use super::tool_card::{
    render_tool_call_header, render_tool_result_body, render_tool_result_header,
};

/// Maximum height (rows) the input field may grow to. Wrapped text beyond
/// this still wraps, but the input box stops expanding so the transcript
/// can't be swallowed.
const MAX_INPUT_HEIGHT: u16 = 10;

/// Session: scrollable transcript, input bar, status bar, plus overlays.
///
/// When `s.session` is `None` (the entry state, before the user has sent
/// their first message), the transcript shows a one-line "type to start"
/// hint and the status bar reflects the placeholder. Once a session is
/// created, the real title/model/mode are used.
///
/// The input bar's height is computed from the wrapped text — long inputs
/// grow it (up to [`MAX_INPUT_HEIGHT`]) and shrink back when cleared, while
/// the transcript fills the rest.
pub(super) fn render_session(frame: &mut Frame, area: Rect, s: &mut SessionState) {
    // The input spans the full width, so its inner width is `area.width - 2`
    // (one cell of border on each side). We measure wrapped lines against
    // that before splitting so the constraint can use the real count.
    let inner_input_width = area.width.saturating_sub(2);
    let input_text = s.input.lines().join("\n");
    let input_wrap = Paragraph::new(input_text.as_str()).wrap(Wrap { trim: false });
    let input_lines = input_wrap
        .line_count(inner_input_width)
        .max(1)
        .min(u16::MAX as usize) as u16;
    let max_input = MAX_INPUT_HEIGHT.min(area.height.saturating_sub(2));
    let input_height = input_lines.saturating_add(2).clamp(3, max_input.max(3));

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(1),               // transcript
            Constraint::Length(input_height), // input bar (grows with text)
            Constraint::Length(1),            // status bar
        ])
        .split(area);

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
    let inner = block.inner(chunks[0]);

    // Measure the wrapped height at the inner width (the same width the text is
    // rendered into below), so "the bottom" is computed exactly. `line_count`
    // is the ratatui-unstable API enabled in Cargo.toml.
    let para = Paragraph::new(Text::from(lines)).wrap(Wrap { trim: false });
    let total = para.line_count(inner.width).min(u16::MAX as usize) as u16;

    s.viewport = inner.height;
    s.max_scroll = total.saturating_sub(inner.height);
    if s.follow {
        s.scroll = s.max_scroll;
    } else {
        s.scroll = s.scroll.min(s.max_scroll);
    }

    frame.render_widget(para.block(block).scroll((s.scroll, 0)), chunks[0]);

    let input = Paragraph::new(input_text)
        .block(Block::bordered().title(" message "))
        .wrap(Wrap { trim: false });
    frame.render_widget(input, chunks[1]);

    let status = match (s.streaming.is_some(), &s.session) {
        (true, Some(session)) => format!(
            "{}  {:?}  •  streaming…",
            session.model.display_name(),
            session.mode
        ),
        (false, Some(session)) => format!(
            "{}  {:?}  •  PgUp/PgDn scroll  •  /tools  /skills  •  type 'quit' to exit",
            session.model.display_name(),
            session.mode
        ),
        (true, None) => "starting session…".to_string(),
        (false, None) => format!(
            "{}  {}  •  /tools  /skills  •  type 'quit' to exit",
            ModelId::default().display_name(),
            Mode::default().as_str()
        ),
    };
    frame.render_widget(
        Paragraph::new(status).style(Style::default().fg(Color::DarkGray)),
        chunks[2],
    );

    if s.overlay == Overlay::None {
        park_cursor_in_field(frame, chunks[1], &s.input);
    }

    match s.overlay {
        Overlay::None => {}
        Overlay::Tools => render_overlay(frame, area, "Tools", tools_lines()),
        Overlay::Skills => render_overlay(frame, area, "Skills", skills_lines()),
    }
}

/// Render one persisted message, preserving the arrival order of its parts.
/// Text is markdown; tool calls and results are summarised inline.
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

    // Tracks the id of the most recent `ToolCall` part seen.
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
