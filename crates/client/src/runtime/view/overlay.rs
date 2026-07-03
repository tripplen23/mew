use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Clear, Paragraph, Wrap};

use mewcode_protocol::ModelId;
use mewcode_protocol::tool::ToolName;

use super::super::model::SessionState;

/// The `/tools` overlay body: every tool plus the total count.
pub(super) fn tools_lines() -> Vec<Line<'static>> {
    let mut lines: Vec<Line> = ToolName::ALL
        .iter()
        .map(|t| Line::from(format!("• {t}")))
        .collect();
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        format!("{} tools available", ToolName::ALL.len()),
        Style::default().fg(Color::DarkGray),
    )));
    lines
}

/// The `/skills` overlay body. The skill catalog is loaded by the engine at
/// runtime; the view shows whatever the model carries — here a hint until the
/// catalog is wired through.
pub(super) fn skills_lines() -> Vec<Line<'static>> {
    vec![Line::from(Span::styled(
        "No skills loaded.",
        Style::default().fg(Color::DarkGray),
    ))]
}

/// Body of the `/model` overlay: every entry from `GET /models`, with the
/// active session's current model tagged and the cursor row highlighted.
/// `None` `s.models` is the "fetch in flight" / "fetch failed" state.
pub(super) fn model_picker_lines(s: &SessionState) -> Vec<Line<'static>> {
    let Some(entries) = s.models.as_ref() else {
        return vec![Line::from(Span::styled(
            "Loading models...",
            Style::default().fg(Color::DarkGray),
        ))];
    };
    if entries.is_empty() {
        return vec![Line::from(Span::styled(
            "No models available.",
            Style::default().fg(Color::DarkGray),
        ))];
    }
    let current = s.session.as_ref().map(|sess| sess.model);
    entries
        .iter()
        .enumerate()
        .map(|(i, m)| {
            let is_current = m.id.parse::<ModelId>().ok() == current;
            let marker = if is_current { "*" } else { " " };
            let style = if i == s.model_cursor {
                Style::default().fg(Color::Black).bg(Color::Cyan)
            } else {
                Style::default()
            };
            Line::from(Span::styled(
                format!("{marker} {} ({})", m.display_name, m.id),
                style,
            ))
        })
        .collect()
}

/// Body of the `/session` overlay: every saved session as a one-line
/// summary, newest-first, with the cursor row highlighted.
pub(super) fn session_list_lines(s: &SessionState) -> Vec<Line<'static>> {
    if s.session_summaries.is_empty() {
        return vec![Line::from(Span::styled(
            "No saved sessions.",
            Style::default().fg(Color::DarkGray),
        ))];
    }
    s.session_summaries
        .iter()
        .enumerate()
        .map(|(i, summary)| {
            let style = if i == s.session_cursor {
                Style::default().fg(Color::Black).bg(Color::Cyan)
            } else {
                Style::default()
            };
            Line::from(Span::styled(
                format!("  {}  ({})", summary.title, summary.model.as_str()),
                style,
            ))
        })
        .collect()
}

/// Body of the rename overlay: a hint pointing to the input bar where the
/// user is editing the new title. The actual title text is shown live in
/// the input bar — the overlay just frames the action.
pub(super) fn rename_session_lines(s: &SessionState) -> Vec<Line<'static>> {
    let current = s.input.lines().join("\n");
    let trimmed = current.trim();
    if trimmed.is_empty() {
        vec![Line::from(Span::styled(
            "(type a new title in the input bar, then press Enter)",
            Style::default().fg(Color::DarkGray),
        ))]
    } else {
        vec![
            Line::from(Span::styled(
                "New title:",
                Style::default().fg(Color::DarkGray),
            )),
            Line::from(Span::styled(
                trimmed.to_string(),
                Style::default().add_modifier(Modifier::BOLD),
            )),
        ]
    }
}

/// Draw a centred, bordered overlay with a `Clear` underneath it.
pub(super) fn render_overlay(frame: &mut Frame, area: Rect, title: &str, body: Vec<Line<'static>>) {
    let rect = centered_rect(area, 60, 60);
    frame.render_widget(Clear, rect);
    let block = Block::bordered()
        .title(format!(" {title}  (Esc to close) "))
        .border_style(Style::default().fg(Color::Cyan));
    frame.render_widget(
        Paragraph::new(Text::from(body))
            .wrap(Wrap { trim: false })
            .block(block),
        rect,
    );
}

fn centered_rect(area: Rect, percent_x: u16, percent_y: u16) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(vertical[1])[1]
}
