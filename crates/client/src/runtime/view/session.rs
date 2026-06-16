use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Paragraph, Wrap};

use mewcode_protocol::{MessagePart, Role};

use super::super::model::{Overlay, SessionState};
use super::markdown::render_markdown;
use super::overlay::{render_overlay, skills_lines, tools_lines};
use super::spinner::spinner_frame;

/// Session: scrollable transcript, input bar, status bar, plus overlays.
pub(super) fn render_session(frame: &mut Frame, area: Rect, s: &SessionState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(1),    // transcript
            Constraint::Length(3), // input bar
            Constraint::Length(1), // status bar
        ])
        .split(area);

    // --- transcript ---
    let mut lines: Vec<Line> = Vec::new();
    for msg in &s.session.messages {
        lines.extend(render_message(msg));
        lines.push(Line::from(""));
    }
    // The in-flight assistant turn, if any.
    if let Some(st) = &s.streaming {
        lines.push(Line::from(Span::styled(
            format!("{} assistant", spinner_frame(st.started_at.elapsed())),
            Style::default().fg(Color::Yellow),
        )));
        if !st.buffer.is_empty() {
            lines.extend(render_markdown(&st.buffer));
        }
    }

    let transcript = Paragraph::new(Text::from(lines))
        .block(Block::bordered().title(format!(" {} ", s.session.title)))
        .wrap(Wrap { trim: false })
        .scroll((s.scroll, 0));
    frame.render_widget(transcript, chunks[0]);

    // --- input bar ---
    let input_text = s.input.lines().join("\n");
    let input = Paragraph::new(input_text)
        .block(Block::bordered().title(" message "))
        .wrap(Wrap { trim: false });
    frame.render_widget(input, chunks[1]);

    // --- status bar ---
    let status = if s.streaming.is_some() {
        format!(
            "{}  {:?}  •  streaming…",
            s.session.model.display_name(),
            s.session.mode
        )
    } else {
        format!(
            "{}  {:?}  •  /tools  /skills  •  Esc back",
            s.session.model.display_name(),
            s.session.mode
        )
    };
    frame.render_widget(
        Paragraph::new(status).style(Style::default().fg(Color::DarkGray)),
        chunks[2],
    );

    // --- overlays ---
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

    for part in &msg.parts {
        match part {
            MessagePart::Text { text } => out.extend(render_markdown(text)),
            MessagePart::ToolCall(call) => out.push(Line::from(Span::styled(
                format!("→ {}({})", call.name, call.input),
                Style::default().fg(Color::Magenta),
            ))),
            MessagePart::ToolResult(res) => out.push(Line::from(Span::styled(
                format!("← {} result", res.name),
                Style::default().fg(Color::Magenta),
            ))),
            MessagePart::FileMention { path } => out.push(Line::from(Span::styled(
                format!("@{path}"),
                Style::default().fg(Color::Blue),
            ))),
        }
    }
    out
}
