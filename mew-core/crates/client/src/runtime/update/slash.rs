//! Slash-command picker state transitions.
//!
//! The picker only chooses text commands; actual command execution stays in
//! the session submit path so typed `/model` and picked `/model` behave the
//! same way.

use crossterm::event::{KeyCode, KeyEvent};
use tui_textarea::TextArea;

use super::super::model::{Cmd, Overlay, SLASH_COMMANDS, SessionState};
use super::key_to_input;

/// Result of handling a slash-picker key.
pub(super) enum SlashPickerResult {
    /// A normal runtime command was produced.
    Cmd(Cmd),
    /// The picker seeded the composer and the session submit parser should run.
    Submit,
}

/// Handle one key while the slash-command picker is active.
pub(super) fn on_slash_picker_key(s: &mut SessionState, key: KeyEvent) -> SlashPickerResult {
    match key.code {
        KeyCode::Up => {
            slash_cursor_move(s, -1);
            SlashPickerResult::Cmd(Cmd::None)
        }
        KeyCode::Down => {
            slash_cursor_move(s, 1);
            SlashPickerResult::Cmd(Cmd::None)
        }
        KeyCode::Enter => {
            let cmd_text = SLASH_COMMANDS
                .get(s.slash_cursor)
                .map(|c| c.command)
                .unwrap_or("/model")
                .to_string();
            if cmd_text == "quit" {
                s.input = TextArea::default();
                s.overlay = Overlay::None;
                return SlashPickerResult::Cmd(Cmd::Quit);
            }
            s.input = TextArea::new(vec![format!("{cmd_text} ")]);
            s.overlay = Overlay::None;
            SlashPickerResult::Submit
        }
        _ => {
            s.input.input(key_to_input(key));
            if let Some(next) = slash_default_cursor(&s.input.lines().join("\n")) {
                s.slash_cursor = next;
            } else {
                s.overlay = Overlay::None;
            }
            SlashPickerResult::Cmd(Cmd::None)
        }
    }
}

/// Open the picker and highlight the best match for the current composer text.
pub(super) fn open_slash_picker(s: &mut SessionState) {
    s.overlay = Overlay::SlashPicker;
    s.slash_cursor = slash_default_cursor(&s.input.lines().join("\n")).unwrap_or(0);
}

/// Return the first slash-command row matching a composer prefix.
pub(super) fn slash_default_cursor(prefix: &str) -> Option<usize> {
    let trimmed = prefix.trim();
    if !trimmed.starts_with('/') {
        return None;
    }
    let needle = trimmed.trim_start_matches('/');
    SLASH_COMMANDS
        .iter()
        .position(|cmd| cmd.command.trim_start_matches('/').starts_with(needle))
}

fn slash_cursor_move(s: &mut SessionState, delta: i32) {
    if SLASH_COMMANDS.is_empty() {
        return;
    }
    let max = (SLASH_COMMANDS.len() - 1) as i32;
    s.slash_cursor = (s.slash_cursor as i32 + delta).clamp(0, max) as usize;
}
