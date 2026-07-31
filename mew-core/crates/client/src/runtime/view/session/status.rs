//! Status bar rendering: the bottom row showing pwd, token usage, mode,
//! and model, plus streaming/compaction indicators.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use unicode_width::UnicodeWidthStr;

use crate::runtime::model::SessionState;
use crate::runtime::view::theme::Theme;

pub(super) fn render_status(frame: &mut Frame, chunk: Rect, s: &SessionState, theme: Theme) {
    let (model, mode) = match &s.session {
        Some(session) => (session.model.display_name(), session.mode),
        None => (
            s.creation.pending_model.unwrap_or_default().display_name(),
            s.creation.pending_mode.unwrap_or_default(),
        ),
    };

    let pwd = s.pwd.as_deref().unwrap_or(".");
    let token_pct = if s.context_limit > 0 {
        (s.session_tokens as f64 / s.context_limit as f64) * 100.0
    } else {
        0.0
    };
    let token_display = format_tokens(s.session_tokens);

    let left = format!("  {pwd}");
    let right = format!(
        "{token_display} ({token_pct:.0}%)  ·  {}  ·  {}",
        mode.label(),
        model
    );

    let mut spans = vec![Span::styled(&left, Style::default().fg(theme.muted))];

    let padding = chunk
        .width
        .saturating_sub(left.width() as u16 + right.width() as u16);
    if padding > 0 {
        spans.push(Span::raw(" ".repeat(padding as usize)));
    }

    spans.push(Span::styled(&right, Style::default().fg(theme.muted)));

    if s.compaction.active {
        let elapsed = s
            .compaction
            .started_at
            .map(|t| t.elapsed().as_secs_f64())
            .unwrap_or(0.0);
        let dot = ".".repeat((elapsed as usize % 3) + 1);
        spans.push(Span::styled(
            format!("  ·  compacting{dot}"),
            Style::default().fg(theme.muted),
        ));
    } else if s.streaming.is_some() {
        spans.push(Span::styled(
            "  ·  streaming...",
            Style::default().fg(theme.muted),
        ));
    }

    frame.render_widget(Paragraph::new(Line::from(spans)), chunk);
}

fn format_tokens(tokens: u64) -> String {
    if tokens >= 1_000_000 {
        format!("{:.1}M", tokens as f64 / 1_000_000.0)
    } else if tokens >= 1_000 {
        format!("{:.1}K", tokens as f64 / 1_000.0)
    } else {
        tokens.to_string()
    }
}
