//! Per-screen rendering: turns the model into pixels on the terminal.
//!
//! Given a model, the view paints a single frame and returns. It is a pure
//! function of the model with one exception: the session renderer writes
//! `scroll`/`max_scroll`/`viewport` back during the draw, because the wrapped
//! line count is only known once [ratatui](https://docs.rs/ratatui/latest/ratatui/)
//! has actually wrapped the text.
//!
//! Animations (spinner, toast fade) work the same way: each one stores only
//! the `started_at` instant, and the view derives the current frame from it
//! on every redraw. The 50 ms tick task pushes a redraw; nothing on the model
//! has to be written per frame.
//!
//! [`tui-textarea`](https://docs.rs/tui-textarea/latest/tui_textarea/) 0.7
//! still renders against ratatui 0.29, but the client draws with ratatui 0.30.
//! Rather than bridge the two `Widget` traits, the editors are rendered by
//! reading the textarea's `.lines()` and drawing them as a plain ratatui 0.30
//! `Paragraph`.

use ratatui::Frame;
use ratatui::layout::{Position, Rect};
use tui_textarea::TextArea;

use super::model::{App, Screen};

mod markdown;
mod overlay;
mod session;
mod spinner;
mod toast;
mod tool_card;

pub use markdown::highlight_code_block;
pub use spinner::spinner_frame;
pub use toast::toast_alpha;
pub use tool_card::{
    render_tool_call_header, render_tool_result_body, render_tool_result_header, summarise_json,
    truncate_one_line,
};

use session::render_session;
use toast::render_toast;

/// Draw the whole application: the active screen, then any toast on top.
pub fn render(frame: &mut Frame, app: &mut App) {
    let area = frame.area();
    match &mut app.screen {
        Screen::Session(s) => render_session(frame, area, s),
    }

    if let Some(toast) = &app.toast {
        render_toast(frame, area, toast);
    }
}

/// Park the terminal cursor inside the bordered box that hosts `textarea`.
///
/// Needed because the TextAreas render as plain `Paragraph`s and so don't move
/// the cursor themselves; without this the cursor
/// stays at the end of the last write — the status bar — and the user's
/// keystrokes appear to land in the wrong place.
pub(super) fn park_cursor_in_field(frame: &mut Frame, chunk: Rect, textarea: &TextArea) {
    let (cursor_row, cursor_col) = textarea.cursor();
    let inner_x = chunk.x.saturating_add(1);
    let inner_y = chunk.y.saturating_add(1);
    let max_x = chunk.x.saturating_add(chunk.width.saturating_sub(2));
    let max_y = chunk.y.saturating_add(chunk.height.saturating_sub(2));
    let x = inner_x.saturating_add(cursor_col as u16).min(max_x);
    let y = inner_y.saturating_add(cursor_row as u16).min(max_y);
    frame.set_cursor_position(Position::new(x, y));
}
