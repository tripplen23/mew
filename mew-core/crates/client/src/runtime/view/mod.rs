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
//! Rather than bridge the two `Widget` traits, the editors are rendered by reading
//! the textarea's `.lines()` and drawing them as a plain ratatui 0.30 `Paragraph`.

use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::style::Style;
use ratatui::widgets::{Block, Paragraph, Widget, Wrap};
use tui_textarea::TextArea;

use super::model::{App, Screen};

mod markdown;
mod overlay;
mod session;
mod spinner;
mod theme;
mod toast;
mod tool_card;
mod transcript;

/// Re-exported for integration tests so they can assert on the row
/// builder directly (e.g. "every model fits on one line"). The overlay
/// module itself stays private; only these two line builders are part
/// of the test surface.
pub use overlay::{model_picker_lines, session_list_lines};

pub use markdown::highlight_code_block;
pub use spinner::spinner_frame;
pub use toast::toast_alpha;
pub use tool_card::{
    render_diff, render_tool_call_header, render_tool_result_body, render_tool_result_header,
    summarise_json, truncate_one_line,
};

use session::render_session;
use theme::{COMPOSER_HORIZONTAL_PAD, COMPOSER_LEFT_PAD, theme_for};
use toast::render_toast;

const CURSOR_MARKER: &str = "\u{E000}";

/// Draw the whole application: the active screen, then any toast on top.
pub fn render(frame: &mut Frame, app: &mut App) {
    let area = frame.area();
    let theme = theme_for(app.theme);
    frame.render_widget(
        Block::default().style(Style::default().bg(theme.ink_bg)),
        area,
    );
    match &mut app.screen {
        Screen::Session(s) => render_session(frame, area, s, theme),
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
///
pub(super) fn park_cursor_in_field(frame: &mut Frame, chunk: Rect, textarea: &TextArea) {
    let (cursor_row, cursor_col) = textarea.cursor();
    let inner_width = chunk.width.saturating_sub(COMPOSER_HORIZONTAL_PAD) as usize;
    let (visual_row, visual_col) =
        visual_cursor_pos(textarea.lines(), cursor_row, cursor_col, inner_width);

    let inner_x = chunk.x.saturating_add(COMPOSER_LEFT_PAD);
    let inner_y = chunk.y;
    let max_x = chunk.x.saturating_add(chunk.width.saturating_sub(2));
    let max_y = chunk.y.saturating_add(chunk.height.saturating_sub(1));
    let x = inner_x.saturating_add(visual_col as u16).min(max_x);
    let y = inner_y.saturating_add(visual_row as u16).min(max_y);
    frame.set_cursor_position(Position::new(x, y));
}

#[doc(hidden)]
pub fn visual_cursor_pos(
    lines: &[String],
    cursor_row: usize,
    cursor_col: usize,
    width: usize,
) -> (usize, usize) {
    if width == 0 {
        return (cursor_row, cursor_col);
    }

    let width = width.min(u16::MAX as usize) as u16;
    let text = text_with_cursor_marker(lines, cursor_row, cursor_col);
    let paragraph = Paragraph::new(text.as_str()).wrap(Wrap { trim: false });
    let height = paragraph.line_count(width).max(1).min(u16::MAX as usize) as u16;
    let area = Rect::new(0, 0, width, height);
    let mut buffer = Buffer::empty(area);
    Widget::render(paragraph, area, &mut buffer);

    for y in 0..height {
        for x in 0..width {
            if buffer[(x, y)].symbol().contains(CURSOR_MARKER) {
                return (y as usize, x as usize);
            }
        }
    }

    (cursor_row, cursor_col)
}

fn text_with_cursor_marker(lines: &[String], cursor_row: usize, cursor_col: usize) -> String {
    let mut text = String::new();
    let mut inserted = false;

    for (row_idx, line) in lines.iter().enumerate() {
        if row_idx > 0 {
            text.push('\n');
        }
        for (col_idx, c) in line.chars().enumerate() {
            if row_idx == cursor_row && col_idx == cursor_col {
                text.push_str(CURSOR_MARKER);
                inserted = true;
            }
            text.push(c);
        }
        if row_idx == cursor_row && !inserted {
            text.push_str(CURSOR_MARKER);
            inserted = true;
        }
    }

    if !inserted {
        text.push_str(CURSOR_MARKER);
    }

    text
}
