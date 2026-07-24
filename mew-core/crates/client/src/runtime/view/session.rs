use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Clear, Paragraph, Wrap};
use ratatui::Frame;
use std::sync::atomic::{AtomicU64, Ordering};

use super::super::model::{Overlay, SessionState};
use super::overlay::{
    centered_rect, render_overlay, render_scrolled_overlay, render_slash_picker, skills_lines,
    theme_lines, tools_lines,
};
use super::park_cursor_in_field;
use super::theme::{Theme, COMPOSER_HORIZONTAL_PAD, COMPOSER_LEFT_PAD};
use super::transcript::render_transcript;

/// Maximum height (rows) the input field may grow to.
const MAX_INPUT_HEIGHT: u16 = 10;

/// Session: scrollable transcript, input bar, status bar, plus overlays.
///
/// When `s.session` is `None` (the entry state, before the user has sent
/// their first message), the transcript shows a one-line "type to start"
/// hint and the status bar reflects the placeholder. Once a session is
/// created, the real title/model/mode are used.
///
/// The input bar's height is computed from the wrapped text — long inputs
/// grow it (up to [`MAX_INPUT_HEIGHT`]) and shrink back when cleared, while
/// the transcript fills the rest.
pub(super) fn render_session(frame: &mut Frame, area: Rect, s: &mut SessionState, theme: Theme) {
    let input_text = s.input.lines().join("\n");
    let input_height = input_height(area, &input_text);
    let queue_height = queue_display_height(s);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(1),               // transcript
            Constraint::Length(queue_height), // queued-message list (0 when empty)
            Constraint::Length(input_height), // input bar (grows with text)
            Constraint::Length(1),            // status bar
        ])
        .split(area);

    render_transcript(frame, chunks[0], s, theme);
    render_message_queue(frame, chunks[1], s, theme);
    render_input(frame, chunks[2], &input_text, theme);
    render_status(frame, chunks[3], s, theme);

    park_cursor_in_field(frame, chunks[2], &s.input);
    render_active_overlay(frame, area, s);
}

/// Maximum number of queued-message rows shown
const MAX_QUEUE_ROWS: usize = 3;

static DOT_FRAME: AtomicU64 = AtomicU64::new(0);
const DOT_BLINK_FRAMES: u64 = 10; // ~500 ms per phase at 50 ms tick

/// Rows needed by the composer header: 1 separator line + queued messages.
fn queue_display_height(s: &SessionState) -> u16 {
    let len = s.message_queue.len();
    let shown = len.min(MAX_QUEUE_ROWS);
    let overflow_row = if len > MAX_QUEUE_ROWS { 1 } else { 0 };
    1 + shown as u16 + overflow_row
}

/// Render the composer header directly above the input bar.
///
/// Two states, same visual language as the transcript's dashed "Compaction"
/// section header (`render_compaction_section`) so the boundary between
/// transcript and input never looks like an empty gap:
/// - **Empty queue:** a single dashed row labelled "Composer", with a
///   context-sensitive hint (what Enter does right now).
/// - **Non-empty queue:** the FIFO backlog, one row per queued message —
///   `message_queue[0]: <text> (status: pending)` — the only feedback that
///   a message typed while a turn was in flight was queued, not dropped.
///   See `on_session_submit`'s shared queueing branch.
fn render_message_queue(frame: &mut Frame, chunk: Rect, s: &SessionState, theme: Theme) {
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
    // `.chars().count()` not `.len()` — `·` is 3 bytes but 1 column.
    // Leftover from odd split goes to right, filling the row exactly.
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

    let shown = (s.message_queue.len()).min(MAX_QUEUE_ROWS);
    let tick = DOT_FRAME.fetch_add(1, Ordering::Relaxed);
    let dot = if (tick / DOT_BLINK_FRAMES) % 2 == 0 { "●" } else { "○" };
    let mut lines: Vec<Line> = s.message_queue[..shown]
        .iter()
        .enumerate()
        .map(|(i, text)| {
            let preview: String = text.chars().take(80).collect();
            let preview = if text.chars().count() > 80 {
                format!("{preview}…")
            } else {
                preview
            };
            Line::from(vec![
                Span::styled(
                    format!(" {dot} message_queue[{i}]: "),
                    Style::default()
                        .fg(Color::Rgb(255, 165, 0))
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

fn input_height(area: Rect, input_text: &str) -> u16 {
    let input_wrap = Paragraph::new(input_text).wrap(Wrap { trim: false });
    let input_lines = input_wrap
        .line_count(area.width.saturating_sub(COMPOSER_HORIZONTAL_PAD))
        .max(1)
        .min(u16::MAX as usize) as u16;
    let max_input = MAX_INPUT_HEIGHT.min(area.height.saturating_sub(2));
    input_lines.saturating_add(1).clamp(2, max_input.max(2))
}

fn render_input(frame: &mut Frame, chunk: Rect, input_text: &str, theme: Theme) {
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
    let lines = input_text
        .lines()
        .map(|line| input_line(line, theme))
        .collect::<Vec<_>>();
    let input = Paragraph::new(Text::from(lines))
        .style(Style::default().fg(theme.text).bg(theme.panel_bg))
        .wrap(Wrap { trim: false });
    frame.render_widget(input, inner);
}

fn render_status(frame: &mut Frame, chunk: Rect, s: &SessionState, theme: Theme) {
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
        .saturating_sub(left.len() as u16 + right.len() as u16);
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

fn input_line(line: &str, theme: Theme) -> Line<'static> {
    let mut spans = Vec::new();
    let mut rest = line;

    while let Some(start) = rest.find("[Pasted ~") {
        if start > 0 {
            spans.extend(render_mentions(&rest[..start], theme));
        }
        let marked = &rest[start..];
        let Some(end) = marked.find(']') else {
            spans.push(Span::raw(marked.to_string()));
            return Line::from(spans);
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
    Line::from(spans)
}

pub(super) fn render_mentions(text: &str, theme: Theme) -> Vec<Span<'static>> {
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

fn render_active_overlay(frame: &mut Frame, area: Rect, s: &mut SessionState) {
    match s.overlay {
        Overlay::None => {}
        Overlay::Tools => {
            let mode = s.session.as_ref().map(|sess| sess.mode).unwrap_or_default();
            render_overlay(frame, area, "Tools", tools_lines(mode))
        }
        Overlay::Skills => render_overlay(frame, area, "Skills", skills_lines(s)),
        Overlay::Theme => render_overlay(frame, area, "Theme", theme_lines()),
        Overlay::Choice => render_overlay(frame, area, "Choose", super::overlay::choice_lines(s)),
        Overlay::ModelPicker => {
            // Compute the overlay rect first so the row builder knows
            // the inner width and can truncate each model to a single
            // line — otherwise a long name wraps and the cursor
            // (one per model) desyncs from the visual highlight (one
            // per wrapped line).
            let rect = centered_rect(area, 60, 60);
            let inner_w = rect.width.saturating_sub(2) as usize;
            let body = super::overlay::model_picker_lines(s, inner_w);
            render_scrolled_overlay(
                frame,
                area,
                "Model",
                "Esc close",
                body,
                s.model_picker.models.as_ref().map(Vec::len).unwrap_or(0),
                s.model_picker.picker.scroll,
                s.model_picker.picker.cursor,
                &mut s.model_picker.picker.viewport,
            );
            // Track the largest viewport the view has ever reported so
            // a transient 0 (first frame, resize) doesn't make the
            // clamp think there's no room and leave the cursor
            // off-screen.
            if s.model_picker.picker.viewport > s.model_picker.picker.viewport_max {
                s.model_picker.picker.viewport_max = s.model_picker.picker.viewport;
            }
        }
        Overlay::SessionList => {
            let rect = centered_rect(area, 60, 60);
            let inner_w = rect.width.saturating_sub(2) as usize;
            let body = super::overlay::session_list_lines(s, inner_w);
            render_scrolled_overlay(
                frame,
                area,
                "Sessions",
                "Esc close, d delete",
                body,
                s.session_list.summaries.len(),
                s.session_list.picker.scroll,
                s.session_list.picker.cursor,
                &mut s.session_list.picker.viewport,
            );
            if s.session_list.picker.viewport > s.session_list.picker.viewport_max {
                s.session_list.picker.viewport_max = s.session_list.picker.viewport;
            }
        }
        Overlay::FilePicker => {
            const FILE_PICKER_WIDTH_PERCENT: u16 = 70;
            const FILE_PICKER_HEIGHT_PERCENT: u16 = 50;
            let rect = centered_rect(area, FILE_PICKER_WIDTH_PERCENT, FILE_PICKER_HEIGHT_PERCENT);
            let inner_w = rect.width.saturating_sub(2) as usize;
            let body = super::overlay::file_picker_lines(s, inner_w);
            render_scrolled_overlay(
                frame,
                area,
                "Files",
                "Enter insert, Esc close",
                body,
                super::super::update::picker::filtered_files(s).len(),
                s.file_picker.picker.scroll,
                s.file_picker.picker.cursor,
                &mut s.file_picker.picker.viewport,
            );
            if s.file_picker.picker.viewport > s.file_picker.picker.viewport_max {
                s.file_picker.picker.viewport_max = s.file_picker.picker.viewport;
            }
        }
        Overlay::RenameSession => render_overlay(
            frame,
            area,
            "Rename session",
            super::overlay::rename_session_lines(s),
        ),
        Overlay::SlashPicker => render_slash_picker(frame, area, s),
    }
}
