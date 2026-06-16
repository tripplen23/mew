use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::widgets::{Block, Borders, Paragraph};

use mewcode_protocol::ModelId;

use super::super::model::{NewSessionField, NewSessionState};

/// NewSession: title input plus model and mode pickers, with the focused
/// field highlighted.
pub(super) fn render_new_session(frame: &mut Frame, area: Rect, n: &NewSessionState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // title
            Constraint::Length(3), // model
            Constraint::Length(3), // mode
            Constraint::Min(0),    // filler
            Constraint::Length(1), // footer
        ])
        .split(area);

    let title_text = n.title.lines().join("\n");
    frame.render_widget(
        field_block("Title", &title_text, n.field == NewSessionField::Title),
        chunks[0],
    );

    let model = ModelId::ALL.get(n.model_idx).copied().unwrap_or_default();
    frame.render_widget(
        field_block(
            "Model  (←/→)",
            &format!("‹ {} ›", model.display_name()),
            n.field == NewSessionField::Model,
        ),
        chunks[1],
    );

    frame.render_widget(
        field_block(
            "Mode  (←/→)",
            &format!("‹ {:?} ›", n.mode),
            n.field == NewSessionField::Mode,
        ),
        chunks[2],
    );

    let footer = Paragraph::new("Tab next field  •  Enter create  •  Esc cancel")
        .style(Style::default().fg(Color::DarkGray));
    frame.render_widget(footer, chunks[4]);
}

/// A single labelled field; the border brightens when focused.
fn field_block<'a>(label: &'a str, value: &str, focused: bool) -> Paragraph<'a> {
    let border = if focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    Paragraph::new(value.to_string()).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(border)
            .title(label),
    )
}
