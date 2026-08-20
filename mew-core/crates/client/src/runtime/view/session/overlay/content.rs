use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Clear, Paragraph, Wrap};

use mewcode_protocol::Mode;
use mewcode_protocol::ModelId;
use mewcode_protocol::ProviderId;
use mewcode_protocol::tool::allowed_tools_for_mode;

use crate::runtime::model::{SLASH_COMMANDS, SessionState, ThemeId};
use crate::runtime::view::panel::scroll_start_for_cursor;
use crate::runtime::view::text_cursor_glyph;

/// The `/tools` overlay body: the tools allowed in the active mode plus
/// the total count. Engine may also expose denied tools to the model so it can
/// receive explicit permission feedback, but those are not user-available here.
pub(super) fn tools_lines(mode: Mode) -> Vec<Line<'static>> {
    let tools = allowed_tools_for_mode(mode);
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
pub(super) fn skills_lines(s: &SessionState, max_width: usize) -> Vec<Line<'static>> {
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
    let start = s.skills_picker.scroll.min(entries.len().saturating_sub(1));
    entries
        .iter()
        .enumerate()
        .skip(start)
        .map(|(i, entry)| {
            let style = if i == s.skills_picker.cursor {
                Style::default().fg(Color::Black).bg(Color::Cyan)
            } else {
                Style::default().add_modifier(Modifier::BOLD)
            };
            Line::from(Span::styled(
                truncate_with_ellipsis(&format!(" {}", entry.name), max_width, "…"),
                style,
            ))
        })
        .collect()
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

pub(super) fn choice_lines(s: &SessionState) -> (Vec<Line<'static>>, usize) {
    let Some(choice) = s.pending_choice.as_ref() else {
        return (vec![Line::from("No pending choice.")], 0);
    };
    let mut lines = vec![
        Line::from(Span::styled(
            choice.request.title.clone(),
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::from(choice.request.prompt.clone()),
        Line::from(""),
    ];
    let mut cursor_line = 0;
    for (i, option) in choice.request.options.iter().enumerate() {
        if i == choice.picker.cursor {
            cursor_line = lines.len();
        }
        let marker = if i == choice.picker.cursor {
            "›"
        } else {
            " "
        };
        lines.push(Line::from(vec![
            Span::styled(format!("{marker} "), Style::default().fg(Color::Cyan)),
            Span::styled(
                option.label.clone(),
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw(format!("  [{}]", option.id)),
        ]));
        if let Some(description) = &option.description {
            lines.push(Line::from(Span::styled(
                format!("    {description}"),
                Style::default().fg(Color::Gray),
            )));
        }
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "↑↓ move, Enter select, Esc cancel",
        Style::default().fg(Color::DarkGray),
    )));
    (lines, cursor_line)
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
    let visible = inner.height as usize;
    let start = scroll_start_for_cursor(s.slash_cursor, visible, SLASH_COMMANDS.len());
    let lines: Vec<Line> = SLASH_COMMANDS
        .iter()
        .enumerate()
        .skip(start)
        .take(visible)
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
    let files = s.filtered_files();
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
            let is_selected = i == s.file_picker.picker.cursor;
            let label = if file.is_dir {
                format!("{}/", file.path)
            } else {
                file.path.clone()
            };
            let display = truncate_with_ellipsis(&label, max_width.saturating_sub(1), ELLIPSIS);
            if is_selected {
                Line::from(Span::styled(
                    format!(" {display}"),
                    Style::default().fg(Color::Black).bg(Color::Cyan),
                ))
            } else if file.is_dir {
                Line::from(Span::styled(
                    format!(" {display}"),
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ))
            } else {
                Line::from(Span::styled(format!(" {display}"), Style::default()))
            }
        })
        .collect()
}

fn fallback(value: u16, default: u16) -> u16 {
    if value == 0 { default } else { value }
}

/// Body of the `/model` overlay: provider-grouped model rows, the active
/// model tagged and cursor row highlighted. `None` models = fetch in-flight
/// or failed.
///
/// Returns the visible window after scroll; the cursor highlight still uses
/// the full-list index, so callers need no window→global translation. Rows
/// are truncated to `max_width` so the picker never wraps.
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
        .or(s.creation.pending_model);

    // Offset map so the cursor still indexes into `entries` despite headers.
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
    // Space after the marker + 1 slack cell, so wide CJK glyphs don't push
    // past the panel.
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

/// Body of the `/session` overlay: saved-session one-liners, newest-first,
/// sliced by `s.session_list.picker.scroll`, cursor row highlighted. Rows
/// truncated to `max_width` so titles never wrap (see [`model_picker_lines`]).
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
    // Two-space leading/trailing padding + ` (model)` tail; falls back to
    // the title alone when the model won't fit.
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

/// Body of the rename overlay: a hint pointing to the composer bar where the
/// user is editing the new title. The actual title text is shown live in
/// the composer bar — the overlay just frames the action.
pub(super) fn rename_session_lines(s: &SessionState) -> Vec<Line<'static>> {
    let current = s.composer_text();
    let trimmed = current.trim();
    if trimmed.is_empty() {
        vec![Line::from(Span::styled(
            "(type a new title in the composer bar, then press Enter)",
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

/// Body of the `/connect` overlay: provider list and API-key prompt,
/// depending on `s.connect_provider.step`.
pub(super) fn connect_provider_lines(s: &SessionState) -> Vec<Line<'static>> {
    use crate::runtime::model::ConnectStep;
    let state = &s.connect_provider;
    match state.step {
        ConnectStep::PickProvider => {
            let providers = [ProviderId::OpenCodeGo, ProviderId::OpenAi];
            let mut lines = vec![Line::from("Select a provider to connect:")];
            for p in providers {
                let marker = if Some(p) == state.selected_provider {
                    "▶"
                } else {
                    " "
                };
                lines.push(Line::from(vec![
                    Span::raw(format!("  {marker} ")),
                    Span::styled(
                        p.to_string(),
                        if Some(p) == state.selected_provider {
                            Style::default()
                                .add_modifier(Modifier::BOLD)
                                .fg(Color::Cyan)
                        } else {
                            Style::default()
                        },
                    ),
                ]));
            }
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "↑↓ select  Enter confirm  Esc cancel",
                Style::default().fg(Color::DarkGray),
            )));
            lines
        }
        ConnectStep::EnterKey => {
            let provider = state
                .selected_provider
                .map(|p| p.to_string())
                .unwrap_or_default();
            let key_text = connect_provider_key_text(s);
            let mut lines = vec![
                Line::from(vec![
                    Span::raw("Provider: "),
                    Span::styled(provider, Style::default().add_modifier(Modifier::BOLD)),
                ]),
                Line::from(""),
                Line::from("Enter your API key (type directly):"),
                Line::from(""),
                Line::from(vec![Span::styled(
                    format!("  {key_text}{}", text_cursor_glyph(&key_text)),
                    Style::default().fg(Color::Yellow),
                )]),
            ];
            if let Some(ref error) = state.error {
                lines.push(Line::from(""));
                lines.push(Line::from(Span::styled(
                    format!("✗ {error}"),
                    Style::default().fg(Color::Red),
                )));
            }
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "Enter submit  Esc cancel",
                Style::default().fg(Color::DarkGray),
            )));
            lines
        }
        ConnectStep::Validating => vec![Line::from(Span::styled(
            "Validating API key...",
            Style::default().fg(Color::Yellow),
        ))],
        ConnectStep::Done => vec![
            Line::from(Span::styled(
                "✓ Connected successfully!",
                Style::default().fg(Color::Green),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "Enter or Esc to close",
                Style::default().fg(Color::DarkGray),
            )),
        ],
    }
}

pub(super) fn connect_provider_key_text(s: &SessionState) -> String {
    let mut text = s.connect_provider.key_input.lines().join("");
    text.push_str(&s.composer.lines().join(""));
    text
}
