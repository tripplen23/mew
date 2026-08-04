//! Composer rendering: the composer bar, its height computation, and the
//! queued-message header above it. `render_mentions` is re-exported by the
//! parent module because the transcript uses it too.

use std::sync::atomic::{AtomicU64, Ordering};

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Clear, Paragraph, Wrap};

use crate::net::SkillEntry;
use crate::runtime::model::{PASTED_MARKER_PREFIX, SessionState};
use crate::runtime::view::theme::{COMPOSER_HORIZONTAL_PAD, COMPOSER_LEFT_PAD, Theme};

/// Maximum height (rows) the composer may grow to.
const MAX_COMPOSER_HEIGHT: u16 = 10;

/// Maximum number of queued-message rows shown.
const MAX_QUEUE_ROWS: usize = 3;

/// Characters of a queued message shown before ellipsis.
const MAX_QUEUE_PREVIEW_CHARS: usize = 80;

static DOT_FRAME: AtomicU64 = AtomicU64::new(0);
const DOT_BLINK_FRAMES: u64 = 10; // ~500 ms per phase at 50 ms tick

/// Rows needed by the composer header: 1 separator line + queued messages.
pub(super) fn queue_display_height(s: &SessionState) -> u16 {
    let len = s.message_queue.len();
    let shown = len.min(MAX_QUEUE_ROWS);
    let overflow_row = if len > MAX_QUEUE_ROWS { 1 } else { 0 };
    1 + shown as u16 + overflow_row
}

/// Render the dashed header above the composer bar, matching the transcript's
/// "Compaction" header. Empty queue: a "Composer" row with a context hint;
/// non-empty: the FIFO backlog — the only feedback that a message sent
/// mid-turn was queued, not dropped.
pub(super) fn render_message_queue(frame: &mut Frame, chunk: Rect, s: &SessionState, theme: Theme) {
    if chunk.height == 0 {
        return;
    }
    frame.render_widget(
        Block::default().style(Style::default().bg(theme.ink_bg)),
        chunk,
    );

    let [dash_row, queue_area] = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(0)])
        .split(chunk)[0..2]
    else {
        return;
    };

    let hint = if s.streaming.is_some() || s.compaction.active {
        "queued on send"
    } else {
        "Enter to send"
    };
    let label = format!(" Composer · {hint} ");
    // `·` is 3 bytes but 1 column, so count chars; odd leftover goes right.
    let total_dashes = (dash_row.width as usize).saturating_sub(label.chars().count());
    let left_len = total_dashes / 2;
    let right_len = total_dashes - left_len;
    let header = format!("{}{}{}", "─".repeat(left_len), label, "─".repeat(right_len));
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            header,
            Style::default().fg(theme.muted),
        ))),
        dash_row,
    );

    if s.message_queue.is_empty() {
        return;
    }

    let shown = s.message_queue.len().min(MAX_QUEUE_ROWS);
    let tick = DOT_FRAME.fetch_add(1, Ordering::Relaxed);
    let dot = if (tick / DOT_BLINK_FRAMES) % 2 == 0 {
        "●"
    } else {
        "○"
    };
    let mut lines: Vec<Line> = s.message_queue[..shown]
        .iter()
        .enumerate()
        .map(|(i, text)| {
            let char_count = text.chars().count();
            let preview: String = text.chars().take(MAX_QUEUE_PREVIEW_CHARS).collect();
            let preview = if char_count > MAX_QUEUE_PREVIEW_CHARS {
                format!("{preview}…")
            } else {
                preview
            };
            Line::from(vec![
                Span::styled(
                    format!(" {dot} message_queue[{i}]: "),
                    Style::default()
                        .fg(theme.queue_fg)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(preview, Style::default().fg(theme.text)),
                Span::styled(" (status: pending)", Style::default().fg(theme.muted)),
            ])
        })
        .collect();

    if s.message_queue.len() > MAX_QUEUE_ROWS {
        let remaining = s.message_queue.len() - MAX_QUEUE_ROWS;
        lines.push(Line::from(Span::styled(
            format!(" … +{remaining} more queued"),
            Style::default().fg(theme.muted),
        )));
    }

    frame.render_widget(Paragraph::new(Text::from(lines)), queue_area);
}

pub(super) fn composer_height(area: Rect, composer_text: &str) -> u16 {
    let composer_wrap = Paragraph::new(composer_text).wrap(Wrap { trim: false });
    let composer_lines = composer_wrap
        .line_count(area.width.saturating_sub(COMPOSER_HORIZONTAL_PAD))
        .max(1)
        .min(u16::MAX as usize) as u16;
    let max_height = MAX_COMPOSER_HEIGHT.min(area.height.saturating_sub(2));
    composer_lines.saturating_add(1).clamp(2, max_height.max(2))
}

pub(super) fn render_composer(
    frame: &mut Frame,
    chunk: Rect,
    composer_text: &str,
    skills: Option<&[SkillEntry]>,
    theme: Theme,
) {
    frame.render_widget(
        Block::default().style(Style::default().bg(theme.panel_bg)),
        chunk,
    );
    for offset in 0..chunk.height {
        frame.render_widget(
            Paragraph::new("▏").style(Style::default().fg(theme.hot_pink).bg(theme.panel_bg)),
            Rect::new(
                chunk.x,
                chunk.y.saturating_add(offset),
                1.min(chunk.width),
                1,
            ),
        );
    }

    let inner = Rect::new(
        chunk.x.saturating_add(COMPOSER_LEFT_PAD),
        chunk.y,
        chunk.width.saturating_sub(COMPOSER_HORIZONTAL_PAD),
        chunk.height,
    );
    frame.render_widget(Clear, inner);
    frame.render_widget(
        Block::default().style(Style::default().bg(theme.panel_bg)),
        inner,
    );
    let lines = composer_text
        .lines()
        .enumerate()
        .map(|(i, line)| {
            if i == 0 {
                first_line(line, skills, theme)
            } else {
                composer_line(line, theme)
            }
        })
        .collect::<Vec<_>>();
    let composer = Paragraph::new(Text::from(lines))
        .style(Style::default().fg(theme.text).bg(theme.panel_bg))
        .wrap(Wrap { trim: false });
    frame.render_widget(composer, inner);
}

/// First composer line: a leading `/skill-name` that matches the loaded
/// catalog renders as a chip, mirroring how the server expands it.
fn first_line(line: &str, skills: Option<&[SkillEntry]>, theme: Theme) -> Line<'static> {
    let Some(name) = leading_skill_name(line, skills) else {
        return composer_line(line, theme);
    };
    let mut spans = vec![Span::styled(
        format!("/{name}"),
        Style::default()
            .fg(theme.chip_fg)
            .bg(theme.lavender)
            .add_modifier(Modifier::BOLD),
    )];
    spans.extend(line_spans(&line[name.len() + 1..], theme));
    Line::from(spans)
}

fn leading_skill_name<'a>(line: &'a str, skills: Option<&'a [SkillEntry]>) -> Option<&'a str> {
    let name = line.strip_prefix('/')?.split_whitespace().next()?;
    let known = skills.is_some_and(|sk| sk.iter().any(|entry| entry.name == name));
    known.then_some(name)
}

fn composer_line(line: &str, theme: Theme) -> Line<'static> {
    Line::from(line_spans(line, theme))
}

fn line_spans(line: &str, theme: Theme) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    let mut rest = line;

    while let Some(start) = rest.find(PASTED_MARKER_PREFIX) {
        if start > 0 {
            spans.extend(render_mentions(&rest[..start], theme));
        }
        let marked = &rest[start..];
        let Some(end) = marked.find(']') else {
            spans.push(Span::raw(marked.to_string()));
            return spans;
        };
        let end = end + 1;
        spans.push(Span::styled(
            marked[..end].to_string(),
            Style::default()
                .fg(theme.chip_fg)
                .bg(theme.lavender)
                .add_modifier(Modifier::BOLD),
        ));
        rest = &marked[end..];
    }

    if !rest.is_empty() {
        spans.extend(render_mentions(rest, theme));
    }
    spans
}

pub(crate) fn render_mentions(text: &str, theme: Theme) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    let mut pos = 0;
    while pos < text.len() {
        let Some(token_start_rel) = text[pos..].find(|c: char| !c.is_whitespace()) else {
            spans.push(Span::raw(text[pos..].to_string()));
            return spans;
        };
        let token_start = pos + token_start_rel;
        if token_start > pos {
            spans.push(Span::raw(text[pos..token_start].to_string()));
        }
        let token_end = text[token_start..]
            .find(char::is_whitespace)
            .map_or(text.len(), |i| token_start + i);
        let token = &text[token_start..token_end];
        if token.starts_with('@') && token.len() > 1 {
            let is_dir = token.ends_with('/');
            let color = if is_dir {
                theme.psy_cyan
            } else {
                theme.mew_gold
            };
            spans.push(Span::styled(
                token.to_string(),
                Style::default().fg(color).add_modifier(Modifier::BOLD),
            ));
        } else {
            spans.push(Span::raw(token.to_string()));
        }
        pos = token_end;
    }
    spans
}
