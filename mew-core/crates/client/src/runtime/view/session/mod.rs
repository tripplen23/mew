//! Session screen rendering: splits the frame into transcript, composer
//! header, composer bar, and status bar, then renders each. The composer,
//! status, and overlay submodules own their respective sections.

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};

use crate::runtime::model::SessionState;
use crate::runtime::view::park_cursor_in_field;
use crate::runtime::view::session::composer::{
    composer_height, queue_display_height, render_composer, render_message_queue,
};
use crate::runtime::view::session::overlay::{active_overlay_text_input, render_active_overlay};
use crate::runtime::view::session::status::render_status;
use crate::runtime::view::session::transcript::render_transcript;
use crate::runtime::view::theme::Theme;

mod composer;
pub(super) mod overlay;
mod status;
pub(super) mod transcript;

/// Shared with the transcript for consistent `@`-mentions.
pub(super) use composer::render_mentions;

/// Session: scrollable transcript, composer bar, status bar, plus overlays.
/// The composer bar grows with its text; before the first message the
/// transcript shows a "type to start" hint.
pub(super) fn render_session(frame: &mut Frame, area: Rect, s: &mut SessionState, theme: Theme) {
    let composer_text = if active_overlay_text_input(s) {
        String::new()
    } else {
        s.composer_text()
    };
    let composer_height = composer_height(area, &composer_text);
    let queue_height = queue_display_height(s);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(1),                  // transcript
            Constraint::Length(queue_height),    // queue list, at least the dashed header row
            Constraint::Length(composer_height), // composer bar (grows with text)
            Constraint::Length(1),               // status bar
        ])
        .split(area);

    render_transcript(frame, chunks[0], s, theme);
    render_message_queue(frame, chunks[1], s, theme);
    render_composer(frame, chunks[2], &composer_text, theme);
    render_status(frame, chunks[3], s, theme);

    if !active_overlay_text_input(s) {
        park_cursor_in_field(frame, chunks[2], &s.composer);
    }
    render_active_overlay(frame, area, s);
}
