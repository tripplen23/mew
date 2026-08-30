//! Provider connect dialog: pick provider, enter API key, validate, done.

use crossterm::event::{KeyCode, KeyEvent};
use tui_textarea::TextArea;

use mewcode_protocol::ProviderId;

use crate::runtime::model::{CONNECT_PROVIDERS, Cmd, ConnectStep, Overlay, SessionState};
use crate::runtime::update::key_to_input;

/// Handle key events while the provider connect dialog is open.
pub(super) fn on_connect_provider_key(s: &mut SessionState, key: KeyEvent) -> Cmd {
    use ConnectStep::*;
    use mewcode_protocol::credential::ConnectProviderRequest;

    if s.connect_provider.step == EnterKey {
        promote_composer_draft_to_connect_key(s);
    }

    let state = &mut s.connect_provider;

    match state.step {
        PickProvider => match key.code {
            KeyCode::Enter => {
                let provider = CONNECT_PROVIDERS
                    .get(state.picker.cursor)
                    .map(|descriptor| descriptor.id)
                    .unwrap_or(ProviderId::OpenCodeGo);
                state.selected_provider = Some(provider);
                state.step = EnterKey;
                state.key_input = TextArea::default();
                Cmd::None
            }
            KeyCode::Up => {
                state.picker.cursor = state.picker.cursor.saturating_sub(1);
                Cmd::None
            }
            KeyCode::Down => {
                state.picker.cursor =
                    (state.picker.cursor + 1).min(CONNECT_PROVIDERS.len().saturating_sub(1));
                Cmd::None
            }
            _ => Cmd::None,
        },
        EnterKey => match key.code {
            KeyCode::Enter => {
                let api_key = state.key_input.lines().join("\n").trim().to_string();
                if api_key.is_empty() {
                    state.error = Some("API key cannot be empty".to_string());
                    return Cmd::None;
                }
                let provider = state.selected_provider.unwrap_or(ProviderId::OpenCodeGo);
                state.error = None;
                state.step = Validating;
                state.attempt = state.attempt.wrapping_add(1);
                Cmd::ConnectProvider(ConnectProviderRequest { provider, api_key }, state.attempt)
            }
            _ => {
                state.key_input.input(key_to_input(key));
                Cmd::None
            }
        },
        Validating => Cmd::None,
        Done => {
            if key.code == KeyCode::Enter || key.code == KeyCode::Esc {
                s.overlay = Overlay::None;
                state.key_input = TextArea::default();
            }
            Cmd::None
        }
    }
}

fn promote_composer_draft_to_connect_key(s: &mut SessionState) {
    let draft = s.composer_text();
    if draft.is_empty() {
        return;
    }
    s.connect_provider.key_input.insert_str(&draft);
    s.clear_composer();
}
