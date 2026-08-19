//! Todo dock rendering: the session's task list as a compact checkbox list
//! between the transcript and the composer, styled like the queued-message
//! header above the composer. Collapsible via mouse click on the header row;
//! hidden entirely when the list is empty.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};

use mewcode_protocol::{TodoItem, TodoStatus};

use crate::runtime::model::SessionState;
use crate::runtime::view::theme::Theme;

/// Maximum rows the dock may occupy when expanded. Bounded so a long list
/// cannot crowd out the transcript on a short terminal.
pub const DOCK_MAX_ROWS: u16 = 10;

/// A finished list (every item completed) is noise: it only eats terminal
/// space the next task's list will need. The transcript keeps the record.
fn all_completed(s: &SessionState) -> bool {
    !s.todos.is_empty() && s.todos.iter().all(|t| t.status == TodoStatus::Completed)
}

/// Height of the dock for the current state: hidden when empty or fully
/// completed, the header row plus item rows otherwise (collapsed shows the
/// header only).
pub fn dock_height(s: &SessionState) -> u16 {
    if s.todos.is_empty() || all_completed(s) {
        0
    } else if s.todos_collapsed {
        1
    } else {
        (s.todos.len() as u16).min(DOCK_MAX_ROWS) + 1
    }
}

/// Render the todo dock into `chunk` (already vertically sized to
/// [`dock_height`]). Records the header row's absolute rect on
/// [`SessionState::dock_header`] so `update` can route mouse clicks.
pub(super) fn render_dock(frame: &mut Frame, chunk: Rect, s: &mut SessionState, theme: Theme) {
    if s.todos.is_empty() || all_completed(s) {
        s.dock_header = None;
        return;
    }
    frame.render_widget(
        Block::default().style(Style::default().bg(theme.ink_bg)),
        chunk,
    );

    let [header_row, body_area] = ratatui::layout::Layout::default()
        .direction(ratatui::layout::Direction::Vertical)
        .constraints([
            ratatui::layout::Constraint::Length(1),
            ratatui::layout::Constraint::Min(0),
        ])
        .split(chunk)[0..2]
    else {
        return;
    };
    s.dock_header = Some(header_row);

    let done = s
        .todos
        .iter()
        .filter(|t| t.status == TodoStatus::Completed)
        .count();
    let hint = if s.todos_collapsed {
        "▴ click to expand"
    } else {
        "▾ click to hide"
    };
    let label = format!(" Tasks · {done}/{} · {hint} ", s.todos.len());
    // `·` is 3 bytes but 1 column, so count chars; odd leftover goes right.
    let total_dashes = (header_row.width as usize).saturating_sub(label.chars().count());
    let left_len = total_dashes / 2;
    let right_len = total_dashes - left_len;
    let header = format!("{}{}{}", "─".repeat(left_len), label, "─".repeat(right_len));
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            header,
            Style::default()
                .fg(theme.psy_cyan)
                .add_modifier(Modifier::BOLD),
        ))),
        header_row,
    );

    if s.todos_collapsed {
        return;
    }

    let mut lines: Vec<Line<'static>> = Vec::new();
    for item in s.todos.iter().take(DOCK_MAX_ROWS as usize) {
        lines.push(render_item(item, theme));
    }
    frame.render_widget(Paragraph::new(lines), body_area);
}

fn render_item<'a>(item: &TodoItem, theme: Theme) -> Line<'a> {
    let (glyph, style) = match item.status {
        TodoStatus::Completed => ("[x]", Style::default().fg(theme.muted)),
        TodoStatus::InProgress => ("[~]", Style::default().fg(theme.mew_gold)),
        TodoStatus::Pending => ("[ ]", Style::default().fg(theme.text)),
    };
    let glyph_span = Span::styled(glyph, style);
    if item.status == TodoStatus::Completed {
        let content = Span::styled(
            item.content.clone(),
            Style::default()
                .fg(theme.muted)
                .add_modifier(Modifier::CROSSED_OUT),
        );
        Line::from(vec![glyph_span, Span::raw(" "), content])
    } else {
        Line::from(vec![
            glyph_span,
            Span::raw(" "),
            Span::styled(item.content.clone(), style),
        ])
    }
}
