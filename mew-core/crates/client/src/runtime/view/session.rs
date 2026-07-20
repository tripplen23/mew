use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Clear, Paragraph, Wrap};

use super::super::model::{Overlay, SessionState};
use super::overlay::{
    centered_rect, render_overlay, render_scrolled_overlay, render_slash_picker, skills_lines,
    theme_lines, tools_lines,
};
use super::park_cursor_in_field;
use super::theme::{COMPOSER_HORIZONTAL_PAD, COMPOSER_LEFT_PAD, Theme};
use super::transcript::render_transcript;

/// Maximum height (rows) the input field may grow to. Wrapped text beyond
/// this still wraps, but the input box stops expanding so the transcript
/// can't be swallowed. Note: text that wraps past this height is clipped
/// at the bottom of the input box (the input `Paragraph` has no internal
/// scroll); the user must backspace or clear the input to see it.
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

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(1),               // transcript
            Constraint::Length(input_height), // input bar (grows with text)
            Constraint::Length(1),            // status bar
        ])
        .split(area);

    render_transcript(frame, chunks[0], s, theme);
    render_input(frame, chunks[1], &input_text, theme);
    render_status(frame, chunks[2], s, theme);

    park_cursor_in_field(frame, chunks[1], &s.input);
    render_active_overlay(frame, area, s);
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
            s.pending_model.unwrap_or_default().display_name(),
            s.pending_mode.unwrap_or_default(),
        ),
    };
    let mut spans = vec![
        Span::styled(
            format!("  {}", mode.label()),
            Style::default()
                .fg(theme.hot_pink)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" · ", Style::default().fg(theme.muted)),
        Span::styled(model.to_string(), Style::default().fg(Color::Gray)),
    ];
    if s.streaming.is_some() {
        spans.push(Span::styled(
            " · streaming...",
            Style::default().fg(theme.muted),
        ));
    }
    frame.render_widget(Paragraph::new(Line::from(spans)), chunk);
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
            spans.push(Span::styled(
                token.to_string(),
                Style::default()
                    .fg(theme.mew_gold)
                    .add_modifier(Modifier::BOLD),
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
