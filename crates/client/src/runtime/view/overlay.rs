use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Clear, Paragraph};

use mewcode_protocol::tool::ToolName;

/// The `/tools` overlay body: every tool plus the total count.
pub(super) fn tools_lines() -> Vec<Line<'static>> {
    let mut lines: Vec<Line<'static>> = ToolName::ALL
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

/// Draw a centred, bordered overlay with a `Clear` underneath it.
pub(super) fn render_overlay(frame: &mut Frame, area: Rect, title: &str, body: Vec<Line<'static>>) {
    let rect = centered_rect(area, 60, 60);
    frame.render_widget(Clear, rect);
    let block = Block::bordered()
        .title(format!(" {title}  (Esc to close) "))
        .border_style(Style::default().fg(Color::Cyan));
    frame.render_widget(Paragraph::new(Text::from(body)).block(block), rect);
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
