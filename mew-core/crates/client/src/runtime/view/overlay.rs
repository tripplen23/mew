use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Clear, Paragraph, Wrap};

use mewcode_protocol::Mode;
use mewcode_protocol::ModelId;
use mewcode_protocol::ProviderId;
use mewcode_protocol::tool::tools_for_mode;

use super::super::model::{SLASH_COMMANDS, SessionState, ThemeId};
use super::super::update::picker::filtered_files;

/// The `/tools` overlay body: the tools available in the active mode plus
/// the total count. Mirrors the mode gating in the engine's tool registry
/// (`Plan` is read-only; `Build` adds the write tools).
pub(super) fn tools_lines(mode: Mode) -> Vec<Line<'static>> {
    let tools = tools_for_mode(mode);
    let mut lines: Vec<Line> = tools.iter().map(|t| Line::from(format!("• {t}"))).collect();
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        format!("{} tools available in {} mode", tools.len(), mode.label()),
        Style::default().fg(Color::DarkGray),
    )));
    lines
}

/// The `/skills` overlay body, built from the catalog fetched via
/// `GET /skills`. `None` is the fetch-in-flight / fetch-failed state;
/// an empty list means the server found no skills.
pub(super) fn skills_lines(s: &SessionState) -> Vec<Line<'static>> {
    let Some(entries) = s.skills.as_ref() else {
        return vec![Line::from(Span::styled(
            "Loading skills...",
            Style::default().fg(Color::DarkGray),
        ))];
    };
    if entries.is_empty() {
        return vec![Line::from(Span::styled(
            "No skills loaded.",
            Style::default().fg(Color::DarkGray),
        ))];
    }
    let mut lines: Vec<Line> = Vec::with_capacity(entries.len() * 2);
    for e in entries {
        lines.push(Line::from(vec![
            Span::styled(
                format!("• {}", e.name),
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("  ({})", e.source),
                Style::default().fg(Color::DarkGray),
            ),
        ]));
        if !e.description.is_empty() {
            lines.push(Line::from(Span::styled(
                format!("  {}", e.description),
                Style::default().fg(Color::Gray),
            )));
        }
    }
    lines
}

pub(super) fn theme_lines() -> Vec<Line<'static>> {
    let current = ThemeId::default();
    vec![
        Line::from(vec![
            Span::styled("* ", Style::default().fg(Color::Cyan)),
            Span::styled(
                current.as_str().to_string(),
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("  {}", current.display_name()),
                Style::default().fg(Color::Gray),
            ),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            "More themes can plug into this list later.",
            Style::default().fg(Color::DarkGray),
        )),
    ]
}

pub(super) fn render_slash_picker(frame: &mut Frame, area: Rect, s: &SessionState) {
    let row_count = SLASH_COMMANDS.len() as u16;
    let max_height = fallback(area.height.saturating_sub(4), 1);
    let height = row_count.saturating_add(2).min(max_height.max(3));
    let input_y = area
        .y
        .saturating_add(area.height)
        .saturating_sub(1)
        .saturating_sub(3);
    let panel_y = input_y.saturating_sub(height);
    let panel = Rect {
        x: area.x,
        y: panel_y,
        width: area.width,
        height,
    };

    let block = Block::bordered()
        .title(" commands  (↑↓ to move, Enter to run, Esc to close) ")
        .border_style(Style::default().fg(Color::Cyan));
    let inner = block.inner(panel);
    frame.render_widget(Clear, panel);
    frame.render_widget(block, panel);

    let cmd_w = SLASH_COMMANDS
        .iter()
        .map(|c| c.command.chars().count())
        .max()
        .unwrap_or(0);
    let lines: Vec<Line> = SLASH_COMMANDS
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let style = if i == s.slash_cursor {
                Style::default().fg(Color::Black).bg(Color::Cyan)
            } else {
                Style::default()
            };
            let cmd = format!("{:<cmd_w$}", c.command);
            Line::from(Span::styled(format!(" {cmd}  {}", c.description), style))
        })
        .collect();
    frame.render_widget(
        Paragraph::new(Text::from(lines)).wrap(Wrap { trim: false }),
        inner,
    );
}

pub(super) fn file_picker_lines(s: &SessionState, max_width: usize) -> Vec<Line<'static>> {
    const ELLIPSIS: &str = "…";
    if s.file_picker.files.is_none() {
        return vec![Line::from(Span::styled(
            "Loading files...",
            Style::default().fg(Color::DarkGray),
        ))];
    }
    let files = filtered_files(s);
    if files.is_empty() {
        return vec![Line::from(Span::styled(
            "No matching files.",
            Style::default().fg(Color::DarkGray),
        ))];
    }
    files
        .iter()
        .enumerate()
        .skip(s.file_picker.picker.scroll)
        .map(|(i, file)| {
            let style = if i == s.file_picker.picker.cursor {
                Style::default().fg(Color::Black).bg(Color::Cyan)
            } else {
                Style::default()
            };
            Line::from(Span::styled(
                format!(
                    " {}",
                    truncate_with_ellipsis(&file.path, max_width.saturating_sub(1), ELLIPSIS)
                ),
                style,
            ))
        })
        .collect()
}

fn fallback(value: u16, default: u16) -> u16 {
    if value == 0 { default } else { value }
}

/// Body of the `/model` overlay: every provider model entry, grouped
/// by provider, with the active session's current model tagged and the
/// cursor row highlighted. `None` `s.model_picker.models` is the "fetch
/// in flight" / "fetch failed" state.
///
/// The helper returns the **visible window** of rows (after applying
/// `s.model_picker.picker.scroll`) so the list scrolls cleanly when there are
/// more models than the overlay can fit on screen. The cursor highlight
/// still reflects the full-list index, so the caller doesn't need to
/// translate between window-local and global rows.
///
/// `max_width` is the inner width of the overlay panel; each row is
/// truncated to fit so the picker never wraps a model onto two visual
/// lines.
pub fn model_picker_lines(s: &SessionState, max_width: usize) -> Vec<Line<'static>> {
    let Some(entries) = s.model_picker.models.as_ref() else {
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
    let current = s
        .session
        .as_ref()
        .map(|sess| sess.model)
        .or(s.pending_model);

    // Build flat rows with an offset map so the cursor still indexes into
    // entries while we insert group headers.
    let mut rows: Vec<Row> = Vec::with_capacity(entries.len() + 4);
    let mut prev_provider: Option<ProviderId> = None;
    for (i, m) in entries.iter().enumerate() {
        if prev_provider != Some(m.provider) {
            rows.push(Row::Header {
                label: m.provider.to_string(),
            });
            prev_provider = Some(m.provider);
        }
        rows.push(Row::Model {
            entry_idx: i,
            is_current: m.id.parse::<ModelId>().ok() == current,
        });
    }

    // Translate cursor from entry-index to row-index.
    let cursor_row = cursor_to_row(&rows, s.model_picker.picker.cursor);

    let start = s
        .model_picker
        .picker
        .scroll
        .min(rows.len().saturating_sub(1));
    rows.iter()
        .enumerate()
        .skip(start)
        .map(|(row_i, row)| match row {
            Row::Header { label } => {
                let style = Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD);
                Line::from(Span::styled(format!(" {} ", label), style))
            }
            Row::Model {
                entry_idx,
                is_current,
            } => {
                let m = &entries[*entry_idx];
                let marker = if *is_current { "*" } else { " " };
                let style = if row_i == cursor_row {
                    Style::default().fg(Color::Black).bg(Color::Cyan)
                } else {
                    Style::default()
                };
                Line::from(Span::styled(
                    format_model_row(marker, &m.display_name, &m.id, max_width),
                    style,
                ))
            }
        })
        .collect()
}

enum Row {
    Header { label: String },
    Model { entry_idx: usize, is_current: bool },
}

fn cursor_to_row(rows: &[Row], cursor: usize) -> usize {
    let mut model_count = 0;
    for (i, row) in rows.iter().enumerate() {
        if let Row::Model { .. } = row {
            if model_count == cursor {
                return i;
            }
            model_count += 1;
        }
    }
    cursor.min(rows.len().saturating_sub(1))
}

/// Format a single model-picker row, truncated to fit `max_width` so
/// the row never wraps. Shows the parenthesised id when there's room;
/// falls back to the display name alone when the id would push the row
/// over the limit. The `* ` / `  ` marker is always preserved so the
/// "current model" indicator stays aligned.
fn format_model_row(marker: &str, display_name: &str, id: &str, max_width: usize) -> String {
    const ELLIPSIS: &str = "…";
    // 2 = marker char + the space after it. Leave 1 cell of slack so
    // wide CJK glyphs don't accidentally push past the panel.
    let overhead = marker.chars().count() + 1 + 1;
    if max_width <= overhead {
        return marker.to_string();
    }
    let budget = max_width - overhead;
    let id_part = format!(" ({id})");
    let id_w = id_part.chars().count();
    if id_w <= budget {
        let name_budget = budget - id_w;
        let name = truncate_with_ellipsis(display_name, name_budget, ELLIPSIS);
        return format!("{marker} {name}{id_part}");
    }
    // Not enough room for the id; show just the name, truncated to the
    // full budget.
    let name = truncate_with_ellipsis(display_name, budget, ELLIPSIS);
    format!("{marker} {name}")
}

/// Truncate `s` so it occupies at most `max_width` display cells.
/// Replaces the tail with `ellipsis` when truncation is needed.
/// If `max_width` is smaller than the ellipsis itself, the ellipsis is
/// clipped to whatever fits.
fn truncate_with_ellipsis(s: &str, max_width: usize, ellipsis: &str) -> String {
    if max_width == 0 {
        return String::new();
    }
    let w = s.chars().count();
    if w <= max_width {
        return s.to_string();
    }
    let ell_w = ellipsis.chars().count();
    if max_width <= ell_w {
        return ellipsis.chars().take(max_width).collect();
    }
    let keep = max_width - ell_w;
    let head: String = s.chars().take(keep).collect();
    format!("{head}{ellipsis}")
}

/// Body of the `/session` overlay: every saved session as a one-line
/// summary, newest-first, with the cursor row highlighted. Sliced by
/// `s.session_list.picker.scroll` so long lists stay usable.
///
/// `max_width` is the inner width of the overlay panel; each row is
/// truncated to fit so titles never wrap onto two visual lines (see
/// [`model_picker_lines`] for the same rationale).
pub fn session_list_lines(s: &SessionState, max_width: usize) -> Vec<Line<'static>> {
    if s.session_list.summaries.is_empty() {
        return vec![Line::from(Span::styled(
            "No saved sessions.",
            Style::default().fg(Color::DarkGray),
        ))];
    }
    let start = s
        .session_list
        .picker
        .scroll
        .min(s.session_list.summaries.len().saturating_sub(1));
    s.session_list
        .summaries
        .iter()
        .enumerate()
        .skip(start)
        .map(|(i, summary)| {
            let style = if i == s.session_list.picker.cursor {
                Style::default().fg(Color::Black).bg(Color::Cyan)
            } else {
                Style::default()
            };
            let row = format_session_row(&summary.title, summary.model.as_str(), max_width);
            Line::from(Span::styled(row, style))
        })
        .collect()
}

fn format_session_row(title: &str, model: &str, max_width: usize) -> String {
    const ELLIPSIS: &str = "…";
    // Leading + trailing two-space padding, plus the ` (model)` tail.
    // When the model doesn't fit, fall back to the title alone.
    let tail = format!(" ({model})");
    let tail_w = tail.chars().count();
    let prefix = 2usize; // leading "  "
    let suffix = 2usize; // trailing "  "
    if max_width <= prefix {
        return String::new();
    }
    let budget = max_width - prefix;
    if tail_w + suffix <= budget {
        let title_budget = budget - tail_w - suffix;
        let t = truncate_with_ellipsis(title, title_budget, ELLIPSIS);
        return format!("  {t}{tail}  ");
    }
    let t = truncate_with_ellipsis(title, budget, ELLIPSIS);
    format!("  {t}  ")
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

/// Centred overlay rect, matching the size used by [`render_overlay`].
/// Exposed so callers that build their own body lines (e.g. to truncate
/// to the inner width) can render into the same rect.
pub(super) fn centered_rect(area: Rect, percent_x: u16, percent_y: u16) -> Rect {
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

/// Like [`render_overlay`] but for a list that may exceed the panel
/// height. `body` is the slice of rows starting at `scroll`; the
/// function truncates it to fit the panel, pads to the footer row, and
/// renders.
///
/// `viewport_out` is set to the **number of rows actually available for
/// list items** (panel inner height minus the footer row). The update
/// loop reads this back to clamp the scroll offset when the cursor
/// moves — if the value is wrong (e.g. the raw inner height without
/// subtracting the footer), the cursor can scroll off the bottom in a
/// small terminal even though the user is still pressing Down.
#[allow(clippy::too_many_arguments)]
pub(super) fn render_scrolled_overlay(
    frame: &mut Frame,
    area: Rect,
    title: &str,
    hint: &str,
    body: Vec<Line<'static>>,
    total_rows: usize,
    _scroll: usize,
    cursor: usize,
    viewport_out: &mut u16,
) {
    let rect = centered_rect(area, 60, 60);
    frame.render_widget(Clear, rect);
    let inner_height = rect.height.saturating_sub(2);
    // One row is reserved for the footer; the rest is the list viewport.
    let visible = inner_height.saturating_sub(1) as usize;

    // Truncate the body to the visible window so the footer row stays
    // clear and so the model's `viewport` matches what we actually drew.
    let mut lines: Vec<Line> = body.into_iter().take(visible).collect();
    // Pad with blank lines so the footer stays anchored at the bottom
    // even when the list is short.
    while lines.len() < visible {
        lines.push(Line::from(""));
    }
    // Footer: "<cursor+1>/<total>" so the user can see where the
    // cursor is, even when the cursor row is scrolled off the top or
    // bottom.
    let footer_text = if total_rows == 0 {
        " — ".to_string()
    } else {
        format!(" {}/{} ", cursor + 1, total_rows)
    };
    lines.push(Line::from(Span::styled(
        footer_text,
        Style::default().fg(Color::DarkGray),
    )));

    let block = Block::bordered()
        .title(format!(" {title}  ({hint}) "))
        .border_style(Style::default().fg(Color::Cyan));
    // Rows are pre-truncated to the inner width by the caller, so wrap
    // is unnecessary and would risk re-wrapping a truncated tail onto a
    // second line.
    frame.render_widget(Paragraph::new(Text::from(lines)).block(block), rect);

    // Report the *list* viewport, not the raw inner height — the footer
    // row is not part of the list.
    *viewport_out = visible as u16;
}
