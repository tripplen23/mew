//! Update arm for [`Screen::Canvas`](super::super::model::Screen::Canvas).
//!
//! T4 ships only the minimum viable key handling: `Esc` to leave
//! the canvas (return to Home, refetch the session list). T5
//! (navigation) will add mouse hit-testing, arrow-key selection,
//! and viewport pan/zoom.

use crossterm::event::{KeyCode, KeyEvent};

use super::super::model::{CanvasData, CanvasState, Cmd, HomeState, Screen, Toast};

/// Handle a key event when the user is on the Canvas screen.
///
/// `Esc` returns the user to Home. The screen transition is
/// performed in place — same pattern as `on_home_key` switching
/// into `Screen::NewSession` — so the user is never stuck on the
/// canvas. The return path also refetches the session list,
/// matching the initial Home behaviour on app startup.
pub(super) fn on_canvas_key(screen: &mut Screen, key: KeyEvent) -> Cmd {
    match key.code {
        KeyCode::Esc => {
            *screen = Screen::Home(HomeState::loading());
            Cmd::LoadSessions
        }
        _ => Cmd::None,
    }
}

/// Apply a finished `Msg::CanvasLoaded` to the model. Populates
/// the graph + layout on success, surfaces a toast on failure,
/// and clears the loading flag in either case.
pub(super) fn apply_canvas_loaded(
    c: &mut CanvasState,
    toast: &mut Option<Toast>,
    result: Result<CanvasData, String>,
) {
    c.loading = false;
    match result {
        Ok(data) => {
            c.graph = data.graph;
            c.layout = data.layout;
            c.selected = None;
        }
        Err(e) => {
            *toast = Some(Toast::error(format!("canvas load failed: {e}")));
        }
    }
}
