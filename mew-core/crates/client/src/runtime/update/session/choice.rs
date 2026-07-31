//! Structured choice prompt overlay: cursor movement and selection, plus
//! submitting the choice response back to the session.

use crossterm::event::{KeyCode, KeyEvent};

use mewcode_protocol::event::{ChoiceResponse, ChoiceResponseRequest};

use crate::runtime::model::{Cmd, Overlay, SessionState};

pub(super) fn on_choice_key(s: &mut SessionState, key: KeyEvent) -> Cmd {
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

pub(crate) fn submit_choice_response(s: &SessionState, response: ChoiceResponse) -> Cmd {
    match s.session.as_ref() {
        Some(session) => Cmd::SubmitChoice(ChoiceResponseRequest {
            session_id: session.id,
            response,
        }),
        None => Cmd::None,
    }
}
