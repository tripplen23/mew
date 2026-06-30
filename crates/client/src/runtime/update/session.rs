use crossterm::event::{KeyCode, KeyEvent};
use tui_textarea::TextArea;
use uuid::Uuid;

use mewcode_protocol::event::ChatRequest;
use mewcode_protocol::{Message, MessagePart, Mode};

use crate::net::CreateSessionRequest;

use super::super::model::{Cmd, Overlay, SessionState, StreamingState, Toast};
use super::key_to_input;

/// Session screen: input editing, submit, slash commands.
pub(super) fn on_session_key(
    s: &mut SessionState,
    app_quit: &mut bool,
    toast: &mut Option<Toast>,
    key: KeyEvent,
) -> Cmd {
    if key.code == KeyCode::Esc {
        // Close an open overlay first; once everything's closed, Esc is a
        // no-op (the chat has nowhere to go back to without a session list).
        if s.overlay != Overlay::None {
            s.overlay = Overlay::None;
        }
        return Cmd::None;
    }

    if key.code == KeyCode::Char('q') && s.overlay == Overlay::None && !s.creating {
        *app_quit = true;
        return Cmd::None;
    }

    if s.creating {
        // A `POST /sessions` is in flight for `pending_chat`; ignore
        // everything else so the user can't double-submit.
        return Cmd::None;
    }

    match key.code {
        KeyCode::Enter => on_session_submit(s, toast),
        // Transcript scrollback. Up/PageUp release auto-follow; scrolling back
        // to the bottom re-engages it. `max_scroll`/`viewport` come from the
        // last rendered frame (see `view::render_session`).
        KeyCode::Up => {
            scroll_by(s, -1);
            Cmd::None
        }
        KeyCode::Down => {
            scroll_by(s, 1);
            Cmd::None
        }
        KeyCode::PageUp => {
            scroll_by(s, -(s.viewport.max(1) as i32));
            Cmd::None
        }
        KeyCode::PageDown => {
            scroll_by(s, s.viewport.max(1) as i32);
            Cmd::None
        }
        _ => {
            s.input.input(key_to_input(key));
            Cmd::None
        }
    }
}

/// Move the transcript scroll offset by `delta` wrapped lines, clamping into
/// `[0, max_scroll]`. Scrolling up releases auto-follow; reaching the bottom
/// re-engages it so new replies keep scrolling into view.
fn scroll_by(s: &mut SessionState, delta: i32) {
    let next = (s.scroll as i32 + delta).clamp(0, s.max_scroll as i32) as u16;
    s.scroll = next;
    s.follow = next >= s.max_scroll;
}

/// Handle `Enter` in the Session input bar: slash command, or — if no
/// session exists yet — create one with the typed text as the seed, or
/// send the chat into the existing session.
pub(super) fn on_session_submit(s: &mut SessionState, toast: &mut Option<Toast>) -> Cmd {
    let text = s.input.lines().join("\n");
    let trimmed = text.trim();

    if trimmed.is_empty() {
        return Cmd::None;
    }

    // one turn at a time — refuse to start another while a turn is
    // in flight, leaving the input intact for the user to retry.
    if s.streaming.is_some() {
        return Cmd::None;
    }

    if let Some(rest) = trimmed.strip_prefix('/') {
        s.input = TextArea::default();
        match rest.split_whitespace().next().unwrap_or("") {
            "tools" => s.overlay = Overlay::Tools,
            "skills" => s.overlay = Overlay::Skills,
            other => {
                *toast = Some(Toast::error(format!("unknown command: /{other}")));
            }
        }
        return Cmd::None;
    }

    let user_text = trimmed.to_string();
    let user_msg = Message::user(vec![MessagePart::Text {
        text: user_text.clone(),
    }]);

    if let Some(session) = s.session.as_mut() {
        session.messages.push(user_msg);
        // Snap back to the latest line so the user watches the reply stream in.
        s.follow = true;
        // `Uuid::nil()` here is intentional: the real id arrives with the SSE
        // `Started` event; we need a placeholder so the `StreamingState` is Some.
        s.streaming = Some(StreamingState::new(Uuid::nil()));
        // Clear the composer now that the message is committed to history.
        s.input = TextArea::default();
        Cmd::StartChat(ChatRequest {
            session_id: session.id,
            model: session.model,
            mode: session.mode,
            messages: session.messages.clone(),
        })
    } else {
        // No session yet — buffer the text in the composer too so the user
        // can retry on a create failure. The `Msg::SessionCreated` handler
        // will clear it once the message is committed as the first turn.
        s.pending_chat = Some(user_text.clone());
        s.creating = true;
        s.creation_started_at = Some(std::time::Instant::now());
        Cmd::CreateSession(CreateSessionRequest {
            title: derive_title(&user_text),
            model: None,
            mode: Some(Mode::default()),
        })
    }
}

/// Cap the auto-generated session title at a sane length and collapse
/// newlines so a multiline first message still produces a single-line
/// title. Used only when there is no session yet.
fn derive_title(text: &str) -> String {
    const MAX_TITLE_LEN: usize = 60;
    let first_line = text.lines().next().unwrap_or(text);
    let collapsed: String = first_line.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.chars().count() <= MAX_TITLE_LEN {
        collapsed
    } else {
        collapsed
            .chars()
            .take(MAX_TITLE_LEN)
            .collect::<String>()
            .trim_end()
            .to_string()
    }
}
