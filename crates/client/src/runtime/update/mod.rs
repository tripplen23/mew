//! The pure, synchronous Elm-style `update` function.
//!
//! > Idiom: the Elm update loop. `update` takes `&mut App` and a [`Msg`],
//! > mutates the model in place, and returns a [`Cmd`] describing any side
//! > effect. It performs **no I/O and no `.await`** — all async work happens in
//! > the loop's `Cmd` executor, whose results come back as more `Msg`s. Because
//! > the model is never borrowed across an `.await`, the borrow checker stays
//! > quiet and the function is trivially unit-testable.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use tui_textarea::{Input, Key};

use super::model::{App, Cmd, Msg, NewSessionField, Screen, SessionState, Toast};

mod home;
mod new_session;
mod session;
mod stream;

use home::on_home_key;
use new_session::on_new_session_key;
use session::on_session_key;
use stream::apply_stream_event;

/// Apply a [`Msg`] to the model, returning the side effect to run next.
///
/// Pure and synchronous: no I/O, no awaiting. The model's `screen` and `toast`
/// fields are borrowed independently (a split borrow) so a single arm can both
/// transition the screen and raise a toast without fighting the borrow checker.
pub fn update(app: &mut App, msg: Msg) -> Cmd {
    let App {
        screen,
        toast,
        should_quit,
        ..
    } = app;

    match msg {
        Msg::Key(key) => match screen {
            Screen::Home(_) => on_home_key(screen, should_quit, key),
            Screen::NewSession(_) => on_new_session_key(screen, toast, key),
            Screen::Session(_) => on_session_key(screen, toast, key),
        },

        Msg::Tick => Cmd::None,

        Msg::SessionsLoaded(result) => {
            if let Screen::Home(h) = screen {
                match result {
                    Ok(list) => {
                        h.sessions = list;
                        h.loading = false;
                        if h.selected >= h.sessions.len() {
                            h.selected = 0;
                        }
                    }
                    Err(e) => {
                        h.sessions.clear();
                        h.selected = 0;
                        h.loading = false;
                        *toast = Some(Toast::error(e));
                    }
                }
            }
            Cmd::None
        }

        Msg::SessionCreated(result) => match result {
            Ok(session) => {
                *screen = Screen::Session(SessionState::new(session));
                Cmd::None
            }
            Err(e) => {
                if let Screen::NewSession(n) = screen {
                    n.field = NewSessionField::Title;
                }
                *toast = Some(Toast::error(e));
                Cmd::None
            }
        },

        Msg::SessionOpened(result) => match result {
            Ok(session) => {
                *screen = Screen::Session(SessionState::new(session));
                Cmd::None
            }
            Err(e) => {
                *toast = Some(Toast::error(e));
                Cmd::None
            }
        },

        Msg::Stream(ev) => {
            if let Screen::Session(s) = screen {
                if let Some(t) = apply_stream_event(s, ev) {
                    *toast = Some(t);
                }
            }
            Cmd::None
        }
    }
}

/// Translate a crossterm [`KeyEvent`] into a [`tui_textarea::Input`].
///
/// tui-textarea 0.7 still bundles crossterm 0.28, so its built-in
/// `From<KeyEvent>` impl targets that older crate. The client talks crossterm
/// 0.29 (via ratatui 0.30), so we map the event ourselves — mirroring
/// tui-textarea's own mapping. Ceiling: this must stay in sync with
/// tui-textarea's conversion; upgrade path is deleting it once tui-textarea
/// publishes a crossterm-0.29 release. Key-release events are filtered by the
/// input reader upstream (`runtime::mod`), so this fn never sees them.
pub(super) fn key_to_input(key: KeyEvent) -> Input {
    let code = match key.code {
        KeyCode::Char(c) => Key::Char(c),
        KeyCode::Backspace => Key::Backspace,
        KeyCode::Enter => Key::Enter,
        KeyCode::Left => Key::Left,
        KeyCode::Right => Key::Right,
        KeyCode::Up => Key::Up,
        KeyCode::Down => Key::Down,
        KeyCode::Tab => Key::Tab,
        KeyCode::Delete => Key::Delete,
        KeyCode::Home => Key::Home,
        KeyCode::End => Key::End,
        KeyCode::PageUp => Key::PageUp,
        KeyCode::PageDown => Key::PageDown,
        KeyCode::Esc => Key::Esc,
        KeyCode::F(x) => Key::F(x),
        _ => Key::Null,
    };
    Input {
        key: code,
        ctrl: key.modifiers.contains(KeyModifiers::CONTROL),
        alt: key.modifiers.contains(KeyModifiers::ALT),
        shift: key.modifiers.contains(KeyModifiers::SHIFT),
    }
}
