//! Unit tests for the `/model` and `/session` slash commands, exercising
//! `update` end-to-end through its public API.
//!
//! Covers the three layers the slash command touches:
//! - the parser inside `on_session_submit` (driven by `Enter` in the input),
//! - the `Cmd` returned to the runtime (the side effect to dispatch),
//! - the resulting state mutation (overlay state, model state).

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::Terminal;
use ratatui::backend::{Backend, TestBackend};
use tui_textarea::{CursorMove, TextArea};
use unicode_width::UnicodeWidthStr;

use mewcode_client::runtime::model::{App, Cmd, ConnectStep, Msg, Overlay, Screen, SessionState};
use mewcode_client::runtime::update;
use mewcode_client::runtime::view::render;
use mewcode_protocol::ProviderId;
use mewcode_protocol::{MessagePart, Mode, ModelId, ModelRef};

fn test_app() -> App {
    App::new()
}

fn session() -> mewcode_client::net::Session {
    mewcode_client::net::Session {
        id: uuid::Uuid::new_v4(),
        title: "demo".into(),
        model: ModelId::Glm51.into(),
        model_kind: None,
        model_context_length: None,
        mode: Mode::Build,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
        messages: vec![],
        compaction_summary: None,
        compacted_up_to: None,
        todos: vec![],
    }
}

fn type_text(s: &mut SessionState, text: &str) {
    // Replace the input with the given text via the public insert API.
    s.composer = TextArea::new(vec![text.to_string()]);
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
        matches!(cmd, Cmd::FetchModels(_)),
        "expected FetchModels, got {cmd:?}"
    );
    let s = active_state(&mut app);
    assert_eq!(s.overlay, Overlay::ModelPicker);
    assert_eq!(s.model_picker.picker.cursor, 0);
    assert!(
        s.composer.lines().join("\n").is_empty(),
        "input should be cleared after dispatch"
    );
}

#[test]
fn slash_model_refreshes_cached_registry() {
    let mut app = test_app();
    seed_active_session(active_state(&mut app));
    active_state(&mut app).model_picker.models = Some(vec![]);

    {
        let s = active_state(&mut app);
        type_text(s, "/model");
    }
    let cmd = update(&mut app, press_enter());

    assert!(
        matches!(cmd, Cmd::FetchModels(_)),
        "expected refresh, got {cmd:?}"
    );
    let s = active_state(&mut app);
    assert_eq!(s.overlay, Overlay::ModelPicker);
    assert!(
        s.model_picker.models.is_none(),
        "stale models must not remain selectable during refresh"
    );
}

#[test]
fn model_picker_ignores_out_of_order_refresh_results() {
    let mut app = test_app();
    seed_active_session(active_state(&mut app));

    for attempt in 0..2 {
        let s = active_state(&mut app);
        type_text(s, "/model");
        let _ = update(&mut app, press_enter());
        if attempt == 0 {
            let _ = update(&mut app, press_esc());
        }
    }

    let stale = vec![mewcode_client::net::ModelEntry {
        id: "stale/model".into(),
        display_name: "Stale".into(),
        provider: ProviderId::OpenRouter,
        kind: mewcode_protocol::ModelKind::OpenRouter,
        context_length: None,
        is_free: false,
    }];
    let current = vec![mewcode_client::net::ModelEntry {
        id: "current/model".into(),
        display_name: "Current".into(),
        provider: ProviderId::OpenRouter,
        kind: mewcode_protocol::ModelKind::OpenRouter,
        context_length: None,
        is_free: false,
    }];

    let _ = update(&mut app, Msg::ModelsFetched(Ok(stale), 1));
    assert!(active_state(&mut app).model_picker.models.is_none());
    let _ = update(&mut app, Msg::ModelsFetched(Ok(current), 2));
    assert_eq!(
        active_state(&mut app).model_picker.models.as_ref().unwrap()[0].id,
        "current/model"
    );
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
    assert_eq!(s.session_list.picker.cursor, 0);
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
        model: ModelId::Glm51.into(),
        model_kind: None,
        model_context_length: None,
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
    assert_eq!(s.session_list.picker.cursor, 0);
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
        s.composer.lines().join("\n"),
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

#[test]
fn slash_mode_plan_emits_mode_patch() {
    let mut app = test_app();
    seed_active_session(active_state(&mut app));

    {
        let s = active_state(&mut app);
        type_text(s, "/mode plan");
    }
    let cmd = update(&mut app, press_enter());

    match cmd {
        Cmd::PatchSession { id, patch, .. } => {
            assert_eq!(id, active_state(&mut app).session.as_ref().unwrap().id);
            assert_eq!(patch.mode, Some(Mode::Plan));
            assert!(patch.title.is_none());
            assert!(patch.model.is_none());
        }
        other => panic!("expected Cmd::PatchSession, got {other:?}"),
    }
}

#[test]
fn tab_toggles_active_session_mode() {
    let mut app = test_app();
    seed_active_session(active_state(&mut app));

    let cmd = update(
        &mut app,
        Msg::Key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE)),
    );

    match cmd {
        Cmd::PatchSession { patch, .. } => assert_eq!(patch.mode, Some(Mode::Plan)),
        other => panic!("expected Cmd::PatchSession, got {other:?}"),
    }
}

#[test]
fn mode_patch_changes_next_chat_mode() {
    let mut app = test_app();
    seed_active_session(active_state(&mut app));
    let mut patched = active_state(&mut app).session.as_ref().unwrap().clone();
    patched.mode = Mode::Plan;
    let _ = update(&mut app, Msg::SessionPatched(Ok(patched), false));

    {
        let s = active_state(&mut app);
        type_text(s, "analyze only");
    }
    let cmd = update(&mut app, press_enter());

    match cmd {
        Cmd::StartChat(req) => assert_eq!(req.mode, Mode::Plan),
        other => panic!("expected Cmd::StartChat, got {other:?}"),
    }
}

#[test]
fn slash_mode_before_session_sets_pending_mode() {
    let mut app = test_app();
    {
        let s = active_state(&mut app);
        type_text(s, "/mode plan");
    }
    let cmd = update(&mut app, press_enter());

    assert!(matches!(cmd, Cmd::None));
    assert_eq!(
        active_state(&mut app).creation.pending_mode,
        Some(Mode::Plan)
    );
    assert!(app.toast.is_none());
}

#[test]
fn tab_before_session_toggles_pending_mode() {
    let mut app = test_app();

    let cmd = update(
        &mut app,
        Msg::Key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE)),
    );

    assert!(matches!(cmd, Cmd::None));
    assert_eq!(
        active_state(&mut app).creation.pending_mode,
        Some(Mode::Plan)
    );
}

#[test]
fn model_picker_enter_emits_patch_session_cmd() {
    let mut app = test_app();
    seed_active_session(active_state(&mut app));

    // Open the picker, deliver the refreshed catalog, and press Enter.
    {
        let s = active_state(&mut app);
        type_text(s, "/model");
    }
    let _ = update(&mut app, press_enter());
    active_state(&mut app).model_picker.models = Some(vec![mewcode_client::net::ModelEntry {
        id: "minimax-m3".into(),
        display_name: "MiniMax M3".into(),
        provider: mewcode_protocol::ProviderId::OpenCodeGo,
        kind: mewcode_protocol::ModelKind::OpenCodeGo,
        context_length: None,
        is_free: false,
    }]);
    let cmd = update(&mut app, press_enter());

    match cmd {
        Cmd::PatchSession { id, patch, .. } => {
            assert_eq!(id, active_state(&mut app).session.as_ref().unwrap().id);
            assert_eq!(patch.model, Some(ModelId::MiniMaxM3.into()));
        }
        other => panic!("expected Cmd::PatchSession, got {other:?}"),
    }
}

#[test]
fn openrouter_picker_selection_preserves_id_and_context_snapshot() {
    let mut app = test_app();
    seed_active_session(active_state(&mut app));
    active_state(&mut app).model_picker.models = Some(vec![mewcode_client::net::ModelEntry {
        id: "Vendor/Exact:free".into(),
        display_name: "Vendor Exact".into(),
        provider: ProviderId::OpenRouter,
        kind: mewcode_protocol::ModelKind::OpenRouter,
        context_length: Some(262_144),
        is_free: true,
    }]);
    active_state(&mut app).overlay = Overlay::ModelPicker;

    let cmd = update(&mut app, press_enter());
    match cmd {
        Cmd::PatchSession { patch, .. } => {
            assert_eq!(
                patch.model,
                Some(ModelRef::openrouter("Vendor/Exact:free").unwrap())
            );
            assert_eq!(patch.model_context_length, Some(262_144));
        }
        other => panic!("expected Cmd::PatchSession, got {other:?}"),
    }
}

#[test]
fn dynamic_model_ids_are_sanitized_when_rendered() {
    let mut app = test_app();
    let mut active = session();
    active.model = ModelRef::openrouter("safe\u{1b}[31m\u{202e}exe").unwrap();
    active_state(&mut app).session = Some(active);

    let mut terminal = Terminal::new(TestBackend::new(100, 24)).unwrap();
    terminal.draw(|frame| render(frame, &mut app)).unwrap();
    let rendered = terminal.backend().to_string();

    assert!(!rendered.contains('\u{1b}'));
    assert!(!rendered.contains('\u{202e}'));
    assert!(rendered.contains("safe [31m exe"));
}

#[test]
fn dynamic_openai_picker_selection_preserves_exact_identity() {
    let mut app = test_app();
    seed_active_session(active_state(&mut app));
    active_state(&mut app).model_picker.models = Some(vec![mewcode_client::net::ModelEntry {
        id: "gpt-future/model:v1".into(),
        display_name: "Future GPT".into(),
        provider: ProviderId::OpenAi,
        kind: mewcode_protocol::ModelKind::OpenAi,
        context_length: None,
        is_free: false,
    }]);
    active_state(&mut app).overlay = Overlay::ModelPicker;

    let cmd = update(&mut app, press_enter());
    match cmd {
        Cmd::PatchSession { patch, .. } => {
            assert_eq!(
                patch.model,
                Some(ModelRef::openai("gpt-future/model:v1").unwrap())
            );
            assert_eq!(patch.model_context_length, None);
        }
        other => panic!("expected Cmd::PatchSession, got {other:?}"),
    }
}

#[test]
fn model_picker_before_session_sets_pending_model() {
    let mut app = test_app();

    {
        let s = active_state(&mut app);
        type_text(s, "/model");
    }
    let _ = update(&mut app, press_enter());
    active_state(&mut app).model_picker.models = Some(vec![
        mewcode_client::net::ModelEntry {
            id: "minimax-m3".into(),
            display_name: "MiniMax M3".into(),
            provider: mewcode_protocol::ProviderId::OpenCodeGo,
            kind: mewcode_protocol::ModelKind::AnthropicMessages,
            context_length: None,
            is_free: false,
        },
        mewcode_client::net::ModelEntry {
            id: "minimax-m2.5".into(),
            display_name: "MiniMax M2.5".into(),
            provider: mewcode_protocol::ProviderId::OpenCodeGo,
            kind: mewcode_protocol::ModelKind::AnthropicMessages,
            context_length: None,
            is_free: false,
        },
    ]);

    let _ = update(&mut app, press_arrow(KeyCode::Down));
    let cmd = update(&mut app, press_enter());

    let s = active_state(&mut app);
    assert!(matches!(cmd, Cmd::None));
    assert_eq!(s.overlay, Overlay::None);
    assert_eq!(s.creation.pending_model, Some(ModelId::MiniMaxM25.into()));
    assert!(
        app.toast.is_none(),
        "choosing a default model should not toast"
    );
}

#[test]
fn first_session_create_uses_pending_model() {
    let mut app = test_app();
    active_state(&mut app).creation.pending_model = Some(ModelId::MiniMaxM25.into());
    {
        let s = active_state(&mut app);
        type_text(s, "hello");
    }

    match update(&mut app, press_enter()) {
        Cmd::CreateSession(req) => {
            assert_eq!(req.title, "hello");
            assert_eq!(req.model, Some(ModelId::MiniMaxM25.into()));
        }
        other => panic!("expected CreateSession, got {other:?}"),
    }
}

#[test]
fn first_session_create_uses_pending_mode() {
    let mut app = test_app();
    active_state(&mut app).creation.pending_mode = Some(Mode::Plan);
    {
        let s = active_state(&mut app);
        type_text(s, "hello");
    }

    match update(&mut app, press_enter()) {
        Cmd::CreateSession(req) => {
            assert_eq!(req.title, "hello");
            assert_eq!(req.mode, Some(Mode::Plan));
        }
        other => panic!("expected CreateSession, got {other:?}"),
    }
}

#[test]
fn session_list_enter_emits_open_session_cmd() {
    let mut app = test_app();
    let id = uuid::Uuid::new_v4();
    active_state(&mut app).session_list.summaries = vec![mewcode_client::net::SessionSummary {
        id,
        title: "first".into(),
        model: ModelId::Glm51.into(),
        model_kind: None,
        model_context_length: None,
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
        model: ModelId::Glm51.into(),
        model_kind: None,
        model_context_length: None,
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

#[test]
fn unknown_slash_command_is_sent_as_chat_text() {
    let mut app = test_app();
    {
        let s = active_state(&mut app);
        seed_active_session(s);
        type_text(s, "/nonsense");
    }
    let cmd = update(&mut app, press_enter());

    assert!(matches!(cmd, Cmd::StartChat(_)), "got {cmd:?}");
    let s = active_state(&mut app);
    assert_eq!(s.overlay, Overlay::None);
    assert!(s.model_picker.models.is_none());
    assert!(s.session_list.summaries.is_empty());
    assert!(app.toast.is_none());
}

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
fn slash_theme_opens_theme_overlay() {
    let mut app = test_app();
    {
        let s = active_state(&mut app);
        type_text(s, "/theme");
    }
    let cmd = update(&mut app, press_enter());

    assert!(matches!(cmd, Cmd::None));
    assert_eq!(active_state(&mut app).overlay, Overlay::Theme);
}

#[test]
fn slash_picker_lists_theme_command() {
    let mut app = test_app();
    {
        let s = active_state(&mut app);
        type_text(s, "/");
    }
    update(
        &mut app,
        Msg::Key(KeyEvent::new(KeyCode::Char('/'), KeyModifiers::NONE)),
    );

    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
    terminal.draw(|frame| render(frame, &mut app)).unwrap();
    let buf = terminal.backend().to_string();

    assert!(
        buf.contains("/theme"),
        "slash picker should list /theme:\n{buf}"
    );
}

#[test]
fn connect_provider_click_selects_and_confirms_row() {
    let mut app = test_app();
    {
        let s = active_state(&mut app);
        s.overlay = Overlay::ConnectProvider;
        s.connect_provider.step = ConnectStep::PickProvider;
    }
    render_picker(&mut app, 80, 24);
    let rect = active_state(&mut app)
        .connect_provider
        .picker
        .rect
        .expect("render should store connect picker geometry");

    let cmd = update(
        &mut app,
        picker_mouse(MouseEventKind::Down(MouseButton::Left), rect.x, rect.y + 1),
    );

    let s = active_state(&mut app);
    assert!(matches!(cmd, Cmd::None));
    assert_eq!(
        s.connect_provider.selected_provider,
        Some(ProviderId::OpenAi)
    );
    assert_eq!(s.connect_provider.step, ConnectStep::EnterKey);
}

#[test]
fn connect_provider_wheel_uses_shared_picker_cursor() {
    let mut app = test_app();
    {
        let s = active_state(&mut app);
        s.overlay = Overlay::ConnectProvider;
        s.connect_provider.step = ConnectStep::PickProvider;
    }
    render_picker(&mut app, 80, 24);
    let rect = active_state(&mut app)
        .connect_provider
        .picker
        .rect
        .expect("render should store connect picker geometry");

    let _ = update(
        &mut app,
        picker_mouse(MouseEventKind::ScrollDown, rect.x, rect.y),
    );

    let s = active_state(&mut app);
    assert_eq!(s.connect_provider.picker.cursor, 1);
    assert_eq!(s.connect_provider.step, ConnectStep::PickProvider);
}

#[test]
fn connect_overlay_parks_cursor_in_api_key_field() {
    let mut app = test_app();
    {
        let s = active_state(&mut app);
        s.overlay = Overlay::ConnectProvider;
        s.connect_provider.step = ConnectStep::EnterKey;
        s.connect_provider.selected_provider = Some(ProviderId::OpenCodeGo);
        s.connect_provider.key_input.insert_str("abc");
    }

    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
    terminal.draw(|frame| render(frame, &mut app)).unwrap();
    let buf = terminal.backend().to_string();
    assert!(!buf.contains("abc"), "API key must not be rendered: {buf}");
    let row = buf
        .lines()
        .enumerate()
        .find_map(|(row, line)| line.contains("•••│").then_some(row))
        .expect("masked API key cursor should render in overlay");

    assert_eq!(
        terminal.backend_mut().get_cursor_position().unwrap().y,
        row as u16
    );
}

#[test]
fn connect_overlay_keys_go_to_api_key_not_composer() {
    let mut app = test_app();
    {
        let s = active_state(&mut app);
        s.overlay = Overlay::ConnectProvider;
        s.connect_provider.step = ConnectStep::EnterKey;
        s.connect_provider.selected_provider = Some(ProviderId::OpenCodeGo);
    }

    update(
        &mut app,
        Msg::Key(KeyEvent::new(KeyCode::Char('h'), KeyModifiers::NONE)),
    );

    let s = active_state(&mut app);
    assert_eq!(s.connect_provider.key_input.lines(), &vec!["h".to_string()]);
    assert_eq!(s.composer.lines(), &vec!["".to_string()]);
}

#[test]
fn connect_command_opens_with_empty_composer() {
    let mut app = test_app();
    type_text(active_state(&mut app), "/connect");

    update(&mut app, press_enter());

    let s = active_state(&mut app);
    assert_eq!(s.overlay, Overlay::ConnectProvider);
    assert_eq!(s.composer.lines(), &vec!["".to_string()]);
}

#[test]
fn connect_overlay_key_handler_promotes_stray_composer_text() {
    let mut app = test_app();
    {
        let s = active_state(&mut app);
        type_text(s, "abc");
        s.overlay = Overlay::ConnectProvider;
        s.connect_provider.step = ConnectStep::EnterKey;
        s.connect_provider.selected_provider = Some(ProviderId::OpenCodeGo);
    }

    update(
        &mut app,
        Msg::Key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::NONE)),
    );

    let s = active_state(&mut app);
    assert_eq!(
        s.connect_provider.key_input.lines(),
        &vec!["abcd".to_string()]
    );
    assert_eq!(s.composer.lines(), &vec!["".to_string()]);
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

#[test]
fn esc_on_rename_clears_composer_draft() {
    let mut app = test_app();
    seed_active_session(active_state(&mut app));
    {
        let s = active_state(&mut app);
        type_text(s, "/session rename");
    }
    let _ = update(&mut app, press_enter());
    // The rename overlay seeds `s.composer` with the current title.
    assert_eq!(active_state(&mut app).overlay, Overlay::RenameSession);
    assert_eq!(active_state(&mut app).composer.lines().join("\n"), "demo");

    // Type some new characters into the composer to make it a draft.
    {
        let s = active_state(&mut app);
        type_text(s, "EDIT");
    }

    // Esc should close the overlay AND clear the draft.
    let _ = update(&mut app, press_esc());
    let s = active_state(&mut app);
    assert_eq!(s.overlay, Overlay::None);
    let draft = s.composer.lines().join("\n");
    assert!(
        draft.trim().is_empty(),
        "Esc should discard the rename draft, not leave it in the composer (got {draft:?})"
    );
}

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
    assert!(s.creation.creating);
    assert_eq!(s.overlay, Overlay::None);
}

/// `/session new` with no title does not create a session up front — it
/// drops back to the entry view (no active session, no pending create), the
/// same view the app launches into. A session is only actually created once
/// the user's first message in that empty view derives a title from it.
#[test]
fn slash_session_new_without_title_returns_to_entry_view() {
    let mut app = test_app();
    {
        let s = active_state(&mut app);
        seed_active_session(s);
        assert!(s.session.is_some());
        type_text(s, "/session new");
    }
    let cmd = update(&mut app, press_enter());

    assert!(matches!(cmd, Cmd::None));
    let s = active_state(&mut app);
    assert_eq!(s.overlay, Overlay::None);
    assert!(
        !s.creation.creating,
        "no session should be created without a title"
    );
    assert!(
        s.session.is_none(),
        "bare /session new must clear the active session, landing back on the entry view"
    );
}

/// The model/mode picked while on the entry view (before any session exists)
/// carry over across a bare `/session new`, so switching model right after
/// isn't lost.
#[test]
fn slash_session_new_without_title_carries_over_active_session_model_and_mode() {
    let mut app = test_app();
    {
        let s = active_state(&mut app);
        seed_active_session(s);
        s.session.as_mut().unwrap().model = ModelId::MiMoV25Pro.into();
        s.session.as_mut().unwrap().mode = Mode::Plan;
        type_text(s, "/session new");
    }
    update(&mut app, press_enter());

    let s = active_state(&mut app);
    assert_eq!(s.creation.pending_model, Some(ModelId::MiMoV25Pro.into()));
    assert_eq!(s.creation.pending_mode, Some(Mode::Plan));
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
        model: ModelId::MiniMaxM3.into(),
        model_kind: None,
        model_context_length: None,
        mode: Mode::Build,
        messages: vec![],
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
        compaction_summary: None,
        compacted_up_to: None,
        todos: vec![],
    };
    let _ = update(
        &mut app,
        Msg::SessionPatched(Ok(new_session.clone()), false),
    );

    let s = active_state(&mut app);
    // The session adopts the patch (it's still the active session), but
    // the composer draft is preserved.
    assert_eq!(s.session.as_ref().unwrap().title, "renamed");
    let draft = s.composer.lines().join("\n");
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
    assert_eq!(active_state(&mut app).composer.lines().join("\n"), "demo");

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
        model: ModelId::MiniMaxM3.into(),
        model_kind: None,
        model_context_length: None,
        mode: Mode::Build,
        messages: vec![],
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
        compaction_summary: None,
        compacted_up_to: None,
        todos: vec![],
    };
    let _ = update(&mut app, Msg::SessionPatched(Ok(new_session), true));

    let s = active_state(&mut app);
    assert_eq!(s.session.as_ref().unwrap().title, "renamed");
    // Rename PATCH clears the composer so the rename is the final word.
    let draft = s.composer.lines().join("\n");
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
        model: ModelId::MiniMaxM3.into(),
        model_kind: None,
        model_context_length: None,
        mode: Mode::Build,
        messages: vec![],
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
        compaction_summary: None,
        compacted_up_to: None,
        todos: vec![],
    };
    let _ = update(&mut app, Msg::SessionOpened(Ok(other)));

    let s = active_state(&mut app);
    assert_eq!(
        s.session.as_ref().unwrap().id,
        original_id,
        "stale SessionOpened must not replace the active session"
    );
    let draft = s.composer.lines().join("\n");
    assert_eq!(draft, "draft", "stale SessionOpened must not clobber input");
}

fn type_char(c: char) -> Msg {
    Msg::Key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE))
}

fn press_arrow(code: KeyCode) -> Msg {
    Msg::Key(KeyEvent::new(code, KeyModifiers::NONE))
}

fn picker_mouse(kind: MouseEventKind, column: u16, row: u16) -> Msg {
    Msg::Mouse(MouseEvent {
        kind,
        column,
        row,
        modifiers: KeyModifiers::NONE,
    })
}

fn render_picker(app: &mut App, width: u16, height: u16) {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal.draw(|frame| render(frame, app)).unwrap();
}

#[test]
fn typing_slash_opens_picker() {
    let mut app = test_app();
    let _ = update(&mut app, type_char('/'));
    let s = active_state(&mut app);
    assert_eq!(s.overlay, Overlay::SlashPicker);
    assert_eq!(s.composer.lines().join("\n"), "/");
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
    assert_eq!(s.composer.lines().join("\n"), "");
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
fn picker_mouse_wheel_moves_cursor_without_scrolling_transcript() {
    let mut app = test_app();
    let _ = update(&mut app, type_char('/'));
    render_picker(&mut app, 80, 24);
    active_state(&mut app).scroll = 5;
    active_state(&mut app).max_scroll = 10;
    let rect = active_state(&mut app)
        .slash_picker_geometry
        .expect("render should store slash picker geometry")
        .0;

    let _ = update(
        &mut app,
        picker_mouse(MouseEventKind::ScrollDown, rect.x, rect.y),
    );

    let s = active_state(&mut app);
    assert_eq!(s.slash_cursor, 1);
    assert_eq!(s.scroll, 5, "picker wheel must not scroll the transcript");
}

#[test]
fn picker_mouse_click_activates_visible_row() {
    let mut app = test_app();
    let _ = update(&mut app, type_char('/'));
    render_picker(&mut app, 80, 24);
    let rect = active_state(&mut app)
        .slash_picker_geometry
        .expect("render should store slash picker geometry")
        .0;

    let cmd = update(
        &mut app,
        picker_mouse(MouseEventKind::Down(MouseButton::Left), rect.x, rect.y + 4),
    );

    assert!(matches!(cmd, Cmd::None));
    assert_eq!(active_state(&mut app).overlay, Overlay::Tools);
}

#[test]
fn model_picker_click_activates_openrouter_model_row() {
    let mut app = test_app();
    seed_active_session(active_state(&mut app));
    active_state(&mut app).model_picker.models = Some(vec![mewcode_client::net::ModelEntry {
        id: "Vendor/Exact:free".into(),
        display_name: "Vendor Exact".into(),
        provider: ProviderId::OpenRouter,
        kind: mewcode_protocol::ModelKind::OpenRouter,
        context_length: Some(262_144),
        is_free: true,
    }]);
    active_state(&mut app).overlay = Overlay::ModelPicker;
    render_picker(&mut app, 80, 24);
    let rect = active_state(&mut app)
        .model_picker
        .picker
        .rect
        .expect("render should store model picker geometry");

    let cmd = update(
        &mut app,
        picker_mouse(MouseEventKind::Down(MouseButton::Left), rect.x, rect.y + 1),
    );

    assert!(matches!(
        cmd,
        Cmd::PatchSession {
            patch: mewcode_client::net::SessionPatch {
                model: Some(ref model),
                model_context_length: Some(262_144),
                ..
            },
            ..
        } if model == &ModelRef::openrouter("Vendor/Exact:free").unwrap()
    ));
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
    assert!(matches!(cmd, Cmd::FetchModels(_)));
    // The composer is cleared by the slash submit path.
    assert!(s.composer.lines().join("\n").is_empty());
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
    assert!(s.composer.lines().join("\n").is_empty());
}

#[test]
fn picker_quit_and_seeded_command_clear_pasted_text() {
    let mut app = test_app();
    seed_active_session(active_state(&mut app));
    {
        let s = active_state(&mut app);
        // Simulate a stale pasted marker from an earlier paste.
        s.pasted.push(mewcode_client::runtime::model::PastedText {
            marker: "[Pasted ~2 lines]".into(),
            text: "a\nb".into(),
        });
    }
    // Fresh composer, picker opens on `/`.
    let _ = update(&mut app, type_char('/'));
    assert_eq!(active_state(&mut app).overlay, Overlay::SlashPicker);

    // Selecting /quit (index 11 in SLASH_COMMANDS) must clear the composer
    // AND the pending pasted text.
    for _ in 0..11 {
        let _ = update(&mut app, press_arrow(KeyCode::Down));
    }
    let cmd = update(&mut app, press_enter());
    assert!(matches!(cmd, Cmd::Quit));
    let s = active_state(&mut app);
    assert!(s.composer.lines().join("\n").is_empty());
    assert!(s.pasted.is_empty());

    // Seeding any other command draft must also drop pending pasted text.
    let mut app = test_app();
    seed_active_session(active_state(&mut app));
    {
        let s = active_state(&mut app);
        s.pasted.push(mewcode_client::runtime::model::PastedText {
            marker: "[Pasted ~2 lines]".into(),
            text: "a\nb".into(),
        });
    }
    let _ = update(&mut app, type_char('/'));
    let _ = update(&mut app, press_enter()); // /model at index 0
    let s = active_state(&mut app);
    assert_eq!(s.overlay, Overlay::ModelPicker);
    assert!(s.composer.lines().join("\n").is_empty());
    assert!(s.pasted.is_empty());
}

fn seed_models(n: usize) -> Vec<mewcode_client::net::ModelEntry> {
    (0..n)
        .map(|i| mewcode_client::net::ModelEntry {
            id: format!("id-{i}"),
            display_name: format!("Model {i}"),
            provider: mewcode_protocol::ProviderId::OpenCodeGo,
            kind: mewcode_protocol::ModelKind::OpenCodeGo,
            context_length: None,
            is_free: false,
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
    s.model_picker.picker.cursor = 0;
    s.model_picker.picker.scroll = 0;

    let max_width = 30; // tight enough to force truncation for long ids
    s.model_picker.models.as_mut().unwrap()[0].display_name = "界".repeat(40);
    let lines = model_picker_lines(s, max_width);
    assert_eq!(lines.len(), 6); // one provider header + five models
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
            span.content.width() <= max_width,
            "row {i} text {:?} ({} cells) exceeds width {max_width}",
            span.content,
            span.content.width()
        );
    }
}

#[test]
fn free_openrouter_models_are_marked_without_hiding_exact_id() {
    use mewcode_client::runtime::view::model_picker_lines;
    let mut app = test_app();
    let state = active_state(&mut app);
    state.model_picker.models = Some(vec![mewcode_client::net::ModelEntry {
        id: "Vendor/Exact:free".into(),
        display_name: "Vendor Exact".into(),
        provider: ProviderId::OpenRouter,
        kind: mewcode_protocol::ModelKind::OpenRouter,
        context_length: Some(262_144),
        is_free: true,
    }]);

    let text = model_picker_lines(state, 80)[1]
        .spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect::<String>();
    assert!(text.contains("[free]"), "missing free marker: {text}");
    assert!(
        text.contains("Vendor/Exact:free"),
        "exact id hidden: {text}"
    );
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

    assert_eq!(active_state(&mut app).model_picker.picker.cursor, 14);
    assert!(
        buf.contains("Model 14"),
        "last cursor row should be visible, not replaced by the footer:\n{buf}"
    );
    assert!(buf.contains("15/15"), "footer should still render:\n{buf}");
}

/// Regression test: a manual `/compact` sets `s.streaming = Some(...)` so
/// its progress renders through the same live-turn UI as a normal chat
/// reply (see `StreamMsg::CompactionStarted`). `on_session_submit` must
/// queue a message typed during compaction into `s.message_queue` and
/// clear the composer, exactly like a message typed during a busy chat
/// turn — both share the same queueing path.
#[test]
fn message_sent_during_manual_compact_is_queued_not_stuck_in_composer() {
    let mut app = test_app();
    let s = active_state(&mut app);
    seed_active_session(s);
    // Simulate the state after `/compact` has started: both flags a real
    // manual compaction sets are true at once.
    s.compaction.active = true;
    s.streaming = Some(mewcode_client::runtime::model::StreamingState::new(
        uuid::Uuid::nil(),
    ));

    type_text(s, "hello");
    let cmd = update(&mut app, press_enter());

    assert!(matches!(cmd, Cmd::None));
    let s = active_state(&mut app);
    assert_eq!(
        s.composer.lines().join("\n"),
        "",
        "the composer must be cleared, not left holding the typed text"
    );
    assert_eq!(
        s.message_queue.as_slice(),
        ["hello"],
        "the message must be queued for automatic send once compaction finishes"
    );
}

#[test]
fn model_picker_search_filters_and_selects_filtered_model() {
    let mut app = test_app();
    seed_active_session(active_state(&mut app));
    {
        let s = active_state(&mut app);
        s.overlay = Overlay::ModelPicker;
        s.model_picker.models = Some(vec![
            mewcode_client::net::ModelEntry {
                id: "alpha/id".into(),
                display_name: "Alpha".into(),
                provider: ProviderId::OpenRouter,
                kind: mewcode_protocol::ModelKind::OpenRouter,
                context_length: None,
                is_free: false,
            },
            mewcode_client::net::ModelEntry {
                id: "beta/id".into(),
                display_name: "Beta".into(),
                provider: ProviderId::OpenAi,
                kind: mewcode_protocol::ModelKind::OpenAi,
                context_length: Some(8_192),
                is_free: false,
            },
        ]);
        s.model_picker.picker.cursor = 1;
        s.model_picker.picker.scroll = 3;
    }

    for c in "OPENAI".chars() {
        update(&mut app, type_char(c));
    }

    let state = active_state(&mut app);
    assert_eq!(state.model_picker.query.lines(), &["OPENAI"]);
    assert_eq!(state.model_picker.filtered_models().len(), 1);
    assert_eq!(state.model_picker.picker.cursor, 0);
    assert_eq!(state.model_picker.picker.scroll, 0);

    let cmd = update(&mut app, press_enter());
    assert!(matches!(
        cmd,
        Cmd::PatchSession {
            patch: mewcode_client::net::SessionPatch {
                model: Some(ref model),
                model_context_length: Some(8_192),
                ..
            },
            ..
        } if model == &ModelRef::openai("beta/id").unwrap()
    ));
}

#[test]
fn model_picker_search_matches_display_name_id_and_restores_on_backspace() {
    let mut app = test_app();
    {
        let s = active_state(&mut app);
        s.overlay = Overlay::ModelPicker;
        s.model_picker.models = Some(vec![
            mewcode_client::net::ModelEntry {
                id: "vendor/exact-id".into(),
                display_name: "Friendly Name".into(),
                provider: ProviderId::OpenRouter,
                kind: mewcode_protocol::ModelKind::OpenRouter,
                context_length: None,
                is_free: false,
            },
            mewcode_client::net::ModelEntry {
                id: "other".into(),
                display_name: "Other".into(),
                provider: ProviderId::OpenAi,
                kind: mewcode_protocol::ModelKind::OpenAi,
                context_length: None,
                is_free: false,
            },
        ]);
    }

    for c in "FRIENDLY".chars() {
        update(&mut app, type_char(c));
    }
    assert_eq!(
        active_state(&mut app).model_picker.filtered_models().len(),
        1
    );

    active_state(&mut app).model_picker.query = TextArea::new(vec!["EXACT-ID".into()]);
    active_state(&mut app)
        .model_picker
        .query
        .move_cursor(CursorMove::End);
    assert_eq!(
        active_state(&mut app).model_picker.filtered_models().len(),
        1
    );

    update(
        &mut app,
        Msg::Key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE)),
    );
    assert_eq!(
        active_state(&mut app).model_picker.query.lines(),
        &["EXACT-I"]
    );
    assert_eq!(
        active_state(&mut app).model_picker.filtered_models().len(),
        1
    );
}

#[test]
fn model_picker_no_match_is_clear_and_enter_is_noop() {
    let mut app = test_app();
    seed_active_session(active_state(&mut app));
    {
        let s = active_state(&mut app);
        s.overlay = Overlay::ModelPicker;
        s.model_picker.models = Some(seed_models(2));
    }
    for c in "missing".chars() {
        update(&mut app, type_char(c));
    }

    let rendered = draw(&mut app, 100, 28);
    assert!(rendered.contains("No matching models."), "{rendered}");
    assert!(rendered.contains("0/2"), "{rendered}");
    assert!(matches!(update(&mut app, press_enter()), Cmd::None));
}

#[test]
fn reopening_model_picker_clears_search_cursor_and_scroll() {
    let mut app = test_app();
    {
        let s = active_state(&mut app);
        s.model_picker.query = TextArea::new(vec!["old".into()]);
        s.model_picker.picker.cursor = 4;
        s.model_picker.picker.scroll = 7;
        type_text(s, "/model");
    }

    update(&mut app, press_enter());

    let state = active_state(&mut app);
    assert_eq!(state.model_picker.query.lines(), &[""]);
    assert_eq!(state.model_picker.picker.cursor, 0);
    assert_eq!(state.model_picker.picker.scroll, 0);
}

#[test]
fn model_picker_renders_search_and_parks_cursor_at_edit_position() {
    let mut app = test_app();
    {
        let s = active_state(&mut app);
        s.overlay = Overlay::ModelPicker;
        s.model_picker.models = Some(seed_models(2));
        s.model_picker.query = TextArea::new(vec!["ac".into()]);
        s.model_picker.query.move_cursor(CursorMove::Jump(0, 1));
    }
    update(&mut app, type_char('b'));

    let mut terminal = Terminal::new(TestBackend::new(100, 28)).unwrap();
    terminal.draw(|frame| render(frame, &mut app)).unwrap();
    let rendered = terminal.backend().to_string();
    let cursor = terminal.backend_mut().get_cursor_position().unwrap();
    let search_row = rendered
        .lines()
        .enumerate()
        .find_map(|(row, line)| line.contains("Search: abc").then_some(row))
        .expect("search field should render inside the model modal");

    assert_eq!(active_state(&mut app).model_picker.query.lines(), &["abc"]);
    assert_eq!(cursor.y, search_row as u16);
    assert_eq!(
        rendered
            .lines()
            .nth(search_row)
            .unwrap()
            .chars()
            .nth(cursor.x as usize),
        Some('b')
    );
}

#[test]
fn filtered_model_picker_mouse_maps_grouped_rows_to_filtered_entries() {
    let mut app = test_app();
    seed_active_session(active_state(&mut app));
    {
        let s = active_state(&mut app);
        s.overlay = Overlay::ModelPicker;
        s.model_picker.models = Some(vec![
            mewcode_client::net::ModelEntry {
                id: "hidden".into(),
                display_name: "Hidden".into(),
                provider: ProviderId::OpenRouter,
                kind: mewcode_protocol::ModelKind::OpenRouter,
                context_length: None,
                is_free: false,
            },
            mewcode_client::net::ModelEntry {
                id: "target".into(),
                display_name: "Target".into(),
                provider: ProviderId::OpenAi,
                kind: mewcode_protocol::ModelKind::OpenAi,
                context_length: None,
                is_free: false,
            },
        ]);
        s.model_picker.query = TextArea::new(vec!["target".into()]);
    }
    render_picker(&mut app, 100, 28);
    let rect = active_state(&mut app).model_picker.picker.rect.unwrap();

    let cmd = update(
        &mut app,
        picker_mouse(MouseEventKind::Down(MouseButton::Left), rect.x, rect.y + 1),
    );

    assert!(matches!(
        cmd,
        Cmd::PatchSession {
            patch: mewcode_client::net::SessionPatch { model: Some(ref model), .. },
            ..
        } if model == &ModelRef::openai("target").unwrap()
    ));
}

#[test]
fn model_picker_clamps_cursor_and_scroll_when_catalog_shrinks() {
    let mut app = test_app();
    open_model_picker(&mut app);
    let generation = active_state(&mut app).model_picker.generation;
    update(
        &mut app,
        Msg::ModelsFetched(Ok(seed_models(20)), generation),
    );
    draw(&mut app, 100, 28);
    {
        let s = active_state(&mut app);
        s.model_picker.picker.cursor = 19;
        s.model_picker.picker.scroll = 20;
    }

    update(&mut app, Msg::ModelsFetched(Ok(seed_models(2)), generation));

    let s = active_state(&mut app);
    assert_eq!(s.model_picker.picker.cursor, 1);
    assert_eq!(s.model_picker.picker.scroll, 0);
}

#[test]
fn model_picker_resize_uses_current_search_adjusted_viewport() {
    let mut app = test_app();
    {
        let s = active_state(&mut app);
        s.overlay = Overlay::ModelPicker;
        s.model_picker.models = Some(seed_models(20));
        s.model_picker.picker.cursor = 19;
    }
    draw(&mut app, 100, 40);
    let rendered = draw(&mut app, 100, 20);

    assert!(
        rendered.contains("Model 19"),
        "selected row must remain visible:\n{rendered}"
    );
}

#[test]
fn model_picker_search_cursor_handles_wide_and_long_queries() {
    let mut app = test_app();
    {
        let s = active_state(&mut app);
        s.overlay = Overlay::ModelPicker;
        s.model_picker.models = Some(seed_models(1));
        s.model_picker.query = TextArea::new(vec![format!("e\u{301}👩‍💻{}z", "a".repeat(80))]);
        s.model_picker.query.move_cursor(CursorMove::End);
    }

    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
    terminal.draw(|frame| render(frame, &mut app)).unwrap();
    let rendered = terminal.backend().to_string();
    let cursor = terminal.backend_mut().get_cursor_position().unwrap();
    let rect = active_state(&mut app).model_picker.picker.rect.unwrap();

    assert!(
        rendered
            .lines()
            .any(|line| line.contains("Search:") && line.contains('z'))
    );
    assert_eq!(cursor.y, rect.y - 1);
    assert!(cursor.x >= rect.x + " Search: ".chars().count() as u16);
    assert!(cursor.x < rect.x + rect.width);
}

#[test]
fn zen_picker_carries_transport_snapshot_through_patch_and_create() {
    let entry = mewcode_client::net::ModelEntry {
        id: "gpt-5.4".into(),
        display_name: "GPT 5.4".into(),
        provider: ProviderId::OpenCodeZen,
        kind: mewcode_protocol::ModelKind::OpenAiResponses,
        context_length: Some(1_050_000),
        is_free: false,
    };

    let mut app = test_app();
    seed_active_session(active_state(&mut app));
    active_state(&mut app).model_picker.models = Some(vec![entry.clone()]);
    active_state(&mut app).overlay = Overlay::ModelPicker;
    match update(&mut app, press_enter()) {
        Cmd::PatchSession { patch, .. } => {
            assert_eq!(
                patch.model_kind,
                Some(mewcode_protocol::ModelKind::OpenAiResponses)
            );
            assert_eq!(patch.model_context_length, Some(1_050_000));
        }
        other => panic!("expected model patch, got {other:?}"),
    }

    let mut app = test_app();
    active_state(&mut app).model_picker.models = Some(vec![entry]);
    active_state(&mut app).overlay = Overlay::ModelPicker;
    assert!(matches!(update(&mut app, press_enter()), Cmd::None));
    assert_eq!(
        active_state(&mut app).creation.pending_model_kind,
        Some(mewcode_protocol::ModelKind::OpenAiResponses)
    );
    type_text(active_state(&mut app), "hello");
    match update(&mut app, press_enter()) {
        Cmd::CreateSession(request) => {
            assert_eq!(
                request.model_kind,
                Some(mewcode_protocol::ModelKind::OpenAiResponses)
            );
        }
        other => panic!("expected create, got {other:?}"),
    }
}

#[test]
fn bare_session_new_carries_model_kind_snapshot() {
    let mut app = test_app();
    seed_active_session(active_state(&mut app));
    let session = active_state(&mut app).session.as_mut().unwrap();
    session.model = ModelRef::open_code_zen("gpt-5.4").unwrap();
    session.model_kind = Some(mewcode_protocol::ModelKind::OpenAiResponses);
    type_text(active_state(&mut app), "/session new");

    update(&mut app, press_enter());

    assert_eq!(
        active_state(&mut app).creation.pending_model_kind,
        Some(mewcode_protocol::ModelKind::OpenAiResponses)
    );
}
