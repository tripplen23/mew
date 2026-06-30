//! The pure, synchronous Elm-style `update` function.
//!
//! > Idiom: the Elm update loop. `update` takes `&mut App` and a [`Msg`],
//! > mutates the model in place, and returns a [`Cmd`] describing any side
//! > effect. It performs **no I/O and no `.await`** — all async work happens in
//! > the loop's `Cmd` executor, whose results come back as more `Msg`s. Because
//! > the model is never borrowed across an `.await`, the borrow checker stays
//! > quiet and the function is trivially unit-testable.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use tui_textarea::{Input, Key, TextArea};
use uuid::Uuid;

use mewcode_protocol::event::ChatRequest;
use mewcode_protocol::{Message, MessagePart};

use super::model::{App, Cmd, CreateError, Msg, Screen, StreamingState, Toast};

mod session;
mod stream;

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
    let Screen::Session(s) = screen;

    match msg {
        Msg::Key(key) => on_session_key(s, should_quit, toast, key),

        Msg::Tick => Cmd::None,

        Msg::SessionCreated(result) => match result {
            Ok(session) => {
                // Adopt the new session. If the user already typed a
                // message -> commit it as the first turn.
                let pending = s.pending_chat.take();
                s.session = Some(session.clone());
                s.creating = false;
                s.creation_started_at = None;
                if let Some(text) = pending {
                    let user_msg = Message::user(vec![MessagePart::Text { text: text.clone() }]);
                    s.session.as_mut().unwrap().messages.push(user_msg);
                    s.follow = true;
                    s.streaming = Some(StreamingState::new(Uuid::nil()));
                    // The composer is cleared now that the first turn
                    // has been committed.
                    s.input = TextArea::default();
                    // The local `session` is the pre-push server clone —
                    // read from the model, which has the user message.
                    let live = s.session.as_ref().unwrap();
                    return Cmd::StartChat(ChatRequest {
                        session_id: live.id,
                        model: live.model,
                        mode: live.mode,
                        messages: live.messages.clone(),
                    });
                }
                Cmd::None
            }
            // Create failed; surface the error. The composer keeps the
            // typed text so the user can retry; `pending_chat` is dropped
            // so a retry rebuilds it from the still-present input.
            Err(CreateError::Other(message)) => {
                s.creating = false;
                s.creation_started_at = None;
                s.pending_chat = None;
                *toast = Some(Toast::error(message));
                Cmd::None
            }
            Err(CreateError::EmptyTitle(_)) => {
                // The title comes from the first message, so this arm
                // is unreachable. Treat as a generic failure if it ever fires.
                s.creating = false;
                s.creation_started_at = None;
                s.pending_chat = None;
                *toast = Some(Toast::error("could not create session: empty title"));
                Cmd::None
            }
        },

        Msg::Stream(ev) => {
            if let Some(t) = apply_stream_event(s, ev) {
                *toast = Some(t);
            }
            Cmd::None
        }
    }
}

/// Translate a crossterm [`KeyEvent`] into a [`tui_textarea::Input`].
///
/// [`tui-textarea`](https://docs.rs/tui-textarea/latest/tui_textarea/) 0.7
/// still bundles crossterm 0.28, so its built-in `From<KeyEvent>` impl
/// targets that older crate. The client talks crossterm 0.29 (via
/// [ratatui](https://docs.rs/ratatui/latest/ratatui/) 0.30), so we map the
/// event ourselves — mirroring tui-textarea's own mapping. Ceiling: this
/// must stay in sync with tui-textarea's conversion; upgrade path is deleting
/// it once tui-textarea publishes a crossterm-0.29 release. Key-release
/// and key-repeat events are filtered by the input reader upstream
/// (`runtime::mod`), so this fn never sees them.
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
