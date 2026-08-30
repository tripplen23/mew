//! Composer editing: paste handling, Enter/submit, slash-command dispatch,
//! and session creation from the first message.

use uuid::Uuid;

use mewcode_protocol::event::ChatRequest;
use mewcode_protocol::{Message, MessagePart};

use crate::net::CreateSessionRequest;

use super::commands::{
    on_compact_command, on_connect_command, on_mode_command, on_model_command, on_session_command,
    on_skills_command, on_sound_command,
};
use crate::runtime::model::{
    Cmd, ConnectStep, Overlay, PASTED_MARKER_PREFIX, PastedText, QUIT_COMMAND, SessionState,
    SlashCommandKind, StreamingState, Toast, slash_command_by_token,
};

const COMPACT_PASTE_CHARS: usize = 120;

pub(crate) fn on_session_paste(s: &mut SessionState, text: String) -> Cmd {
    if s.creation.creating {
        return Cmd::None;
    }

    // Paste into the overlay's key input, not the composer.
    if s.overlay == Overlay::ConnectProvider && s.connect_provider.step == ConnectStep::EnterKey {
        s.connect_provider.key_input.insert_str(text);
        return Cmd::None;
    }

    let char_count = text.chars().count();
    // split('\n') counts a trailing empty line, so "foo\n" isn't compacted.
    let line_count = text.split('\n').count();
    let has_trailing_terminator = text.ends_with('\n') || text.ends_with('\r');
    if !has_trailing_terminator && line_count == 1 && char_count <= COMPACT_PASTE_CHARS {
        s.composer.insert_str(text);
        return Cmd::None;
    }

    let marker = if line_count > 1 {
        format!("{PASTED_MARKER_PREFIX}{line_count} lines]")
    } else {
        format!("{PASTED_MARKER_PREFIX}{char_count} chars]")
    };
    s.composer.insert_str(&marker);
    s.pasted.push(PastedText { marker, text });
    Cmd::None
}

/// Handle `Enter` in the Session composer bar: the `quit` text command,
/// slash commands, or — if no session exists yet — create one with the
/// typed text as the seed, or send the chat into the existing session.
pub(super) fn on_session_submit(s: &mut SessionState, toast: &mut Option<Toast>) -> Cmd {
    let visible_text = s.composer_text();
    let visible_trimmed = visible_text.trim();

    if visible_trimmed.is_empty() {
        return Cmd::None;
    }

    // Text-based quit.
    if visible_trimmed.eq_ignore_ascii_case(QUIT_COMMAND) {
        s.clear_composer();
        return Cmd::Quit;
    }

    // Queue message when turn in-flight. Auto-sent in order when finished.
    if s.streaming.is_some() || s.compaction.active {
        let expanded = expand_pastes(s, &visible_text);
        s.message_queue.push(expanded.trim().to_string());
        s.clear_composer();
        return Cmd::None;
    }

    if let Some(rest) = visible_trimmed.strip_prefix('/') {
        let mut parts = rest.split_whitespace();
        let cmd = parts.next().unwrap_or("");
        let args: Vec<&str> = parts.collect();
        let Some(command) = slash_command_by_token(cmd) else {
            return submit_chat_text(s, &visible_text);
        };
        s.clear_composer();
        return match command.kind {
            SlashCommandKind::Tools => {
                s.overlay = Overlay::Tools;
                Cmd::None
            }
            SlashCommandKind::Skills => on_skills_command(s),
            SlashCommandKind::Theme => {
                s.overlay = Overlay::Theme;
                Cmd::None
            }
            SlashCommandKind::Mode => on_mode_command(s, &args, toast),
            SlashCommandKind::Sound => on_sound_command(s, &args, toast),
            SlashCommandKind::Model => on_model_command(s),
            SlashCommandKind::Session => on_session_command(s, &args, toast),
            SlashCommandKind::Connect => on_connect_command(s),
            SlashCommandKind::Compact => on_compact_command(s, toast),
            SlashCommandKind::Quit => Cmd::Quit,
        };
    }

    submit_chat_text(s, &visible_text)
}

fn submit_chat_text(s: &mut SessionState, visible_text: &str) -> Cmd {
    let text = expand_pastes(s, visible_text);
    let trimmed = text.trim();
    let user_text = trimmed.to_string();
    let user_msg = Message::user(vec![MessagePart::Text {
        text: user_text.clone(),
    }]);

    if let Some(session) = s.session.as_mut() {
        session.messages.push(user_msg);
        // Snap back to the latest line so the user watches the reply stream in.
        s.follow = true;
        // `Uuid::nil()` placeholder; the real id arrives with SSE `Started`.
        s.streaming = Some(StreamingState::new(Uuid::nil()));
        let session_id = session.id;
        let model = session.model.clone();
        let mode = session.mode;
        let messages = session.messages.clone();
        // Clear the composer now that the message is committed to history.
        s.clear_composer();
        Cmd::StartChat(ChatRequest {
            session_id,
            model,
            provider: None,
            mode,
            messages,
        })
    } else {
        // No session yet — buffer the text so the user can retry on a
        // create failure; `Msg::SessionCreated` clears it once committed.
        s.creation.pending_chat = Some(user_text.clone());
        s.creation.creating = true;
        s.creation.creation_started_at = Some(std::time::Instant::now());
        Cmd::CreateSession(CreateSessionRequest {
            title: derive_title(&user_text),
            model: s.creation.pending_model.clone(),
            model_kind: s.creation.pending_model_kind,
            model_context_length: s.creation.pending_model_context_length,
            mode: Some(s.creation.pending_mode.unwrap_or_default()),
        })
    }
}

fn expand_pastes(s: &SessionState, text: &str) -> String {
    let mut expanded = text.to_string();
    for paste in &s.pasted {
        // Replace the first remaining occurrence so identical markers
        // expand to their own text.
        // Assumes markers appear in paste order; breaks if re-ordered.
        if let Some(pos) = expanded.find(&paste.marker) {
            expanded.replace_range(pos..pos + paste.marker.len(), &paste.text);
        }
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
