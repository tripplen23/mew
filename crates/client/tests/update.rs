//! Unit tests for the pure Elm-style `update` function.
//!
//! These exercise `update` through its public API only: build an `App`, feed
//! it a `Msg`, and assert on the resulting model mutation and returned `Cmd`.
//! No I/O happens — `update` is synchronous and side-effect-free.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use uuid::Uuid;

use mewcode_client::net::{Session, SessionSummary};
use mewcode_client::runtime::model::{
    App, Cmd, HomeState, Msg, NewSessionField, NewSessionState, Overlay, Screen, SessionState,
    StreamMsg,
};
use mewcode_client::runtime::update;
use mewcode_protocol::{MessagePart, Mode, ModelId, Role};

// --- test fixtures -------------------------------------------------------

fn test_app() -> App {
    App::new()
}

fn key(code: KeyCode) -> Msg {
    Msg::Key(KeyEvent::new(code, KeyModifiers::NONE))
}

fn char_key(c: char) -> Msg {
    Msg::Key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE))
}

fn summary(title: &str) -> SessionSummary {
    SessionSummary {
        id: Uuid::new_v4(),
        title: title.to_string(),
        model: ModelId::default(),
        mode: Mode::default(),
        created_at: chrono::Utc::now(),
    }
}

fn session() -> Session {
    Session {
        id: Uuid::new_v4(),
        title: "demo".to_string(),
        model: ModelId::default(),
        mode: Mode::default(),
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
        messages: vec![],
    }
}

/// Build an app already sitting on a Home screen with `n` loaded sessions.
fn home_with(n: usize) -> App {
    let mut app = test_app();
    let sessions = (0..n).map(|i| summary(&format!("s{i}"))).collect();
    app.screen = Screen::Home(HomeState {
        sessions,
        selected: 0,
        loading: false,
    });
    app
}

/// Build an app sitting on a Session screen.
fn on_session() -> App {
    let mut app = test_app();
    app.screen = Screen::Session(SessionState::new(session()));
    app
}

fn home(app: &App) -> &HomeState {
    match &app.screen {
        Screen::Home(h) => h,
        other => panic!("expected Home, got {other:?}"),
    }
}

fn sess(app: &App) -> &SessionState {
    match &app.screen {
        Screen::Session(s) => s,
        other => panic!("expected Session, got {other:?}"),
    }
}

// --- Home navigation / clamping / open -----------------------------------

#[test]
fn home_up_clamps_at_top_without_wrapping() {
    let mut app = home_with(3);
    assert!(matches!(update(&mut app, key(KeyCode::Up)), Cmd::None));
    assert_eq!(home(&app).selected, 0);
}

#[test]
fn home_down_advances_then_clamps_at_bottom() {
    let mut app = home_with(3);
    update(&mut app, key(KeyCode::Down));
    assert_eq!(home(&app).selected, 1);
    update(&mut app, key(KeyCode::Down));
    assert_eq!(home(&app).selected, 2);
    // Already at the last row: stays put, no wrap.
    update(&mut app, key(KeyCode::Down));
    assert_eq!(home(&app).selected, 2);
}

#[test]
fn home_navigation_is_noop_on_empty_list() {
    let mut app = home_with(0);
    update(&mut app, key(KeyCode::Down));
    update(&mut app, key(KeyCode::Up));
    assert_eq!(home(&app).selected, 0);
}

#[test]
fn home_enter_on_empty_list_does_nothing() {
    let mut app = home_with(0);
    assert!(matches!(update(&mut app, key(KeyCode::Enter)), Cmd::None));
    assert!(matches!(app.screen, Screen::Home(_)));
}

#[test]
fn home_enter_opens_selected_session() {
    let mut app = home_with(3);
    update(&mut app, key(KeyCode::Down));
    let expected = home(&app).sessions[1].id;
    match update(&mut app, key(KeyCode::Enter)) {
        Cmd::OpenSession(id) => assert_eq!(id, expected),
        other => panic!("expected OpenSession, got {other:?}"),
    }
}

#[test]
fn home_n_opens_new_session_form() {
    let mut app = home_with(1);
    assert!(matches!(update(&mut app, char_key('n')), Cmd::None));
    assert!(matches!(app.screen, Screen::NewSession(_)));
}

#[test]
fn home_q_quits() {
    let mut app = home_with(1);
    assert!(matches!(update(&mut app, char_key('q')), Cmd::None));
    assert!(app.should_quit);
}

// --- NewSession Tab cycling + validation ---------------------------------

fn new_session_app() -> App {
    let mut app = test_app();
    app.screen = Screen::NewSession(NewSessionState::default());
    app
}

fn field(app: &App) -> NewSessionField {
    match &app.screen {
        Screen::NewSession(n) => n.field,
        other => panic!("expected NewSession, got {other:?}"),
    }
}

#[test]
fn new_session_tab_cycles_focus() {
    let mut app = new_session_app();
    assert_eq!(field(&app), NewSessionField::Title);
    update(&mut app, key(KeyCode::Tab));
    assert_eq!(field(&app), NewSessionField::Model);
    update(&mut app, key(KeyCode::Tab));
    assert_eq!(field(&app), NewSessionField::Mode);
    update(&mut app, key(KeyCode::Tab));
    assert_eq!(field(&app), NewSessionField::Title);
}

#[test]
fn new_session_empty_title_is_rejected() {
    let mut app = new_session_app();
    assert!(matches!(update(&mut app, key(KeyCode::Enter)), Cmd::None));
    assert!(matches!(app.screen, Screen::NewSession(_)));
    assert_eq!(field(&app), NewSessionField::Title);
    assert!(app.toast.is_some());
}

#[test]
fn new_session_whitespace_title_is_rejected() {
    let mut app = new_session_app();
    update(&mut app, char_key(' '));
    update(&mut app, char_key(' '));
    assert!(matches!(update(&mut app, key(KeyCode::Enter)), Cmd::None));
    assert!(matches!(app.screen, Screen::NewSession(_)));
}

#[test]
fn new_session_valid_title_submits_trimmed() {
    let mut app = new_session_app();
    update(&mut app, char_key(' '));
    update(&mut app, char_key('h'));
    update(&mut app, char_key('i'));
    update(&mut app, char_key(' '));
    match update(&mut app, key(KeyCode::Enter)) {
        Cmd::CreateSession(req) => {
            assert_eq!(req.title, "hi");
            assert_eq!(req.model, Some(ModelId::default()));
            assert_eq!(req.mode, Some(Mode::default()));
        }
        other => panic!("expected CreateSession, got {other:?}"),
    }
}

#[test]
fn new_session_esc_returns_to_loading_home() {
    let mut app = new_session_app();
    assert!(matches!(
        update(&mut app, key(KeyCode::Esc)),
        Cmd::LoadSessions
    ));
    assert!(home(&app).loading);
}

// --- Session slash-command parsing ---------------------------------------

/// Type a string into the Session input via key events, then return the app.
fn type_into_session(text: &str) -> App {
    let mut app = on_session();
    for c in text.chars() {
        update(&mut app, char_key(c));
    }
    app
}

#[test]
fn slash_tools_opens_tools_overlay() {
    let mut app = type_into_session("/tools");
    assert!(matches!(update(&mut app, key(KeyCode::Enter)), Cmd::None));
    assert_eq!(sess(&app).overlay, Overlay::Tools);
    assert!(sess(&app).streaming.is_none());
}

#[test]
fn slash_skills_opens_skills_overlay() {
    let mut app = type_into_session("/skills");
    assert!(matches!(update(&mut app, key(KeyCode::Enter)), Cmd::None));
    assert_eq!(sess(&app).overlay, Overlay::Skills);
}

#[test]
fn unknown_slash_command_errors_without_starting_turn() {
    let mut app = type_into_session("/bogus");
    assert!(matches!(update(&mut app, key(KeyCode::Enter)), Cmd::None));
    assert!(app.toast.is_some());
    assert_eq!(sess(&app).overlay, Overlay::None);
    assert!(sess(&app).streaming.is_none());
}

#[test]
fn empty_input_starts_no_turn() {
    let mut app = on_session();
    assert!(matches!(update(&mut app, key(KeyCode::Enter)), Cmd::None));
    assert!(sess(&app).streaming.is_none());
}

#[test]
fn plain_message_starts_a_chat_turn() {
    let mut app = type_into_session("hello");
    match update(&mut app, key(KeyCode::Enter)) {
        Cmd::StartChat(req) => {
            assert_eq!(req.messages.last().unwrap().role, Role::User);
        }
        other => panic!("expected StartChat, got {other:?}"),
    }
    let s = sess(&app);
    assert!(s.streaming.is_some());
    assert_eq!(s.session.messages.len(), 1);
}

#[test]
fn submit_while_streaming_is_rejected() {
    // A second submit while a turn is in flight must not orphan the
    // in-flight `StreamingState` — that would lose deltas and let a late
    // `Finished` commit garbage to history.
    let mut app = type_into_session("first");
    update(&mut app, key(KeyCode::Enter));
    assert!(sess(&app).streaming.is_some());
    let before = sess(&app).session.messages.len();

    update(&mut app, char_key('s'));
    update(&mut app, char_key('e'));
    update(&mut app, char_key('c'));
    update(&mut app, char_key('o'));
    update(&mut app, char_key('n'));
    update(&mut app, char_key('d'));
    assert!(matches!(update(&mut app, key(KeyCode::Enter)), Cmd::None));

    let s = sess(&app);
    // No second user message committed, no second turn started.
    assert_eq!(s.session.messages.len(), before);
    // Input is left intact so the user can retry once the in-flight turn ends.
    assert_eq!(s.input.lines().join("\n"), "second");
}

// --- apply_stream_event cases --------------------------------------------

fn stream(app: &mut App, ev: StreamMsg) -> Cmd {
    update(app, Msg::Stream(ev))
}

/// Drive a session into a streaming turn by submitting a plain message.
fn streaming_session() -> App {
    let mut app = type_into_session("go");
    update(&mut app, key(KeyCode::Enter));
    assert!(sess(&app).streaming.is_some());
    app
}

#[test]
fn stream_started_sets_assistant_id() {
    let mut app = streaming_session();
    let id = Uuid::new_v4();
    stream(&mut app, StreamMsg::Started(id));
    assert_eq!(sess(&app).streaming.as_ref().unwrap().assistant_id, id);
}

#[test]
fn stream_delta_accumulates_buffer() {
    let mut app = streaming_session();
    stream(&mut app, StreamMsg::Delta("Hel".to_string()));
    stream(&mut app, StreamMsg::Delta("lo".to_string()));
    assert_eq!(sess(&app).streaming.as_ref().unwrap().buffer, "Hello");
}

#[test]
fn stream_tool_input_then_output_is_recorded() {
    let mut app = streaming_session();
    stream(
        &mut app,
        StreamMsg::ToolInput {
            id: "c1".to_string(),
            name: "readFile".to_string(),
            input: serde_json::json!({"path": "a.rs"}),
        },
    );
    stream(
        &mut app,
        StreamMsg::ToolOutput {
            id: "c1".to_string(),
            output: serde_json::json!({"ok": true}),
        },
    );
    let calls = &sess(&app).streaming.as_ref().unwrap().tool_calls;
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].name, "readFile");
    assert_eq!(calls[0].output, Some(serde_json::json!({"ok": true})));
}

#[test]
fn stream_finished_commits_one_assistant_message() {
    let mut app = streaming_session();
    let before = sess(&app).session.messages.len();
    stream(&mut app, StreamMsg::Started(Uuid::new_v4()));
    stream(&mut app, StreamMsg::Delta("answer".to_string()));
    stream(
        &mut app,
        StreamMsg::ToolInput {
            id: "c1".to_string(),
            name: "glob".to_string(),
            input: serde_json::json!({}),
        },
    );
    stream(
        &mut app,
        StreamMsg::ToolOutput {
            id: "c1".to_string(),
            output: serde_json::json!(["x"]),
        },
    );
    assert!(matches!(
        stream(&mut app, StreamMsg::Finished { duration_ms: 12 }),
        Cmd::None
    ));

    let s = sess(&app);
    assert!(s.streaming.is_none());
    assert_eq!(s.session.messages.len(), before + 1);

    let committed = s.session.messages.last().unwrap();
    assert_eq!(committed.role, Role::Assistant);
    // Text first, then the tool call, then its result.
    assert!(matches!(committed.parts[0], MessagePart::Text { .. }));
    assert!(matches!(committed.parts[1], MessagePart::ToolCall(_)));
    assert!(matches!(committed.parts[2], MessagePart::ToolResult(_)));
}

#[test]
fn stream_failed_discards_partial_and_toasts() {
    let mut app = streaming_session();
    let before = sess(&app).session.messages.len();
    stream(&mut app, StreamMsg::Delta("partial".to_string()));
    stream(&mut app, StreamMsg::Failed("boom".to_string()));
    let s = sess(&app);
    assert!(s.streaming.is_none());
    assert_eq!(s.session.messages.len(), before);
    assert!(app.toast.is_some());
}

#[test]
fn stream_event_without_streaming_is_ignored() {
    let mut app = on_session();
    assert!(sess(&app).streaming.is_none());
    let before = sess(&app).session.messages.len();
    stream(&mut app, StreamMsg::Delta("ignored".to_string()));
    stream(&mut app, StreamMsg::Finished { duration_ms: 1 });
    let s = sess(&app);
    assert!(s.streaming.is_none());
    assert_eq!(s.session.messages.len(), before);
    // Failed with no tracked turn raises no toast.
    assert!(app.toast.is_none());
}
