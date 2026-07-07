//! Unit tests for the `/model` and `/session` slash commands, exercising
//! `update` end-to-end through its public API.
//!
//! Covers the three layers the slash command touches:
//! - the parser inside `on_session_submit` (driven by `Enter` in the input),
//! - the `Cmd` returned to the runtime (the side effect to dispatch),
//! - the resulting state mutation (overlay state, model state).

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use tui_textarea::TextArea;

use mewcode_client::runtime::model::{App, Cmd, Msg, Overlay, Screen, SessionState};
use mewcode_client::runtime::update;
use mewcode_client::runtime::view::render;
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
    assert_eq!(s.model_picker.cursor, 0);
    assert!(
        s.input.lines().join("\n").is_empty(),
        "input should be cleared after dispatch"
    );
}

#[test]
fn slash_model_reuses_cached_registry() {
    let mut app = test_app();
    seed_active_session(active_state(&mut app));
    active_state(&mut app).model_picker.models = Some(vec![]); // cache the registry

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
    assert_eq!(s.session_list.cursor, 0);
}

#[test]
fn slash_session_always_refetches_to_pick_up_new_sessions() {
    // The current implementation always fetches (the empty cache is
    // indistinguishable from "never fetched"). What we verify here is
    // the visible contract: the overlay still opens, the cursor resets,
    // and a fetch fires.
    let mut app = test_app();
    let id = uuid::Uuid::new_v4();
    active_state(&mut app).session_list.summaries = vec![mewcode_client::net::SessionSummary {
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
    assert_eq!(s.session_list.cursor, 0);
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
        Cmd::PatchSession { id, patch, .. } => {
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
    active_state(&mut app).model_picker.models = Some(vec![mewcode_client::net::ModelEntry {
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
        Cmd::PatchSession { id, patch, .. } => {
            assert_eq!(id, active_state(&mut app).session.as_ref().unwrap().id);
            assert_eq!(patch.model, Some(ModelId::MiniMaxM3));
        }
        other => panic!("expected Cmd::PatchSession, got {other:?}"),
    }
}

#[test]
fn model_picker_before_session_sets_pending_model() {
    let mut app = test_app();
    active_state(&mut app).model_picker.models = Some(vec![
        mewcode_client::net::ModelEntry {
            id: "minimax-m3".into(),
            display_name: "MiniMax M3".into(),
            kind: mewcode_protocol::ModelKind::AnthropicMessages,
        },
        mewcode_client::net::ModelEntry {
            id: "minimax-m2.5".into(),
            display_name: "MiniMax M2.5".into(),
            kind: mewcode_protocol::ModelKind::AnthropicMessages,
        },
    ]);

    {
        let s = active_state(&mut app);
        type_text(s, "/model");
    }
    let _ = update(&mut app, press_enter());
    let _ = update(&mut app, press_arrow(KeyCode::Down));
    let cmd = update(&mut app, press_enter());

    let s = active_state(&mut app);
    assert!(matches!(cmd, Cmd::None));
    assert_eq!(s.overlay, Overlay::None);
    assert_eq!(s.pending_model, Some(ModelId::MiniMaxM25));
    assert!(
        app.toast.is_none(),
        "choosing a default model should not toast"
    );
}

#[test]
fn first_session_create_uses_pending_model() {
    let mut app = test_app();
    active_state(&mut app).pending_model = Some(ModelId::MiniMaxM25);
    {
        let s = active_state(&mut app);
        type_text(s, "hello");
    }

    match update(&mut app, press_enter()) {
        Cmd::CreateSession(req) => {
            assert_eq!(req.title, "hello");
            assert_eq!(req.model, Some(ModelId::MiniMaxM25));
        }
        other => panic!("expected CreateSession, got {other:?}"),
    }
}

// --- /session list key nav -----------------------------------------------

#[test]
fn session_list_enter_emits_open_session_cmd() {
    let mut app = test_app();
    let id = uuid::Uuid::new_v4();
    active_state(&mut app).session_list.summaries = vec![mewcode_client::net::SessionSummary {
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
    active_state(&mut app).session_list.summaries = vec![mewcode_client::net::SessionSummary {
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
    assert!(s.model_picker.models.is_none());
    assert!(s.session_list.summaries.is_empty());
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
    // Simulates a late `Msg::SessionPatched(Ok(...), false)` arriving
    // after the user has already Esc'd out of the rename overlay and
    // started typing a chat message. The handler must not clobber the
    // draft.
    use mewcode_client::net::Session;
    let mut app = test_app();
    seed_active_session(active_state(&mut app));

    // User has typed a draft chat message.
    {
        let s = active_state(&mut app);
        type_text(s, "hi there");
    }

    // A late model-picker PATCH result lands — overlay is None, input
    // is "hi there". `from_rename: false` signals this is not the
    // rename flow, so the composer must not be cleared.
    let new_session = Session {
        id: uuid::Uuid::new_v4(),
        title: "renamed".into(),
        model: ModelId::MiniMaxM3,
        mode: Mode::Build,
        messages: vec![],
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    let _ = update(
        &mut app,
        Msg::SessionPatched(Ok(new_session.clone()), false),
    );

    let s = active_state(&mut app);
    // The session adopts the patch (it's still the active session), but
    // the composer draft is preserved.
    assert_eq!(s.session.as_ref().unwrap().title, "renamed");
    let draft = s.input.lines().join("\n");
    assert_eq!(draft, "hi there", "stale PATCH must not clear the draft");
}

#[test]
fn session_patched_from_rename_clears_draft_even_if_overlay_already_closed() {
    // A successful rename PATCH must always clear the title draft,
    // even if the user Esc'd out of the rename screen while the
    // request was in flight.
    use mewcode_client::net::Session;
    let mut app = test_app();
    seed_active_session(active_state(&mut app));

    // User hit /session rename, the overlay is still open and the
    // input is seeded with the current title.
    {
        let s = active_state(&mut app);
        type_text(s, "/session rename");
    }
    let _ = update(&mut app, press_enter());
    assert_eq!(active_state(&mut app).overlay, Overlay::RenameSession);
    assert!(!active_state(&mut app).input.lines().is_empty());

    // User Esc's out (this is also the moment we need to fix: Esc
    // already cleared the draft in the previous fix).
    let _ = update(&mut app, press_esc());
    assert_eq!(active_state(&mut app).overlay, Overlay::None);

    // User starts typing a chat message.
    {
        let s = active_state(&mut app);
        type_text(s, "hi");
    }

    // The late rename PATCH returns successfully. With `from_rename:
    // true`, the draft from the previous turn is cleared so the
    // in-flight PATCH still wins — the title got renamed, so the
    // rename draft is no longer the user's intent.
    let new_session = Session {
        id: active_state(&mut app).session.as_ref().unwrap().id,
        title: "renamed".into(),
        model: ModelId::MiniMaxM3,
        mode: Mode::Build,
        messages: vec![],
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    let _ = update(&mut app, Msg::SessionPatched(Ok(new_session), true));

    let s = active_state(&mut app);
    assert_eq!(s.session.as_ref().unwrap().title, "renamed");
    // Rename PATCH clears the composer so the rename is the final word.
    let draft = s.input.lines().join("\n");
    assert!(draft.is_empty(), "rename PATCH must clear the draft");
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

// --- slash-command picker ------------------------------------------------

fn type_char(c: char) -> Msg {
    Msg::Key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE))
}

fn press_arrow(code: KeyCode) -> Msg {
    Msg::Key(KeyEvent::new(code, KeyModifiers::NONE))
}

#[test]
fn typing_slash_opens_picker() {
    let mut app = test_app();
    let _ = update(&mut app, type_char('/'));
    let s = active_state(&mut app);
    assert_eq!(s.overlay, Overlay::SlashPicker);
    assert_eq!(s.input.lines().join("\n"), "/");
    assert_eq!(s.slash_cursor, 0, "bare / should highlight the first row");
}

#[test]
fn picker_filters_as_user_types() {
    let mut app = test_app();
    for c in "/m".chars() {
        let _ = update(&mut app, type_char(c));
    }
    let s = active_state(&mut app);
    assert_eq!(s.overlay, Overlay::SlashPicker);
    // `/model` is the only command whose trimmed form starts with "m".
    assert_eq!(s.slash_cursor, 0);
}

#[test]
fn picker_closes_when_prefix_drops_slash() {
    let mut app = test_app();
    let _ = update(&mut app, type_char('/'));
    assert_eq!(active_state(&mut app).overlay, Overlay::SlashPicker);
    // Backspace away the slash — the picker should close and the input
    // should be empty.
    let _ = update(
        &mut app,
        Msg::Key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE)),
    );
    let s = active_state(&mut app);
    assert_eq!(s.overlay, Overlay::None);
    assert_eq!(s.input.lines().join("\n"), "");
}

#[test]
fn picker_down_arrow_moves_cursor() {
    let mut app = test_app();
    let _ = update(&mut app, type_char('/'));
    let _ = update(&mut app, press_arrow(KeyCode::Down));
    let _ = update(&mut app, press_arrow(KeyCode::Down));
    assert_eq!(active_state(&mut app).slash_cursor, 2);
    let _ = update(&mut app, press_arrow(KeyCode::Up));
    assert_eq!(active_state(&mut app).slash_cursor, 1);
}

#[test]
fn picker_enter_dispatches_highlighted_command() {
    let mut app = test_app();
    seed_active_session(active_state(&mut app));
    let _ = update(&mut app, type_char('/'));
    // /model is the first row — pressing Enter opens the model picker.
    let cmd = update(&mut app, press_enter());
    let s = active_state(&mut app);
    assert_eq!(
        s.overlay,
        Overlay::ModelPicker,
        "Enter should dispatch /model"
    );
    assert!(matches!(cmd, Cmd::FetchModels));
    // The composer is cleared by the slash submit path.
    assert!(s.input.lines().join("\n").is_empty());
}

#[test]
fn picker_enter_uses_highlighted_row() {
    let mut app = test_app();
    let _ = update(&mut app, type_char('/'));
    // Navigate to /tools (index 4 in SLASH_COMMANDS).
    for _ in 0..4 {
        let _ = update(&mut app, press_arrow(KeyCode::Down));
    }
    let _ = update(&mut app, press_enter());
    assert_eq!(active_state(&mut app).overlay, Overlay::Tools);
}

#[test]
fn picker_esc_clears_composer_and_closes() {
    let mut app = test_app();
    let _ = update(&mut app, type_char('/'));
    assert_eq!(active_state(&mut app).overlay, Overlay::SlashPicker);
    let _ = update(&mut app, press_esc());
    let s = active_state(&mut app);
    assert_eq!(s.overlay, Overlay::None);
    assert!(s.input.lines().join("\n").is_empty());
}

// --- model / session picker scroll -------------------------------------

fn seed_models(n: usize) -> Vec<mewcode_client::net::ModelEntry> {
    (0..n)
        .map(|i| mewcode_client::net::ModelEntry {
            id: format!("id-{i}"),
            display_name: format!("Model {i}"),
            kind: mewcode_protocol::ModelKind::OpenAiChatCompletions,
        })
        .collect()
}

fn draw(app: &mut App, width: u16, height: u16) -> String {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal.draw(|frame| render(frame, app)).unwrap();
    terminal.backend().to_string()
}

fn open_model_picker(app: &mut App) {
    // `/model` (typed, then Enter) opens the model picker overlay.
    {
        let s = active_state(app);
        type_text(s, "/model");
    }
    let _ = update(app, press_enter());
}

#[test]
fn model_picker_rows_fit_on_one_visual_line() {
    // The picker's cursor is one per model, so the view must
    // guarantee exactly one visual line per model — otherwise the
    // highlight drifts by the wrap count on every cursor move. We
    // assert on the rendered `Line`s: each entry's `Line` must contain
    // a single span, and that span's text must fit the supplied width
    // so `Paragraph` never wraps it.
    use mewcode_client::runtime::view::model_picker_lines;
    let mut app = test_app();
    let s = active_state(&mut app);
    s.session = Some(session());
    s.model_picker.models = Some(seed_models(5));
    s.model_picker.cursor = 0;
    s.model_picker.scroll = 0;

    let max_width = 30; // tight enough to force truncation for long ids
    let lines = model_picker_lines(s, max_width);
    assert_eq!(lines.len(), 5);
    for (i, line) in lines.iter().enumerate() {
        let text: String = line.spans.iter().map(|sp| sp.content.as_ref()).collect();
        assert!(
            !text.contains('\n'),
            "row {i} should not contain an embedded newline: {text:?}"
        );
        // The cursor is on row 0, so that row gets the highlight span.
        // For the others, the span is a single one carrying the row text.
        assert_eq!(line.spans.len(), 1, "row {i} should be a single span");
        let span = &line.spans[0];
        assert!(
            span.content.chars().count() <= max_width,
            "row {i} text {:?} ({} chars) exceeds width {max_width}",
            span.content,
            span.content.chars().count()
        );
    }
}

#[test]
fn model_picker_last_row_is_visible_in_small_terminal() {
    // Regression for the real TUI bug: the footer must not replace the
    // last visible model row. In this terminal size the model overlay has
    // room for 13 model rows + 1 footer row. Moving to 15/15 must scroll
    // the window and still render Model 14 above the footer.
    let mut app = test_app();
    open_model_picker(&mut app);
    {
        let s = active_state(&mut app);
        s.model_picker.models = Some(seed_models(15));
    }
    // First draw reports the picker viewport into the model.
    let _ = draw(&mut app, 100, 28);
    for _ in 0..14 {
        let _ = update(&mut app, press_arrow(KeyCode::Down));
    }
    let buf = draw(&mut app, 100, 28);

    assert_eq!(active_state(&mut app).model_picker.cursor, 14);
    assert!(
        buf.contains("Model 14"),
        "last cursor row should be visible, not replaced by the footer:\n{buf}"
    );
    assert!(buf.contains("15/15"), "footer should still render:\n{buf}");
}
