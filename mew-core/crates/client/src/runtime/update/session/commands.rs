//! Slash-command execution.

use tui_textarea::TextArea;

use mewcode_protocol::Mode;

use crate::net::{CreateSessionRequest, SessionPatch};

use crate::runtime::model::{Cmd, ConnectProviderState, Overlay, SessionState, Toast};

pub(super) fn switch_mode(s: &mut SessionState, mode: Option<Mode>) -> Cmd {
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

pub(super) fn on_mode_command(
    s: &mut SessionState,
    args: &[&str],
    toast: &mut Option<Toast>,
) -> Cmd {
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

pub(super) fn on_sound_command(
    s: &mut SessionState,
    args: &[&str],
    toast: &mut Option<Toast>,
) -> Cmd {
    match args.first().copied() {
        Some("on") => s.sound_enabled = true,
        Some("off") => s.sound_enabled = false,
        _ => s.sound_enabled = !s.sound_enabled,
    }
    let label = if s.sound_enabled { "on" } else { "off" };
    *toast = Some(Toast::info(format!("Sound: {label}")));
    Cmd::None
}

/// Handle `/model`: open the picker overlay, fetching the registry on
/// demand. Picking a row (Enter in the overlay) is handled in
/// `on_session_key`; this function only opens the dialog.
pub(super) fn on_model_command(s: &mut SessionState) -> Cmd {
    s.overlay = Overlay::ModelPicker;
    s.model_picker.picker.cursor = 0;
    if s.model_picker.models.is_none() {
        Cmd::FetchModels
    } else {
        Cmd::None
    }
}

/// Handle `/skills`: open the read-only skills overlay
pub(super) fn on_skills_command(s: &mut SessionState) -> Cmd {
    s.overlay = Overlay::Skills;
    if s.skills.is_none() {
        Cmd::FetchSkills
    } else {
        Cmd::None
    }
}

/// Handle `/compact`: trigger manual context compaction for the current session.
pub(super) fn on_compact_command(s: &mut SessionState, toast: &mut Option<Toast>) -> Cmd {
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
pub(super) fn on_session_command(
    s: &mut SessionState,
    args: &[&str],
    toast: &mut Option<Toast>,
) -> Cmd {
    match args.first().copied() {
        Some("rename") => {
            let Some(session) = s.session.as_ref() else {
                *toast = Some(Toast::error("/session rename needs an active session"));
                return Cmd::None;
            };
            // Pre-fill with current title; Enter in RenameSession reads the
            // new one from `s.composer`.
            s.composer = TextArea::new(vec![session.title.clone()]);
            s.overlay = Overlay::RenameSession;
            Cmd::None
        }
        Some("new") => {
            // `/session new <title...>` — explicit title creates it immediately.
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
                // Chat-first flow: `Msg::SessionCreated` routes the new
                // session into the session view.
                s.creation.creating = true;
                s.creation.creation_started_at = Some(std::time::Instant::now());
                s.clear_composer();
                return Cmd::CreateSession(CreateSessionRequest {
                    title,
                    model: s.creation.pending_model,
                    mode: Some(s.creation.pending_mode.unwrap_or_default()),
                });
            }

            // Bare `/session new` — back to entry view; the first message
            // creates the session and derives a title, like the very first one.
            let carried_model = s.session.as_ref().map(|sess| sess.model);
            let carried_mode = s.session.as_ref().map(|sess| sess.mode);
            let carried_sound = s.sound_enabled;
            let carried_pwd = s.pwd.clone();
            *s = SessionState::empty();
            s.creation.pending_model = carried_model;
            s.creation.pending_mode = carried_mode;
            s.sound_enabled = carried_sound;
            s.pwd = carried_pwd;
            Cmd::None
        }
        Some(other) => {
            *toast = Some(Toast::error(format!(
                "/session: unknown subcommand `{}`",
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

/// Handle `/connect`: open the provider connect dialog.
pub(super) fn on_connect_command(s: &mut SessionState) -> Cmd {
    s.overlay = Overlay::ConnectProvider;
    let next = s.connect_provider.attempt.wrapping_add(1);
    s.connect_provider = ConnectProviderState {
        attempt: next,
        ..Default::default()
    };
    s.clear_composer();
    Cmd::None
}
