use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::widgets::{Block, Paragraph, Wrap};

use mewcode_protocol::Mode;

use super::super::model::{Overlay, SessionState};
use super::overlay::{
    centered_rect, render_overlay, render_scrolled_overlay, render_slash_picker, skills_lines,
    tools_lines,
};
use super::park_cursor_in_field;
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
pub(super) fn render_session(frame: &mut Frame, area: Rect, s: &mut SessionState) {
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

    render_transcript(frame, chunks[0], s);
    render_input(frame, chunks[1], &input_text);
    render_status(frame, chunks[2], s);

    park_cursor_in_field(frame, chunks[1], &s.input);
    render_active_overlay(frame, area, s);
}

fn input_height(area: Rect, input_text: &str) -> u16 {
    let input_wrap = Paragraph::new(input_text).wrap(Wrap { trim: false });
    let input_lines = input_wrap
        .line_count(area.width.saturating_sub(2))
        .max(1)
        .min(u16::MAX as usize) as u16;
    let max_input = MAX_INPUT_HEIGHT.min(area.height.saturating_sub(2));
    input_lines.saturating_add(2).clamp(3, max_input.max(3))
}

fn render_input(frame: &mut Frame, chunk: Rect, input_text: &str) {
    let input = Paragraph::new(input_text)
        .block(Block::bordered().title(" message "))
        .wrap(Wrap { trim: false });
    frame.render_widget(input, chunk);
}

fn render_status(frame: &mut Frame, chunk: Rect, s: &SessionState) {
    let status = match (s.streaming.is_some(), &s.session) {
        (true, Some(session)) => format!(
            "{}  {:?}  •  streaming…",
            session.model.display_name(),
            session.mode
        ),
        (false, Some(session)) => format!("{}  {:?}", session.model.display_name(), session.mode),
        (true, None) => "starting session…".to_string(),
        (false, None) => format!(
            "{}  {}",
            s.pending_model.unwrap_or_default().display_name(),
            Mode::default().as_str()
        ),
    };
    frame.render_widget(
        Paragraph::new(status).style(Style::default().fg(Color::DarkGray)),
        chunk,
    );
}

fn render_active_overlay(frame: &mut Frame, area: Rect, s: &mut SessionState) {
    match s.overlay {
        Overlay::None => {}
        Overlay::Tools => render_overlay(frame, area, "Tools", tools_lines()),
        Overlay::Skills => render_overlay(frame, area, "Skills", skills_lines()),
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
                s.model_picker.scroll,
                s.model_picker.cursor,
                &mut s.model_picker.viewport,
            );
            // Track the largest viewport the view has ever reported so
            // a transient 0 (first frame, resize) doesn't make the
            // clamp think there's no room and leave the cursor
            // off-screen.
            if s.model_picker.viewport > s.model_picker.viewport_max {
                s.model_picker.viewport_max = s.model_picker.viewport;
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
                s.session_list.scroll,
                s.session_list.cursor,
                &mut s.session_list.viewport,
            );
            if s.session_list.viewport > s.session_list.viewport_max {
                s.session_list.viewport_max = s.session_list.viewport;
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
