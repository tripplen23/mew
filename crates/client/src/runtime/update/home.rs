use crossterm::event::{KeyCode, KeyEvent};

use super::super::model::{Cmd, NewSessionState, Screen};

/// Home screen: list navigation and the transitions out of it.
///
/// `Up`/`Down` clamp at the ends without wrapping; `Enter` on an empty list is a no-op.
pub(super) fn on_home_key(screen: &mut Screen, should_quit: &mut bool, key: KeyEvent) -> Cmd {
    match key.code {
        KeyCode::Char('q') => {
            *should_quit = true;
            Cmd::None
        }
        KeyCode::Char('n') => {
            *screen = Screen::NewSession(NewSessionState::default());
            Cmd::None
        }
        KeyCode::Up => {
            if let Screen::Home(h) = screen {
                if !h.sessions.is_empty() {
                    h.selected = h.selected.saturating_sub(1);
                }
            }
            Cmd::None
        }
        KeyCode::Down => {
            if let Screen::Home(h) = screen {
                if !h.sessions.is_empty() {
                    h.selected = (h.selected + 1).min(h.sessions.len() - 1);
                }
            }
            Cmd::None
        }
        KeyCode::Enter => {
            if let Screen::Home(h) = screen {
                if let Some(summary) = h.sessions.get(h.selected) {
                    return Cmd::OpenSession(summary.id);
                }
            }
            Cmd::None
        }
        _ => Cmd::None,
    }
}
