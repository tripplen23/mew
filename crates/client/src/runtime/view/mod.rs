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
use unicode_width::UnicodeWidthChar;

use super::model::{App, Screen};

mod markdown;
mod overlay;
mod session;
mod spinner;
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
///
/// `TextArea::cursor()` returns the cursor position in the **text** (row, col
/// in unwrapped lines). Since we render the field with `Wrap { trim: false }`,
/// a long line spans multiple visual rows. We walk the text once to find the
/// visual row/col the cursor lands on, so the blink lands where the next
/// character will actually appear — not at the end of the first wrapped
/// row.
pub(super) fn park_cursor_in_field(frame: &mut Frame, chunk: Rect, textarea: &TextArea) {
    let (cursor_row, cursor_col) = textarea.cursor();
    let text = textarea.lines().join("\n");
    let inner_width = chunk.width.saturating_sub(2) as usize;
    let (visual_row, visual_col) = visual_cursor_pos(&text, cursor_row, cursor_col, inner_width);

    let inner_x = chunk.x.saturating_add(1);
    let inner_y = chunk.y.saturating_add(1);
    let max_x = chunk.x.saturating_add(chunk.width.saturating_sub(2));
    let max_y = chunk.y.saturating_add(chunk.height.saturating_sub(2));
    let x = inner_x.saturating_add(visual_col as u16).min(max_x);
    let y = inner_y.saturating_add(visual_row as u16).min(max_y);
    frame.set_cursor_position(Position::new(x, y));
}

/// Map a (row, col) text cursor into a (row, col) visual cursor at the given
/// wrap width. Each `\n` advances the text row; a character whose display
/// width would overflow `width` advances the visual row and resets the visual
/// column. CJK / combining marks use the Unicode width from
/// [`unicode_width`] so a wide glyph never lands half-on, half-off a row.
///
/// **Note:** this is a *character*-width wrap, not the word-wrap performed
/// by `Paragraph` for the input box. For typical chat text (short words,
/// spaces between them) the two algorithms agree. They can diverge when a
/// word wrap would put a space-then-word on a fresh row but the char-wrap
/// would not — pin this in `cursor_after_word_wrap_lands_on_new_visual_row`.
fn visual_cursor_pos(
    text: &str,
    cursor_row: usize,
    cursor_col: usize,
    width: usize,
) -> (usize, usize) {
    if width == 0 {
        return (cursor_row, cursor_col);
    }

    let mut visual_row = 0usize;
    let mut visual_col = 0usize;
    let mut text_row = 0usize;
    let mut text_col = 0usize;

    for c in text.chars() {
        if text_row == cursor_row && text_col == cursor_col {
            return (visual_row, visual_col);
        }
        if c == '\n' {
            text_row += 1;
            text_col = 0;
            visual_row += 1;
            visual_col = 0;
        } else {
            let w = c.width().unwrap_or(1);
            if visual_col + w > width {
                visual_row += 1;
                visual_col = 0;
            }
            visual_col += w;
            text_col += 1;
        }
    }

    // Cursor at or past the end of the text.
    (visual_row, visual_col)
}
