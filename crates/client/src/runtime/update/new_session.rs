use crossterm::event::{KeyCode, KeyEvent};

use mewcode_protocol::{Mode, ModelId};

use crate::net::CreateSessionRequest;

use super::super::model::{Cmd, HomeState, NewSessionField, Screen, Toast};
use super::key_to_input;

/// NewSession screen: field cycling, picker changes, validation, and submit.
pub(super) fn on_new_session_key(
    screen: &mut Screen,
    toast: &mut Option<Toast>,
    key: KeyEvent,
) -> Cmd {
    if key.code == KeyCode::Esc {
        *screen = Screen::Home(HomeState::loading());
        return Cmd::LoadSessions;
    }

    let Screen::NewSession(n) = screen else {
        return Cmd::None;
    };

    match key.code {
        KeyCode::Tab => {
            n.field = n.field.next();
            Cmd::None
        }
        KeyCode::Enter => {
            let title = n.title.lines().join("\n");
            let trimmed = title.trim();
            if trimmed.is_empty() {
                n.field = NewSessionField::Title;
                *toast = Some(Toast::error("a non-empty title is required"));
                Cmd::None
            } else {
                let model = ModelId::ALL.get(n.model_idx).copied().unwrap_or_default();
                Cmd::CreateSession(CreateSessionRequest {
                    title: trimmed.to_string(),
                    model: Some(model),
                    mode: Some(n.mode),
                })
            }
        }
        KeyCode::Left => {
            match n.field {
                NewSessionField::Model => n.model_idx = n.model_idx.saturating_sub(1),
                NewSessionField::Mode => n.mode = toggle_mode(n.mode),
                // Pass Left through so the TextArea can move the cursor.
                NewSessionField::Title => {
                    n.title.input(key_to_input(key));
                }
            }
            Cmd::None
        }
        KeyCode::Right => {
            match n.field {
                NewSessionField::Model => {
                    n.model_idx = (n.model_idx + 1).min(ModelId::ALL.len().saturating_sub(1))
                }
                NewSessionField::Mode => n.mode = toggle_mode(n.mode),
                // Pass Right through so the TextArea can move the cursor.
                NewSessionField::Title => {
                    n.title.input(key_to_input(key));
                }
            }
            Cmd::None
        }
        _ => {
            if n.field == NewSessionField::Title {
                n.title.input(key_to_input(key));
            }
            Cmd::None
        }
    }
}

/// Toggle between the two interaction modes.
fn toggle_mode(mode: Mode) -> Mode {
    match mode {
        Mode::Build => Mode::Plan,
        Mode::Plan => Mode::Build,
    }
}
