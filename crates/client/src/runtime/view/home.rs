use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Text};
use ratatui::widgets::{Block, List, ListItem, ListState, Paragraph};

use super::super::model::HomeState;

/// Home: a selectable session list with a loading / empty affordance.
pub(super) fn render_home(frame: &mut Frame, area: Rect, h: &HomeState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(area);

    let block = Block::bordered().title(" mewcode — sessions ");

    if h.loading {
        let p = Paragraph::new("Loading sessions…")
            .block(block)
            .alignment(Alignment::Center);
        frame.render_widget(p, chunks[0]);
    } else if h.sessions.is_empty() {
        let p = Paragraph::new(Text::from(vec![
            Line::from(""),
            Line::from("No sessions yet.").alignment(Alignment::Center),
            Line::from("Press 'n' to start a new one.").alignment(Alignment::Center),
        ]))
        .block(block);
        frame.render_widget(p, chunks[0]);
    } else {
        let items: Vec<ListItem> = h
            .sessions
            .iter()
            .map(|s| ListItem::new(s.title.clone()))
            .collect();
        let list = List::new(items)
            .block(block)
            .highlight_symbol("› ")
            .highlight_style(Style::default().add_modifier(Modifier::REVERSED));
        let mut state = ListState::default().with_selected(Some(h.selected));
        frame.render_stateful_widget(list, chunks[0], &mut state);
    }

    // The `c` binding is on the update side (`update/home.rs:18`) but
    // not advertised on older branches; keep the footer honest about
    // every key the user can press on Home.
    let footer = Paragraph::new("n new  •  c canvas  •  ↑/↓ select  •  Enter open  •  q quit")
        .style(Style::default().fg(Color::DarkGray));
    frame.render_widget(footer, chunks[1]);
}
