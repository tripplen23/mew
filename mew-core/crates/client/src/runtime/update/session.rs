use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use tui_textarea::TextArea;
use uuid::Uuid;

use mewcode_protocol::event::ChatRequest;
use mewcode_protocol::{Message, MessagePart, Mode};

use crate::net::{CreateSessionRequest, SessionPatch};

use super::super::model::{
    Cmd, Overlay, PastedText, QUIT_COMMAND, SessionState, StreamingState, Toast,
};
use super::key_to_input;
use super::picker::{
    on_file_picker_key, on_model_picker_key, on_session_list_key, open_file_picker,
    refresh_file_picker,
};
use super::slash::{
    SlashPickerResult, on_slash_picker_key, open_slash_picker, slash_default_cursor,
};

const COMPACT_PASTE_CHARS: usize = 120;

/// Session screen: input editing, submit, slash commands.
pub(super) fn on_session_key(
    s: &mut SessionState,
    toast: &mut Option<Toast>,
    key: KeyEvent,
) -> Cmd {
    if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
        return Cmd::Quit;
    }

    if key.code == KeyCode::Esc {
        // Close an open overlay first
        if s.overlay != Overlay::None {
            // `Overlay::RenameSession` seeds `s.input` with the current
            // session title so the user can edit it in place.
            let was_rename = s.overlay == Overlay::RenameSession;
            let was_slash = s.overlay == Overlay::SlashPicker;
            s.overlay = Overlay::None;
            if was_rename {
                s.input = TextArea::default();
                s.pasted.clear();
            }
            if was_slash {
                // The picker only opens when the composer starts with `/`,
                s.input = TextArea::default();
                s.pasted.clear();
            }
        }
        return Cmd::None;
    }

    if s.creating {
        // A `POST /sessions` is in flight for `pending_chat`
        return Cmd::None;
    }

    // Overlay-specific key handling: Up/Down move the cursor, Enter applies
    // the highlighted row, `d` deletes (session list only).
    match s.overlay {
        Overlay::SlashPicker => match on_slash_picker_key(s, key) {
            SlashPickerResult::Cmd(cmd) => return cmd,
            SlashPickerResult::Submit => return on_session_submit(s, toast),
        },
        Overlay::ModelPicker => return on_model_picker_key(s, key),
        Overlay::FilePicker => return on_file_picker_key(s, key),
        Overlay::SessionList => return on_session_list_key(s, key),
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
                            from_rename: true,
                        };
                    }
                }
                return Cmd::None;
            }
            // Any other key falls through to the input bar so the user can
            // type the new title.
        }
        Overlay::None | Overlay::Tools | Overlay::Skills | Overlay::Theme => {}
    }

    match key.code {
        KeyCode::Tab => switch_mode(s, None),

        KeyCode::Char('@') => {
            s.input.input(key_to_input(key));
            open_file_picker(s)
        }

        KeyCode::Char('/') => {
            s.input.input(key_to_input(key));
            if slash_default_cursor(&s.input.lines().join("\n")).is_some() {
                open_slash_picker(s);
            }
            Cmd::None
        }

        KeyCode::Enter => on_session_submit(s, toast),

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
            if s.overlay == Overlay::None {
                return refresh_file_picker(s);
            }
            Cmd::None
        }
    }
}

pub(super) fn on_session_paste(s: &mut SessionState, text: String) -> Cmd {
    if s.creating {
        return Cmd::None;
    }

    let char_count = text.chars().count();
    let line_count = text.lines().count().max(1);
    if line_count == 1 && char_count <= COMPACT_PASTE_CHARS {
        s.input.insert_str(text);
        return Cmd::None;
    }

    let marker = if line_count > 1 {
        format!("[Pasted ~{line_count} lines]")
    } else {
        format!("[Pasted ~{char_count} chars]")
    };
    s.input.insert_str(&marker);
    s.pasted.push(PastedText { marker, text });
    Cmd::None
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
    let visible_text = s.input.lines().join("\n");
    let visible_trimmed = visible_text.trim();

    if visible_trimmed.is_empty() {
        return Cmd::None;
    }

    // Text-based quit.
    if visible_trimmed.eq_ignore_ascii_case(QUIT_COMMAND) {
        s.input = TextArea::default();
        s.pasted.clear();
        return Cmd::Quit;
    }

    // one turn at a time — refuse to start another while a turn is
    // in flight, leaving the input intact for the user to retry.
    if s.streaming.is_some() {
        return Cmd::None;
    }

    if let Some(rest) = visible_trimmed.strip_prefix('/') {
        s.input = TextArea::default();
        s.pasted.clear();
        let mut parts = rest.split_whitespace();
        let cmd = parts.next().unwrap_or("");
        let args: Vec<&str> = parts.collect();
        return match cmd {
            "tools" => {
                s.overlay = Overlay::Tools;
                Cmd::None
            }
            "skills" => on_skills_command(s),
            "theme" => {
                s.overlay = Overlay::Theme;
                Cmd::None
            }
            "mode" => on_mode_command(s, &args, toast),
            "model" => on_model_command(s),
            "session" => on_session_command(s, &args, toast),
            other => {
                *toast = Some(Toast::error(format!("unknown command: /{other}")));
                Cmd::None
            }
        };
    }

    let text = expand_pastes(s, &visible_text);
    let trimmed = text.trim();
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
        s.pasted.clear();
        Cmd::StartChat(ChatRequest {
            session_id: session.id,
            model: session.model,
            provider: None,
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
            model: s.pending_model,
            mode: Some(s.pending_mode.unwrap_or_default()),
        })
    }
}

fn switch_mode(s: &mut SessionState, mode: Option<Mode>) -> Cmd {
    let current = s
        .session
        .as_ref()
        .map(|session| session.mode)
        .or(s.pending_mode)
        .unwrap_or_default();
    let next = mode.unwrap_or(match current {
        Mode::Build => Mode::Plan,
        Mode::Plan => Mode::Build,
    });
    let Some(session) = s.session.as_ref() else {
        s.pending_mode = Some(next);
        return Cmd::None;
    };
    Cmd::PatchSession {
        id: session.id,
        patch: SessionPatch {
            mode: Some(next),
            ..Default::default()
        },
        from_rename: false,
    }
}

fn on_mode_command(s: &mut SessionState, args: &[&str], toast: &mut Option<Toast>) -> Cmd {
    match args.first().copied() {
        None => switch_mode(s, None),
        Some(raw) => match raw.parse::<Mode>() {
            Ok(mode) => switch_mode(s, Some(mode)),
            Err(_) => {
                *toast = Some(Toast::error("usage: /mode build|plan"));
                Cmd::None
            }
        },
    }
}

fn expand_pastes(s: &SessionState, text: &str) -> String {
    let mut expanded = text.to_string();
    for paste in &s.pasted {
        expanded = expanded.replace(&paste.marker, &paste.text);
    }
    expanded
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
    s.model_picker.picker.cursor = 0;
    if s.model_picker.models.is_none() {
        Cmd::FetchModels
    } else {
        Cmd::None
    }
}

/// Handle `/skills`: open the read-only skills overlay
fn on_skills_command(s: &mut SessionState) -> Cmd {
    s.overlay = Overlay::Skills;
    if s.skills.is_none() {
        Cmd::FetchSkills
    } else {
        Cmd::None
    }
}

/// Handle `/session`: open the list overlay (default), start a rename
/// (`/session rename`), or create a new session (`/session new <title>`).
/// Switching and deleting rows are handled in `on_session_key`. Always
/// fetches the list — the empty cache is indistinguishable from "never
/// fetched", and a fresh `/session` open should reflect any sessions
/// created since the last view.
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
        Some("new") => {
            // `/session new <title...>` — `<title>` is everything after the
            // subcommand, multi-word allowed.
            let title = args
                .get(1..)
                .map(|rest| rest.join(" "))
                .unwrap_or_default()
                .trim()
                .to_string();
            if title.is_empty() {
                *toast = Some(Toast::error("/session new needs a title"));
                return Cmd::None;
            }
            if s.creating {
                *toast = Some(Toast::error("a session is already being created"));
                return Cmd::None;
            }
            // Mirror the chat-first creation flow so a `Msg::SessionCreated`
            // result routes the new session into the session view.
            s.creating = true;
            s.creation_started_at = Some(std::time::Instant::now());
            s.input = TextArea::default();
            Cmd::CreateSession(CreateSessionRequest {
                title,
                model: s.pending_model,
                mode: Some(s.pending_mode.unwrap_or_default()),
            })
        }
        Some(other) => {
            *toast = Some(Toast::error(format!(
                "/session: unknown subcommand `/{}`",
                other
            )));
            Cmd::None
        }
        _ => {
            s.overlay = Overlay::SessionList;
            s.session_list.picker.cursor = 0;
            Cmd::FetchSessions
        }
    }
}
