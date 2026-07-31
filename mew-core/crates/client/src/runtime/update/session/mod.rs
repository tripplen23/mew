//! Session screen update: routes keys and overlay events to the
//! per-feature handlers in the sibling modules (`composer`, `commands`,
//! `choice`, `connect`, `picker`, `slash`).

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use mewcode_protocol::event::ChoiceCancelReason;

use crate::net::SessionPatch;

use crate::runtime::model::{Cmd, ConnectProviderState, Overlay, SessionState, Toast};
use crate::runtime::update::key_to_input;

use choice::on_choice_key;
use commands::switch_mode;
use composer::on_session_submit;
use connect::on_connect_provider_key;
use picker::{
    on_file_picker_key, on_model_picker_key, on_session_list_key, open_file_picker,
    refresh_file_picker,
};
use slash::{SlashPickerResult, on_slash_picker_key, open_slash_picker, slash_default_cursor};

mod choice;
mod commands;
mod composer;
mod connect;
pub(super) mod picker;
pub(super) mod slash;

pub(super) use choice::submit_choice_response;
pub(super) use composer::on_session_paste;

/// Session screen: composer editing, submit, slash commands.
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
            // `Overlay::RenameSession` seeds `s.composer` with the current
            // session title so the user can edit it in place.
            let was_rename = s.overlay == Overlay::RenameSession;
            let was_slash = s.overlay == Overlay::SlashPicker;
            let was_connect = s.overlay == Overlay::ConnectProvider;
            s.overlay = Overlay::None;
            if was_rename {
                s.clear_composer();
            }
            if was_slash {
                // The picker only opens when the composer starts with `/`,
                s.clear_composer();
            }
            if was_connect {
                let prev_attempt = s.connect_provider.attempt;
                s.connect_provider = ConnectProviderState::default();
                s.connect_provider.attempt = prev_attempt.wrapping_add(1);
            }
        }
        return Cmd::None;
    }

    if s.creation.creating {
        // A `POST /sessions` is in flight for `pending_chat`
        return Cmd::None;
    }

    match s.overlay {
        Overlay::SlashPicker => match on_slash_picker_key(s, key) {
            SlashPickerResult::Cmd(cmd) => return cmd,
            SlashPickerResult::Submit => return on_session_submit(s, toast),
        },
        Overlay::ModelPicker => return on_model_picker_key(s, key),
        Overlay::FilePicker => return on_file_picker_key(s, key),
        Overlay::Choice => return on_choice_key(s, key),
        Overlay::ConnectProvider => return on_connect_provider_key(s, key),
        Overlay::SessionList => return on_session_list_key(s, key),
        Overlay::RenameSession => {
            if key.code == KeyCode::Enter {
                if let Some(session) = s.session.as_ref() {
                    let title = s.composer_text().trim().to_string();
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
            // Non-Enter keys fall through so typing edits the title.
        }
        Overlay::None | Overlay::Tools | Overlay::Skills | Overlay::Theme => {}
    }

    match key.code {
        KeyCode::Tab => switch_mode(s, None),

        KeyCode::Char('@') => {
            s.composer.input(key_to_input(key));
            open_file_picker(s)
        }

        KeyCode::Char('/') => {
            s.composer.input(key_to_input(key));
            if slash_default_cursor(&s.composer_text()).is_some() {
                open_slash_picker(s);
            }
            Cmd::None
        }

        KeyCode::Enter if key.modifiers.contains(KeyModifiers::ALT) => {
            s.composer.insert_newline();
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
            s.composer.input(key_to_input(key));
            if s.overlay == Overlay::None {
                return refresh_file_picker(s);
            }
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
