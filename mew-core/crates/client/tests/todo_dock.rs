//! Tests for the todo dock: the live `ToolDisplay::Todo` fold (`I006`), the
//! session-open hydrate (`I007`), `dock_height` sizing, and a render smoke.

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use uuid::Uuid;

use mewcode_client::net::Session;
use mewcode_client::runtime::model::{App, Msg, Screen, SessionState, StreamMsg};
use mewcode_client::runtime::update;
use mewcode_client::runtime::view::{DOCK_MAX_ROWS, dock_height, render};
use mewcode_protocol::{Mode, ModelId, TodoDisplay, TodoItem, TodoStatus, ToolDisplay};

fn session(todos: Vec<TodoItem>) -> Session {
    Session {
        id: Uuid::new_v4(),
        title: "todo".to_string(),
        model: ModelId::default().into(),
        model_kind: None,
        model_context_length: None,
        mode: Mode::default(),
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
        messages: vec![],
        compaction_summary: None,
        compacted_up_to: None,
        todos,
    }
}

fn item(content: &str, status: TodoStatus) -> TodoItem {
    TodoItem {
        id: None,
        content: content.to_string(),
        status,
    }
}

/// `I006`: a `ToolDisplay::Todo` stream event folds the list into session
/// state, independent of which tool call it is attached to.
#[test]
fn todo_display_event_folds_into_session_state() {
    let mut app = App::new();
    app.screen = Screen::Session(SessionState::new(session(vec![])));

    update(
        &mut app,
        Msg::Stream(StreamMsg::ToolDisplay {
            id: "call-1".into(),
            display: ToolDisplay::Todo(TodoDisplay {
                todos: vec![
                    item("a", TodoStatus::InProgress),
                    item("b", TodoStatus::Completed),
                ],
            }),
        }),
    );

    let Screen::Session(state) = &app.screen;
    assert_eq!(state.todos.len(), 2);
    assert_eq!(state.todos[0].content, "a");
    assert_eq!(state.todos[1].status, TodoStatus::Completed);
}

/// `I006`: an empty todo snapshot clears the dock.
#[test]
fn empty_todo_display_clears_dock() {
    let mut app = App::new();
    app.screen = Screen::Session(SessionState::new(session(vec![item(
        "stale",
        TodoStatus::Pending,
    )])));
    update(
        &mut app,
        Msg::Stream(StreamMsg::ToolDisplay {
            id: "call-1".into(),
            display: ToolDisplay::Todo(TodoDisplay { todos: vec![] }),
        }),
    );
    let Screen::Session(state) = &app.screen;
    assert!(state.todos.is_empty());
    assert_eq!(dock_height(state), 0);
}

/// `I007`: opening a session seeds the dock from the session's persisted
/// task list.
#[test]
fn new_session_seeds_dock_from_persisted_todos() {
    let state = SessionState::new(session(vec![item("hydrated", TodoStatus::Pending)]));
    assert_eq!(state.todos.len(), 1);
    assert_eq!(state.todos[0].content, "hydrated");
    assert_eq!(dock_height(&state), 2);
}

/// Dock height: hidden when empty, one header row + items, capped.
#[test]
fn dock_height_hides_when_empty_and_caps_at_max() {
    let empty = SessionState::new(session(vec![]));
    assert_eq!(dock_height(&empty), 0);

    let mut app = App::new();
    app.screen = Screen::Session(SessionState::new(session(vec![])));
    let many: Vec<TodoItem> = (0..20)
        .map(|i| item(&format!("t{i}"), TodoStatus::Pending))
        .collect();
    update(
        &mut app,
        Msg::Stream(StreamMsg::ToolDisplay {
            id: "c".into(),
            display: ToolDisplay::Todo(TodoDisplay { todos: many }),
        }),
    );
    let Screen::Session(state) = &app.screen;
    assert_eq!(dock_height(state), DOCK_MAX_ROWS + 1);
}

/// Collapsed dock: header row only. Clicking the header row toggles.
#[test]
fn dock_collapses_to_header_and_click_toggles() {
    let mut app = App::new();
    app.screen = Screen::Session(SessionState::new(session(vec![
        item("a", TodoStatus::Pending),
        item("b", TodoStatus::Pending),
    ])));
    let mut terminal = Terminal::new(TestBackend::new(100, 24)).unwrap();
    terminal
        .draw(|frame| render(frame, &mut app))
        .expect("draw");

    let Screen::Session(state) = &app.screen;
    assert_eq!(dock_height(state), 3, "expanded: header + 2 rows");
    let header = state.dock_header.expect("dock header recorded");

    // A click on the header collapses the dock.
    update(
        &mut app,
        Msg::Mouse(crossterm::event::MouseEvent {
            kind: crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left),
            column: header.x + 1,
            row: header.y,
            modifiers: crossterm::event::KeyModifiers::NONE,
        }),
    );
    let Screen::Session(state) = &app.screen;
    assert!(state.todos_collapsed);
    assert_eq!(dock_height(state), 1, "collapsed: header only");

    // Click again (after re-render refreshes the rect) to expand.
    terminal
        .draw(|frame| render(frame, &mut app))
        .expect("draw");
    let Screen::Session(state) = &app.screen;
    let header = state
        .dock_header
        .expect("collapsed dock still has a header");
    update(
        &mut app,
        Msg::Mouse(crossterm::event::MouseEvent {
            kind: crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left),
            column: header.x + 1,
            row: header.y,
            modifiers: crossterm::event::KeyModifiers::NONE,
        }),
    );
    let Screen::Session(state) = &app.screen;
    assert!(!state.todos_collapsed);
}

/// Clicks outside the dock header are ignored.
#[test]
fn click_outside_dock_header_is_ignored() {
    let mut app = App::new();
    app.screen = Screen::Session(SessionState::new(session(vec![item(
        "a",
        TodoStatus::Pending,
    )])));
    let mut terminal = Terminal::new(TestBackend::new(100, 24)).unwrap();
    terminal
        .draw(|frame| render(frame, &mut app))
        .expect("draw");

    update(
        &mut app,
        Msg::Mouse(crossterm::event::MouseEvent {
            kind: crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left),
            column: 0,
            row: 0,
            modifiers: crossterm::event::KeyModifiers::NONE,
        }),
    );
    let Screen::Session(state) = &app.screen;
    assert!(!state.todos_collapsed);
}

/// Wheel scroll moves the transcript: up releases follow, down at the bottom
/// re-engages it. Guards the regression where mouse capture made terminals
/// stop translating the wheel into arrow keys.
#[test]
fn wheel_scrolls_transcript() {
    let mut app = App::new();
    app.screen = Screen::Session(SessionState::new(session(vec![])));
    {
        let Screen::Session(state) = &mut app.screen;
        state.max_scroll = 30;
        state.scroll = 10;
        state.follow = false;
    }

    let wheel = |kind| crossterm::event::MouseEvent {
        kind,
        column: 5,
        row: 2,
        modifiers: crossterm::event::KeyModifiers::NONE,
    };

    update(
        &mut app,
        Msg::Mouse(wheel(crossterm::event::MouseEventKind::ScrollUp)),
    );
    let Screen::Session(state) = &app.screen;
    assert_eq!(state.scroll, 7, "wheel up scrolls back three rows");

    update(
        &mut app,
        Msg::Mouse(wheel(crossterm::event::MouseEventKind::ScrollDown)),
    );
    update(
        &mut app,
        Msg::Mouse(wheel(crossterm::event::MouseEventKind::ScrollDown)),
    );
    let Screen::Session(state) = &app.screen;
    assert_eq!(state.scroll, 13, "wheel down scrolls forward");

    // Scrolling past the bottom clamps and re-engages follow.
    update(
        &mut app,
        Msg::Mouse(wheel(crossterm::event::MouseEventKind::ScrollDown)),
    );
    update(
        &mut app,
        Msg::Mouse(wheel(crossterm::event::MouseEventKind::ScrollDown)),
    );
    update(
        &mut app,
        Msg::Mouse(wheel(crossterm::event::MouseEventKind::ScrollDown)),
    );
    update(
        &mut app,
        Msg::Mouse(wheel(crossterm::event::MouseEventKind::ScrollDown)),
    );
    update(
        &mut app,
        Msg::Mouse(wheel(crossterm::event::MouseEventKind::ScrollDown)),
    );
    update(
        &mut app,
        Msg::Mouse(wheel(crossterm::event::MouseEventKind::ScrollDown)),
    );
    let Screen::Session(state) = &app.screen;
    assert_eq!(state.scroll, state.max_scroll);
    assert!(state.follow, "reaching the bottom re-engages follow");
}

/// A fully-completed list hides the dock so finished work does not crowd the
/// next task; a fresh list with pending items brings it back.
#[test]
fn all_completed_list_hides_dock_until_next_task() {
    let mut app = App::new();
    app.screen = Screen::Session(SessionState::new(session(vec![item(
        "done",
        TodoStatus::Completed,
    )])));
    let Screen::Session(state) = &app.screen;
    assert_eq!(dock_height(state), 0, "finished list takes no space");

    update(
        &mut app,
        Msg::Stream(StreamMsg::ToolDisplay {
            id: "c".into(),
            display: ToolDisplay::Todo(TodoDisplay {
                todos: vec![
                    item("done", TodoStatus::Completed),
                    item("next", TodoStatus::Pending),
                ],
            }),
        }),
    );
    let Screen::Session(state) = &app.screen;
    assert_eq!(dock_height(state), 3, "new task brings the dock back");
}
#[test]
fn mouse_motion_is_not_actionable() {
    let mut app = App::new();
    app.screen = Screen::Session(SessionState::new(session(vec![item(
        "a",
        TodoStatus::Pending,
    )])));
    let Screen::Session(before_state) = &app.screen;
    let before = (before_state.scroll, before_state.todos_collapsed);
    update(
        &mut app,
        Msg::Mouse(crossterm::event::MouseEvent {
            kind: crossterm::event::MouseEventKind::Moved,
            column: 1,
            row: 1,
            modifiers: crossterm::event::KeyModifiers::NONE,
        }),
    );
    let Screen::Session(state) = &app.screen;
    assert_eq!((state.scroll, state.todos_collapsed), before);
}

/// Esc during a live turn aborts it (OpenCode / Claude Code parity); Esc
/// with no live turn is a no-op.
#[test]
fn esc_during_streaming_requests_abort() {
    use crossterm::event::{KeyCode, KeyEvent};
    use mewcode_client::runtime::model::{Cmd, StreamingState};

    let mut app = App::new();
    let id = uuid::Uuid::new_v4();
    app.screen = Screen::Session(SessionState::new(session_at(id, vec![])));
    {
        let Screen::Session(state) = &mut app.screen;
        state.streaming = Some(StreamingState::new(uuid::Uuid::nil()));
    }

    let cmd = update(&mut app, Msg::Key(KeyEvent::from(KeyCode::Esc)));
    assert!(matches!(cmd, Cmd::AbortSession(sid) if sid == id));

    let Screen::Session(state) = &mut app.screen;
    state.streaming = None;
    let cmd = update(&mut app, Msg::Key(KeyEvent::from(KeyCode::Esc)));
    assert!(matches!(cmd, Cmd::None));
}

/// Esc with an overlay open closes the overlay only — it must not also abort
/// the live turn underneath.
#[test]
fn esc_closes_overlay_without_aborting_turn() {
    use crossterm::event::{KeyCode, KeyEvent};
    use mewcode_client::runtime::model::{Cmd, Overlay, StreamingState};

    let mut app = App::new();
    let id = uuid::Uuid::new_v4();
    app.screen = Screen::Session(SessionState::new(session_at(id, vec![])));
    {
        let Screen::Session(state) = &mut app.screen;
        state.streaming = Some(StreamingState::new(uuid::Uuid::nil()));
        state.overlay = Overlay::ModelPicker;
    }

    let cmd = update(&mut app, Msg::Key(KeyEvent::from(KeyCode::Esc)));
    assert!(matches!(cmd, Cmd::None), "overlay Esc must not abort");
    let Screen::Session(state) = &app.screen;
    assert_eq!(state.overlay, Overlay::None, "overlay closed");
    assert!(state.streaming.is_some(), "live turn untouched");
}

fn session_at(id: uuid::Uuid, todos: Vec<TodoItem>) -> Session {
    let mut s = session(todos);
    s.id = id;
    s
}
#[test]
fn token_usage_event_updates_context_live() {
    let mut app = App::new();
    app.screen = Screen::Session(SessionState::new(session(vec![])));
    update(
        &mut app,
        Msg::Stream(StreamMsg::TokenUsage {
            input_tokens: 500,
            output_tokens: 100,
            session_tokens: 3600,
            context_limit: 200_000,
        }),
    );
    let Screen::Session(state) = &app.screen;
    assert_eq!(state.session_tokens, 3600);
    assert_eq!(state.context_limit, 200_000);
}

/// Render smoke: drawing a session with todos does not panic and fits the
/// allocated dock area.
#[test]
fn dock_renders_without_panicking() {
    let mut app = App::new();
    app.screen = Screen::Session(SessionState::new(session(vec![
        item("pending task", TodoStatus::Pending),
        item("in progress task", TodoStatus::InProgress),
        item("done task", TodoStatus::Completed),
    ])));
    let mut terminal = Terminal::new(TestBackend::new(100, 24)).unwrap();
    terminal
        .draw(|frame| render(frame, &mut app))
        .expect("draw");
}
