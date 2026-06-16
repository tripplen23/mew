//! Per-screen rendering for the TUI.
//!
//! `view` is a pure function of the model: [`render`] draws the current
//! [`App`] into a ratatui [`Frame`] and never mutates state. Animations
//! (spinner, toast fade) are derived from each `started_at` instant, so a
//! redraw on every 50 ms tick advances them with no model bookkeeping.
//!
//! > Note on dependency versions: `tui-textarea` 0.7 still renders against
//! > ratatui 0.29, but the client draws with ratatui 0.30. Rather than bridge
//! > the two `Widget` traits, the editors are rendered by reading their
//! > `.lines()` and drawing a plain `Paragraph` in ratatui 0.30.

use ratatui::Frame;

use super::model::{App, Screen};

mod home;
mod markdown;
mod new_session;
mod overlay;
mod session;
mod spinner;
mod toast;

pub use markdown::highlight_code_block;
pub use spinner::spinner_frame;
pub use toast::toast_alpha;

use home::render_home;
use new_session::render_new_session;
use session::render_session;
use toast::render_toast;

/// Draw the whole application: the active screen, then any toast on top.
pub fn render(frame: &mut Frame, app: &App) {
    let area = frame.area();
    match &app.screen {
        Screen::Home(h) => render_home(frame, area, h),
        Screen::NewSession(n) => render_new_session(frame, area, n),
        Screen::Session(s) => render_session(frame, area, s),
    }

    if let Some(toast) = &app.toast {
        render_toast(frame, area, toast);
    }
}
