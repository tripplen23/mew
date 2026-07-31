//! Overlay rendering on the session screen: dispatch by [`Overlay`] variant
//! to the shared overlay row-builders (`content`), plus cursor parking inside
//! overlays that host a text field (the connect-provider API key input).

use ratatui::Frame;
use ratatui::layout::{Position, Rect};
use unicode_width::UnicodeWidthStr;

use crate::runtime::model::{ConnectStep, Overlay, SessionState};
use crate::runtime::view::panel::{centered_rect, render_panel, render_scrolled_panel};
use crate::runtime::view::session::overlay::content::{
    choice_lines, connect_provider_key_text, connect_provider_lines, file_picker_lines,
    rename_session_lines, render_slash_picker, skills_lines, theme_lines, tools_lines,
};

mod content;

pub use content::{model_picker_lines, session_list_lines};

/// True while an overlay hosts a text field (the connect-provider API key
/// input), which changes how the composer is rendered and where the cursor
/// parks.
pub(super) fn active_overlay_text_input(s: &SessionState) -> bool {
    matches!(
        s.overlay,
        Overlay::ConnectProvider if s.connect_provider.step == ConnectStep::EnterKey
    )
}

pub(super) fn render_active_overlay(frame: &mut Frame, area: Rect, s: &mut SessionState) {
    match s.overlay {
        Overlay::None => {}
        Overlay::Tools => {
            let mode = s.session.as_ref().map(|sess| sess.mode).unwrap_or_default();
            render_panel(frame, area, "Tools", tools_lines(mode))
        }
        Overlay::Skills => render_panel(frame, area, "Skills", skills_lines(s)),
        Overlay::Theme => render_panel(frame, area, "Theme", theme_lines()),
        Overlay::Choice => render_panel(frame, area, "Choose", choice_lines(s)),
        Overlay::ConnectProvider => {
            let body = connect_provider_lines(s);
            let title = match s.connect_provider.step {
                ConnectStep::PickProvider => "Connect Provider",
                ConnectStep::EnterKey => "Enter API Key",
                ConnectStep::Validating => "Validating...",
                ConnectStep::Done => "Connected!",
            };
            render_panel(frame, area, title, body);
            if active_overlay_text_input(s) {
                park_cursor_in_overlay_text_input(frame, area, s);
            }
        }
        Overlay::ModelPicker => {
            // Rect first, so the row builder can truncate each model to one
            // line — otherwise a wrapped name desyncs cursor and highlight.
            let rect = centered_rect(area, 60, 60);
            let inner_w = rect.width.saturating_sub(2) as usize;
            let body = model_picker_lines(s, inner_w);
            render_scrolled_panel(
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
            // Largest viewport ever seen: a transient 0 (first frame, resize)
            // would make the clamp think there's no room, leaving the cursor
            // off-screen.
            if s.model_picker.picker.viewport > s.model_picker.picker.viewport_max {
                s.model_picker.picker.viewport_max = s.model_picker.picker.viewport;
            }
        }
        Overlay::SessionList => {
            let rect = centered_rect(area, 60, 60);
            let inner_w = rect.width.saturating_sub(2) as usize;
            let body = session_list_lines(s, inner_w);
            render_scrolled_panel(
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
            let body = file_picker_lines(s, inner_w);
            render_scrolled_panel(
                frame,
                area,
                "Files",
                "Enter insert, Esc close",
                body,
                s.filtered_files().len(),
                s.file_picker.picker.scroll,
                s.file_picker.picker.cursor,
                &mut s.file_picker.picker.viewport,
            );
            if s.file_picker.picker.viewport > s.file_picker.picker.viewport_max {
                s.file_picker.picker.viewport_max = s.file_picker.picker.viewport;
            }
        }
        Overlay::RenameSession => {
            render_panel(frame, area, "Rename session", rename_session_lines(s))
        }
        Overlay::SlashPicker => render_slash_picker(frame, area, s),
    }
}

fn park_cursor_in_overlay_text_input(frame: &mut Frame, area: Rect, s: &SessionState) {
    let rect = centered_rect(area, 60, 60);
    let inner_width = rect.width.saturating_sub(2) as usize;
    if inner_width == 0 {
        return;
    }

    let (left_pad, top_pad) = (2, 4);
    let text = connect_provider_key_text(s);
    let offset = left_pad + UnicodeWidthStr::width(text.as_str());
    let x = rect.x + 1 + (offset % inner_width) as u16;
    let y = rect.y + 1 + top_pad + (offset / inner_width) as u16;
    frame.set_cursor_position(Position::new(
        x.min(rect.x + rect.width.saturating_sub(2)),
        y.min(rect.y + rect.height.saturating_sub(2)),
    ));
}
