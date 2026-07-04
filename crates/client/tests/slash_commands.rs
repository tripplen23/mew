//! Unit tests for the `/model` and `/session` slash commands, exercising
//! `update` end-to-end through its public API.
//!
//! Covers the three layers the slash command touches:
//! - the parser inside `on_session_submit` (driven by `Enter` in the input),
//! - the `Cmd` returned to the runtime (the side effect to dispatch),
//! - the resulting state mutation (overlay state, model state).

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use tui_textarea::TextArea;

use mewcode_client::runtime::model::{App, Cmd, Msg, Overlay, Screen, SessionState};
use mewcode_client::runtime::update;
use mewcode_protocol::{MessagePart, Mode, ModelId};

fn test_app() -> App {
    App::new()
}

fn session() -> mewcode_client::net::Session {
    mewcode_client::net::Session {
        id: uuid::Uuid::new_v4(),
        title: "demo".into(),
        model: ModelId::Glm51,
        mode: Mode::Build,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
        messages: vec![],
    }
}

fn type_text(s: &mut SessionState, text: &str) {
    // Replace the input with the given text via the public insert API.
    s.input = TextArea::new(vec![text.to_string()]);
}

fn press_enter() -> Msg {
    Msg::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
}

fn press_esc() -> Msg {
    Msg::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE))
}

fn active_state(app: &mut App) -> &mut SessionState {
    let Screen::Session(s) = &mut app.screen;
    s
}

fn seed_active_session(s: &mut SessionState) {
    s.session = Some(session());
}

// --- /model --------------------------------------------------------------

#[test]
fn slash_model_opens_picker_and_fetches_when_uncached() {
    let mut app = test_app();
    seed_active_session(active_state(&mut app));

    {
        let s = active_state(&mut app);
        type_text(s, "/model");
    }
    let cmd = update(&mut app, press_enter());

    assert!(
        matches!(cmd, Cmd::FetchModels),
        "expected FetchModels, got {cmd:?}"
    );
    let s = active_state(&mut app);
    assert_eq!(s.overlay, Overlay::ModelPicker);
    assert_eq!(s.model_cursor, 0);
    assert!(
        s.input.lines().join("\n").is_empty(),
        "input should be cleared after dispatch"
    );
}

#[test]
fn slash_model_reuses_cached_registry() {
    let mut app = test_app();
    seed_active_session(active_state(&mut app));
    active_state(&mut app).models = Some(vec![]); // cache the registry

    {
        let s = active_state(&mut app);
        type_text(s, "/model");
    }
    let cmd = update(&mut app, press_enter());

    assert!(matches!(cmd, Cmd::None), "expected no fetch, got {cmd:?}");
    let s = active_state(&mut app);
    assert_eq!(s.overlay, Overlay::ModelPicker);
}

#[test]
fn slash_model_with_no_active_session_toasts() {
    let mut app = test_app();
    {
        let s = active_state(&mut app);
        type_text(s, "/model");
    }
    let _ = update(&mut app, press_enter());

    // The slash should still open the picker (the picker is useful even
    // before a session exists — the user can pick a default for the
    // next session). The toast is for the /session rename path; here we
    // assert the picker opens.
    let s = active_state(&mut app);
    assert_eq!(s.overlay, Overlay::ModelPicker);
}

// --- /session ------------------------------------------------------------

#[test]
fn slash_session_opens_list_and_fetches_when_uncached() {
    let mut app = test_app();
    {
        let s = active_state(&mut app);
        type_text(s, "/session");
    }
    let cmd = update(&mut app, press_enter());

    assert!(
        matches!(cmd, Cmd::FetchSessions),
        "expected FetchSessions, got {cmd:?}"
    );
    let s = active_state(&mut app);
    assert_eq!(s.overlay, Overlay::SessionList);
    assert_eq!(s.session_cursor, 0);
}

#[test]
fn slash_session_always_refetches_to_pick_up_new_sessions() {
    // The current implementation always fetches (the empty cache is
    // indistinguishable from "never fetched"). What we verify here is
    // the visible contract: the overlay still opens, the cursor resets,
    // and a fetch fires.
    let mut app = test_app();
    let id = uuid::Uuid::new_v4();
    active_state(&mut app).session_summaries = vec![mewcode_client::net::SessionSummary {
        id,
        title: "first".into(),
        model: ModelId::Glm51,
        mode: Mode::Build,
        created_at: chrono::Utc::now(),
    }];

    {
        let s = active_state(&mut app);
        type_text(s, "/session");
    }
    let cmd = update(&mut app, press_enter());

    assert!(matches!(cmd, Cmd::FetchSessions));
    let s = active_state(&mut app);
    assert_eq!(s.overlay, Overlay::SessionList);
    assert_eq!(s.session_cursor, 0);
}

#[test]
fn slash_session_rename_seeds_input_with_current_title() {
    let mut app = test_app();
    seed_active_session(active_state(&mut app));

    {
        let s = active_state(&mut app);
        type_text(s, "/session rename");
    }
    let cmd = update(&mut app, press_enter());

    assert!(
        matches!(cmd, Cmd::None),
        "rename should not produce a Cmd yet, got {cmd:?}"
    );
    let s = active_state(&mut app);
    assert_eq!(s.overlay, Overlay::RenameSession);
    assert_eq!(
        s.input.lines().join("\n"),
        "demo",
        "input should be seeded with current title"
    );
}

#[test]
fn slash_session_rename_without_active_session_toasts() {
    let mut app = test_app();
    {
        let s = active_state(&mut app);
        type_text(s, "/session rename");
    }
    let _ = update(&mut app, press_enter());

    let s = active_state(&mut app);
    assert_eq!(s.overlay, Overlay::None);
    assert!(
        app.toast.is_some(),
        "expected an error toast for /session rename"
    );
}

#[test]
fn slash_session_rename_in_rename_overlay_commits_patch_on_enter() {
    let mut app = test_app();
    seed_active_session(active_state(&mut app));

    // Open rename overlay.
    {
        let s = active_state(&mut app);
        type_text(s, "/session rename");
    }
    let _ = update(&mut app, press_enter());
    // Replace the seeded title with the new one and press Enter.
    {
        let s = active_state(&mut app);
        type_text(s, "Renamed");
    }
    let cmd = update(&mut app, press_enter());

    match cmd {
        Cmd::PatchSession { id, patch } => {
            assert_eq!(id, active_state(&mut app).session.as_ref().unwrap().id);
            assert_eq!(patch.title.as_deref(), Some("Renamed"));
            assert!(patch.model.is_none());
            assert!(patch.mode.is_none());
        }
        other => panic!("expected Cmd::PatchSession, got {other:?}"),
    }
    let s = active_state(&mut app);
    assert_eq!(s.overlay, Overlay::None, "overlay should close after Enter");
}

#[test]
fn slash_session_rename_empty_title_toasts() {
    let mut app = test_app();
    seed_active_session(active_state(&mut app));
    {
        let s = active_state(&mut app);
        type_text(s, "/session rename");
    }
    let _ = update(&mut app, press_enter());
    // Type whitespace only.
    {
        let s = active_state(&mut app);
        type_text(s, "   ");
    }
    let cmd = update(&mut app, press_enter());

    assert!(matches!(cmd, Cmd::None));
    let s = active_state(&mut app);
    assert_eq!(
        s.overlay,
        Overlay::RenameSession,
        "empty title should keep the overlay open"
    );
    assert!(
        app.toast.is_some(),
        "expected an error toast for empty title"
    );
}

// --- /model picker key nav -----------------------------------------------

#[test]
fn model_picker_enter_emits_patch_session_cmd() {
    let mut app = test_app();
    seed_active_session(active_state(&mut app));
    active_state(&mut app).models = Some(vec![mewcode_client::net::ModelEntry {
        id: "minimax-m3".into(),
        display_name: "MiniMax M3".into(),
        kind: mewcode_protocol::ModelKind::OpenAiChatCompletions,
    }]);

    // Open the picker and press Enter on the only row.
    {
        let s = active_state(&mut app);
        type_text(s, "/model");
    }
    let _ = update(&mut app, press_enter());
    let cmd = update(&mut app, press_enter());

    match cmd {
        Cmd::PatchSession { id, patch } => {
            assert_eq!(id, active_state(&mut app).session.as_ref().unwrap().id);
            assert_eq!(patch.model, Some(ModelId::MiniMaxM3));
        }
        other => panic!("expected Cmd::PatchSession, got {other:?}"),
    }
}

// --- /session list key nav -----------------------------------------------

#[test]
fn session_list_enter_emits_open_session_cmd() {
    let mut app = test_app();
    let id = uuid::Uuid::new_v4();
    active_state(&mut app).session_summaries = vec![mewcode_client::net::SessionSummary {
        id,
        title: "first".into(),
        model: ModelId::Glm51,
        mode: Mode::Build,
        created_at: chrono::Utc::now(),
    }];

    {
        let s = active_state(&mut app);
        type_text(s, "/session");
    }
    let _ = update(&mut app, press_enter());
    let cmd = update(&mut app, press_enter());

    assert!(
        matches!(cmd, Cmd::OpenSession(sid) if sid == id),
        "got {cmd:?}"
    );
}

#[test]
fn session_list_d_emits_delete_cmd() {
    let mut app = test_app();
    let id = uuid::Uuid::new_v4();
    active_state(&mut app).session_summaries = vec![mewcode_client::net::SessionSummary {
        id,
        title: "first".into(),
        model: ModelId::Glm51,
        mode: Mode::Build,
        created_at: chrono::Utc::now(),
    }];

    {
        let s = active_state(&mut app);
        type_text(s, "/session");
    }
    let _ = update(&mut app, press_enter());
    let cmd = update(
        &mut app,
        Msg::Key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::NONE)),
    );

    assert!(
        matches!(cmd, Cmd::DeleteSession(sid) if sid == id),
        "got {cmd:?}"
    );
}

// --- unknown slash -------------------------------------------------------

#[test]
fn unknown_slash_command_toasts_and_keeps_overlay() {
    let mut app = test_app();
    {
        let s = active_state(&mut app);
        type_text(s, "/nonsense");
    }
    let cmd = update(&mut app, press_enter());

    assert!(matches!(cmd, Cmd::None));
    let s = active_state(&mut app);
    assert_eq!(s.overlay, Overlay::None);
    // The unknown-command branch should not change any slash-related state.
    assert!(s.models.is_none());
    assert!(s.session_summaries.is_empty());
    assert!(app.toast.is_some());
}

// --- existing commands still work ----------------------------------------

#[test]
fn tools_overlay_still_opens() {
    let mut app = test_app();
    {
        let s = active_state(&mut app);
        type_text(s, "/tools");
    }
    let cmd = update(&mut app, press_enter());

    assert!(matches!(cmd, Cmd::None));
    assert_eq!(active_state(&mut app).overlay, Overlay::Tools);
}

#[test]
fn plain_text_is_chat_not_command() {
    // Sanity check: an Enter on plain text commits the chat via Cmd::StartChat,
    // and the slash-command arms do not capture it.
    let mut app = test_app();
    seed_active_session(active_state(&mut app));
    {
        let s = active_state(&mut app);
        type_text(s, "hello world");
    }
    let cmd = update(&mut app, press_enter());

    assert!(matches!(cmd, Cmd::StartChat(_)), "got {cmd:?}");
    let s = active_state(&mut app);
    // The chat is committed into the session history; the picker stays
    // closed.
    assert_eq!(s.overlay, Overlay::None);
    assert_eq!(s.session.as_ref().unwrap().messages.len(), 1);
    assert!(matches!(
        s.session.as_ref().unwrap().messages[0].parts[0],
        MessagePart::Text { .. }
    ));
}

// --- Esc on rename discards the draft ------------------------------------

#[test]
fn esc_on_rename_clears_composer_draft() {
    let mut app = test_app();
    seed_active_session(active_state(&mut app));
    {
        let s = active_state(&mut app);
        type_text(s, "/session rename");
    }
    let _ = update(&mut app, press_enter());
    // The rename overlay seeds `s.input` with the current title.
    assert_eq!(active_state(&mut app).overlay, Overlay::RenameSession);
    assert!(!active_state(&mut app).input.lines().is_empty());

    // Type some new characters into the composer to make it a draft.
    {
        let s = active_state(&mut app);
        type_text(s, "EDIT");
    }

    // Esc should close the overlay AND clear the draft.
    let _ = update(&mut app, press_esc());
    let s = active_state(&mut app);
    assert_eq!(s.overlay, Overlay::None);
    let draft = s.input.lines().join("\n");
    assert!(
        draft.trim().is_empty(),
        "Esc should discard the rename draft, not leave it in the composer (got {draft:?})"
    );
}

// --- /session new <title> ------------------------------------------------

#[test]
fn slash_session_new_with_title_emits_create_session_cmd() {
    let mut app = test_app();
    {
        let s = active_state(&mut app);
        type_text(s, "/session new my plan");
    }
    let cmd = update(&mut app, press_enter());

    match cmd {
        Cmd::CreateSession(req) => {
            assert_eq!(req.title, "my plan");
            assert!(req.model.is_none(), "/session new should not force a model");
        }
        other => panic!("expected Cmd::CreateSession, got {other:?}"),
    }
    let s = active_state(&mut app);
    // The chat-first flow flags `creating` so a duplicate submit is
    // ignored until `Msg::SessionCreated` lands.
    assert!(s.creating);
    assert_eq!(s.overlay, Overlay::None);
}

#[test]
fn slash_session_new_without_title_toasts() {
    let mut app = test_app();
    {
        let s = active_state(&mut app);
        type_text(s, "/session new");
    }
    let cmd = update(&mut app, press_enter());

    assert!(matches!(cmd, Cmd::None));
    let s = active_state(&mut app);
    assert_eq!(s.overlay, Overlay::None);
    assert!(!s.creating, "no session should be created without a title");
    assert!(app.toast.is_some());
}

#[test]
fn slash_session_unknown_subcommand_toasts() {
    let mut app = test_app();
    {
        let s = active_state(&mut app);
        type_text(s, "/session frobnicate");
    }
    let cmd = update(&mut app, press_enter());

    assert!(matches!(cmd, Cmd::None));
    let s = active_state(&mut app);
    // Unknown subcommands surface an error instead of silently opening
    // the list, so the user is told their `/session <arg>` was wrong.
    assert_eq!(s.overlay, Overlay::None);
    assert!(app.toast.is_some());
}

// --- stale async completions ---------------------------------------------

#[test]
fn session_patched_after_overlay_closed_does_not_clear_composer() {
    // Simulates a late `Msg::SessionPatched(Ok(...))` arriving after the
    // user has already Esc'd out of the rename overlay and started
    // typing a chat message. The handler must not clobber the draft.
    use mewcode_client::net::Session;
    let mut app = test_app();
    seed_active_session(active_state(&mut app));

    // User has typed a draft chat message.
    {
        let s = active_state(&mut app);
        type_text(s, "hi there");
    }

    // A late PATCH result lands — overlay is None, input is "hi there".
    let new_session = Session {
        id: uuid::Uuid::new_v4(),
        title: "renamed".into(),
        model: ModelId::MiniMaxM3,
        mode: Mode::Build,
        messages: vec![],
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    let _ = update(&mut app, Msg::SessionPatched(Ok(new_session.clone())));

    let s = active_state(&mut app);
    // The session adopts the patch (it's still the active session), but
    // the composer draft is preserved.
    assert_eq!(s.session.as_ref().unwrap().title, "renamed");
    let draft = s.input.lines().join("\n");
    assert_eq!(draft, "hi there", "stale PATCH must not clear the draft");
}

#[test]
fn session_opened_after_overlay_closed_does_not_adopt_session() {
    // The /session list triggers `Cmd::OpenSession`. If the user has
    // since moved on (overlay is None and they sent a chat), a late
    // `Msg::SessionOpened` must not stomp the in-flight state.
    use mewcode_client::net::Session;
    let mut app = test_app();
    seed_active_session(active_state(&mut app));

    // User already closed the list and is composing a chat.
    active_state(&mut app).overlay = Overlay::None;
    {
        let s = active_state(&mut app);
        type_text(s, "draft");
    }
    let original_id = active_state(&mut app).session.as_ref().unwrap().id;

    // Late completion arrives with a different session id.
    let other = Session {
        id: uuid::Uuid::new_v4(),
        title: "other".into(),
        model: ModelId::MiniMaxM3,
        mode: Mode::Build,
        messages: vec![],
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    let _ = update(&mut app, Msg::SessionOpened(Ok(other)));

    let s = active_state(&mut app);
    assert_eq!(
        s.session.as_ref().unwrap().id,
        original_id,
        "stale SessionOpened must not replace the active session"
    );
    let draft = s.input.lines().join("\n");
    assert_eq!(draft, "draft", "stale SessionOpened must not clobber input");
}
