use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use tui_textarea::TextArea;
use uuid::Uuid;

use mewcode_protocol::event::{
    ChatRequest, ChoiceCancelReason, ChoiceResponse, ChoiceResponseRequest,
};
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
            if s.overlay == Overlay::Choice {
                if let Some(choice) = s.pending_choice.as_mut() {
                    choice.cancel(ChoiceCancelReason::User);
                    let response = choice.response.clone().unwrap();
                    s.overlay = Overlay::None;
                    return submit_choice_response(s, response);
                }
                s.overlay = Overlay::None;
                return Cmd::None;
            }
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

    if s.creation.creating {
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
        Overlay::Choice => return on_choice_key(s, key),
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

        KeyCode::Enter if key.modifiers.contains(KeyModifiers::ALT) => {
            s.input.insert_newline();
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

fn on_choice_key(s: &mut SessionState, key: KeyEvent) -> Cmd {
    let Some(choice) = s.pending_choice.as_mut() else {
        s.overlay = Overlay::None;
        return Cmd::None;
    };
    let len = choice.request.options.len();
    match key.code {
        KeyCode::Up => {
            if len > 0 {
                choice.picker.cursor = choice.picker.cursor.saturating_sub(1);
            }
        }
        KeyCode::Down => {
            if len > 0 {
                choice.picker.cursor = (choice.picker.cursor + 1).min(len - 1);
            }
        }
        KeyCode::Enter => {
            if let Some(option) = choice.request.options.get(choice.picker.cursor) {
                let response = ChoiceResponse::Selected {
                    request_id: choice.request.request_id.clone(),
                    option_id: option.id.clone(),
                };
                choice.response = Some(response.clone());
                s.overlay = Overlay::None;
                return submit_choice_response(s, response);
            }
        }
        _ => {}
    }
    Cmd::None
}

pub(super) fn submit_choice_response(s: &SessionState, response: ChoiceResponse) -> Cmd {
    match s.session.as_ref() {
        Some(session) => Cmd::SubmitChoice(ChoiceResponseRequest {
            session_id: session.id,
            response,
        }),
        None => Cmd::None,
    }
}

pub(super) fn on_session_paste(s: &mut SessionState, text: String) -> Cmd {
    if s.creation.creating {
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

    // Queue message when turn in-flight. Auto-sent in order when finished.
    if s.streaming.is_some() || s.compaction.active {
        s.message_queue.push(visible_trimmed.to_string());
        s.input = TextArea::default();
        s.pasted.clear();
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
            "sound" => on_sound_command(s, &args, toast),
            "model" => on_model_command(s),
            "session" => on_session_command(s, &args, toast),
            "compact" => on_compact_command(s, toast),
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
        s.creation.pending_chat = Some(user_text.clone());
        s.creation.creating = true;
        s.creation.creation_started_at = Some(std::time::Instant::now());
        Cmd::CreateSession(CreateSessionRequest {
            title: derive_title(&user_text),
            model: s.creation.pending_model,
            mode: Some(s.creation.pending_mode.unwrap_or_default()),
        })
    }
}

fn switch_mode(s: &mut SessionState, mode: Option<Mode>) -> Cmd {
    let current = s
        .session
        .as_ref()
        .map(|session| session.mode)
        .or(s.creation.pending_mode)
        .unwrap_or_default();
    let next = mode.unwrap_or(match current {
        Mode::Build => Mode::Plan,
        Mode::Plan => Mode::Build,
    });
    let Some(session) = s.session.as_ref() else {
        s.creation.pending_mode = Some(next);
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

fn on_sound_command(s: &mut SessionState, args: &[&str], toast: &mut Option<Toast>) -> Cmd {
    match args.first().copied() {
        Some("on") => s.sound_enabled = true,
        Some("off") => s.sound_enabled = false,
        _ => s.sound_enabled = !s.sound_enabled,
    }
    let label = if s.sound_enabled { "on" } else { "off" };
    *toast = Some(Toast::info(format!("Sound: {label}")));
    Cmd::None
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

/// Handle `/compact`: trigger manual context compaction for the current session.
fn on_compact_command(s: &mut SessionState, toast: &mut Option<Toast>) -> Cmd {
    let Some(session) = s.session.as_ref() else {
        *toast = Some(Toast::error("no active session to compact"));
        return Cmd::None;
    };
    if s.streaming.is_some() {
        *toast = Some(Toast::error("cannot compact while a turn is in flight"));
        return Cmd::None;
    }
    if s.compaction.active {
        *toast = Some(Toast::error("compaction already in progress"));
        return Cmd::None;
    }
    s.compaction.active = true;
    s.compaction.started_at = Some(std::time::Instant::now());
    Cmd::Compact(session.id)
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
            // `/session new <title...>` — an explicit title still creates
            // the session immediately (unchanged from before).
            let title = args
                .get(1..)
                .map(|rest| rest.join(" "))
                .unwrap_or_default()
                .trim()
                .to_string();
            if !title.is_empty() {
                if s.creation.creating {
                    *toast = Some(Toast::error("a session is already being created"));
                    return Cmd::None;
                }
                // Mirror the chat-first creation flow so a `Msg::SessionCreated`
                // result routes the new session into the session view.
                s.creation.creating = true;
                s.creation.creation_started_at = Some(std::time::Instant::now());
                s.input = TextArea::default();
                return Cmd::CreateSession(CreateSessionRequest {
                    title,
                    model: s.creation.pending_model,
                    mode: Some(s.creation.pending_mode.unwrap_or_default()),
                });
            }

            // Bare `/session new` — no title yet. Drop back to the entry
            // view (the Mew mascot screen) with no session and no pending
            // request; a session is only actually created once the user
            // sends their first message, which derives a title from it —
            // mirroring the very first session's creation flow.
            let carried_model = s.session.as_ref().map(|sess| sess.model);
            let carried_mode = s.session.as_ref().map(|sess| sess.mode);
            *s = SessionState::empty();
            s.creation.pending_model = carried_model;
            s.creation.pending_mode = carried_mode;
            Cmd::None
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
