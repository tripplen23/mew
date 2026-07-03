use crossterm::event::{KeyCode, KeyEvent};
use tui_textarea::TextArea;
use uuid::Uuid;

use mewcode_protocol::event::ChatRequest;
use mewcode_protocol::{Message, MessagePart, Mode, ModelId};

use crate::net::{CreateSessionRequest, SessionPatch};

use super::super::model::{Cmd, Overlay, QUIT_COMMAND, SessionState, StreamingState, Toast};
use super::key_to_input;

/// Session screen: input editing, submit, slash commands.
pub(super) fn on_session_key(
    s: &mut SessionState,
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

    if s.creating {
        // A `POST /sessions` is in flight for `pending_chat`; ignore
        // everything else so the user can't double-submit.
        return Cmd::None;
    }

    // Overlay-specific key handling: Up/Down move the cursor, Enter applies
    // the highlighted row, `d` deletes (session list only). These keys
    // never reach the input bar while an overlay is open.
    match s.overlay {
        Overlay::ModelPicker => match key.code {
            KeyCode::Up => {
                cursor_move(s, -1);
                return Cmd::None;
            }
            KeyCode::Down => {
                cursor_move(s, 1);
                return Cmd::None;
            }
            KeyCode::Enter => {
                if s.session.is_none() {
                    *toast = Some(Toast::error(
                        "/model: start a session before picking a model",
                    ));
                    return Cmd::None;
                }
                if let (Some(session), Some(entries)) = (s.session.as_ref(), s.models.as_ref()) {
                    if let Some(entry) = entries.get(s.model_cursor) {
                        if let Ok(model) = entry.id.parse::<ModelId>() {
                            return Cmd::PatchSession {
                                id: session.id,
                                patch: SessionPatch {
                                    model: Some(model),
                                    ..Default::default()
                                },
                            };
                        }
                    }
                }
                return Cmd::None;
            }
            _ => return Cmd::None,
        },
        Overlay::SessionList => match key.code {
            KeyCode::Up => {
                cursor_move(s, -1);
                return Cmd::None;
            }
            KeyCode::Down => {
                cursor_move(s, 1);
                return Cmd::None;
            }
            KeyCode::Enter => {
                if let Some(summary) = s.session_summaries.get(s.session_cursor) {
                    return Cmd::OpenSession(summary.id);
                }
                return Cmd::None;
            }
            KeyCode::Char('d') | KeyCode::Char('D') => {
                if let Some(summary) = s.session_summaries.get(s.session_cursor) {
                    return Cmd::DeleteSession(summary.id);
                }
                return Cmd::None;
            }
            _ => return Cmd::None,
        },
        Overlay::RenameSession => {
            if key.code == KeyCode::Enter {
                if let Some(session) = s.session.as_ref() {
                    let title = s.input.lines().join("\n").trim().to_string();
                    if title.is_empty() {
                        *toast = Some(Toast::error("title cannot be empty"));
                    } else {
                        s.overlay = Overlay::None;
                        return Cmd::PatchSession {
                            id: session.id,
                            patch: SessionPatch {
                                title: Some(title),
                                ..Default::default()
                            },
                        };
                    }
                }
                return Cmd::None;
            }
            // Any other key falls through to the input bar so the user can
            // type the new title.
        }
        Overlay::None | Overlay::Tools | Overlay::Skills => {}
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

/// Move the highlight cursor of an open picker overlay by `delta` rows,
/// clamping into `[0, len - 1]`. A no-op when no overlay is open or the
/// list is empty.
fn cursor_move(s: &mut SessionState, delta: i32) {
    let (len, cursor): (usize, &mut usize) = match s.overlay {
        Overlay::ModelPicker => match s.models.as_ref() {
            Some(m) if !m.is_empty() => (m.len(), &mut s.model_cursor),
            _ => return,
        },
        Overlay::SessionList => {
            if s.session_summaries.is_empty() {
                return;
            }
            (s.session_summaries.len(), &mut s.session_cursor)
        }
        _ => return,
    };
    let max = (len - 1) as i32;
    let next = (*cursor as i32 + delta).clamp(0, max) as usize;
    *cursor = next;
}

/// Move the transcript scroll offset by `delta` wrapped lines, clamping into
/// `[0, max_scroll]`. Scrolling up releases auto-follow; reaching the bottom
/// re-engages it so new replies keep scrolling into view.
fn scroll_by(s: &mut SessionState, delta: i32) {
    let next = (s.scroll as i32 + delta).clamp(0, s.max_scroll as i32) as u16;
    s.scroll = next;
    s.follow = next >= s.max_scroll;
}

/// Handle `Enter` in the Session input bar: the `quit` text command,
/// slash commands, or — if no session exists yet — create one with the
/// typed text as the seed, or send the chat into the existing session.
pub(super) fn on_session_submit(s: &mut SessionState, toast: &mut Option<Toast>) -> Cmd {
    let text = s.input.lines().join("\n");
    let trimmed = text.trim();

    if trimmed.is_empty() {
        return Cmd::None;
    }

    // Text-based quit.
    if trimmed.eq_ignore_ascii_case(QUIT_COMMAND) {
        s.input = TextArea::default();
        return Cmd::Quit;
    }

    // one turn at a time — refuse to start another while a turn is
    // in flight, leaving the input intact for the user to retry.
    if s.streaming.is_some() {
        return Cmd::None;
    }

    if let Some(rest) = trimmed.strip_prefix('/') {
        s.input = TextArea::default();
        let mut parts = rest.split_whitespace();
        let cmd = parts.next().unwrap_or("");
        let args: Vec<&str> = parts.collect();
        return match cmd {
            "tools" => {
                s.overlay = Overlay::Tools;
                Cmd::None
            }
            "skills" => {
                s.overlay = Overlay::Skills;
                Cmd::None
            }
            "model" => on_model_command(s),
            "session" => on_session_command(s, &args, toast),
            other => {
                *toast = Some(Toast::error(format!("unknown command: /{other}")));
                Cmd::None
            }
        };
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

/// Handle `/model`: open the picker overlay, fetching the registry on
/// demand. Picking a row (Enter in the overlay) is handled in
/// `on_session_key`; this function only opens the dialog.
fn on_model_command(s: &mut SessionState) -> Cmd {
    s.overlay = Overlay::ModelPicker;
    s.model_cursor = 0;
    if s.models.is_none() {
        Cmd::FetchModels
    } else {
        Cmd::None
    }
}

/// Handle `/session`: open the list overlay (default) or start a rename
/// (`/session rename`). Switching and deleting rows are handled in
/// `on_session_key`. Always fetches the list — the empty cache is
/// indistinguishable from "never fetched", and a fresh `/session` open
/// should reflect any sessions created since the last view.
fn on_session_command(s: &mut SessionState, args: &[&str], toast: &mut Option<Toast>) -> Cmd {
    match args.first().copied() {
        Some("rename") => {
            let Some(session) = s.session.as_ref() else {
                *toast = Some(Toast::error("/session rename needs an active session"));
                return Cmd::None;
            };
            // Seed the input bar with the current title so the user can edit
            // it in place. Enter in `Overlay::RenameSession` reads the new
            // title from `s.input`.
            s.input = TextArea::new(vec![session.title.clone()]);
            s.overlay = Overlay::RenameSession;
            Cmd::None
        }
        _ => {
            s.overlay = Overlay::SessionList;
            s.session_cursor = 0;
            Cmd::FetchSessions
        }
    }
}
