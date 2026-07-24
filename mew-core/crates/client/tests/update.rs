//! Unit tests for the pure Elm-style `update` function.
//!
//! These exercise `update` through its public API only: build an `App`, feed
//! it a `Msg`, and assert on the resulting model mutation and returned `Cmd`.
//! No I/O happens — `update` is synchronous and side-effect-free.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use uuid::Uuid;

use mewcode_client::net::Session;
use mewcode_client::runtime::model::{App, Cmd, FileEntry, Msg, Overlay, Screen, SessionState};
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

fn ctrl_key(c: char) -> Msg {
    Msg::Key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL))
}

fn session_with_messages(messages: Vec<mewcode_protocol::Message>) -> Session {
    Session {
        id: Uuid::new_v4(),
        title: "demo".to_string(),
        model: ModelId::default(),
        mode: Mode::default(),
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
        messages,
        compaction_summary: None,
        compacted_up_to: None,
    }
}

fn session() -> Session {
    session_with_messages(vec![])
}

/// An app sitting on the Session screen with `session = None` (the entry state).
fn on_empty_session() -> App {
    test_app()
}

/// An app sitting on the Session screen with a hydrated session.
fn on_session() -> App {
    let mut app = test_app();
    app.screen = Screen::Session(SessionState::new(session()));
    app
}

fn sess(app: &App) -> &SessionState {
    match &app.screen {
        Screen::Session(s) => s,
    }
}

// --- chat-first flow: first message creates a session ---------------------

#[test]
fn empty_session_enter_is_noop() {
    let mut app = on_empty_session();
    assert!(matches!(update(&mut app, key(KeyCode::Enter)), Cmd::None));
    let s = sess(&app);
    assert!(s.session.is_none());
    assert!(s.creation.pending_chat.is_none());
    assert!(!s.creation.creating);
}

#[test]
fn empty_session_first_message_kicks_off_create() {
    let mut app = on_empty_session();
    type_chars(&mut app, "hello world");
    match update(&mut app, key(KeyCode::Enter)) {
        Cmd::CreateSession(req) => {
            assert_eq!(req.title, "hello world");
        }
        other => panic!("expected CreateSession, got {other:?}"),
    }
    let s = sess(&app);
    assert!(
        s.session.is_none(),
        "session still None while create in flight"
    );
    assert!(s.creation.creating);
    assert_eq!(s.creation.pending_chat.as_deref(), Some("hello world"));
    // The composer keeps the text so the user can retry if the create
    // fails. It is cleared in the `SessionCreated(Ok)` success path.
    assert_eq!(s.input.lines().join("\n"), "hello world");
}

#[test]
fn multiline_first_message_collapses_to_first_line() {
    let mut app = on_empty_session();
    for c in "first\nsecond".chars() {
        press(&mut app, KeyCode::Char(c));
    }
    match update(&mut app, key(KeyCode::Enter)) {
        Cmd::CreateSession(req) => {
            assert_eq!(req.title, "first");
        }
        other => panic!("expected CreateSession, got {other:?}"),
    }
}

#[test]
fn long_first_message_is_truncated_to_max_title() {
    let mut app = on_empty_session();
    let long = "a".repeat(200);
    for c in long.chars() {
        press(&mut app, KeyCode::Char(c));
    }
    match update(&mut app, key(KeyCode::Enter)) {
        Cmd::CreateSession(req) => {
            assert_eq!(
                req.title.chars().count(),
                60,
                "title should be exactly 60 chars (the cap), got {}",
                req.title.chars().count()
            );
        }
        other => panic!("expected CreateSession, got {other:?}"),
    }
}

#[test]
fn session_created_lifts_session_and_sends_pending_chat() {
    let mut app = on_empty_session();
    for c in "hello".chars() {
        press(&mut app, KeyCode::Char(c));
    }
    update(&mut app, key(KeyCode::Enter));
    assert!(sess(&app).creation.creating);

    let s = session();
    let id = s.id;
    let cmd = update(&mut app, Msg::SessionCreated(Ok(s)));

    let s = sess(&app);
    assert!(!s.creation.creating);
    assert!(s.session.is_some());
    assert_eq!(s.session.as_ref().unwrap().id, id);
    assert_eq!(s.session.as_ref().unwrap().messages.len(), 1);
    assert!(s.creation.pending_chat.is_none());
    assert!(s.streaming.is_some());
    assert!(matches!(cmd, Cmd::StartChat(_)));
}

/// Regression: the first chat request after a session is created must
/// carry the user message that triggered the create. The local
/// `session` binding from `Ok(session)` is the pre-push server response
/// (empty messages), so the request has to be built from the model,
/// not the local.
#[test]
fn first_chat_request_carries_user_message() {
    let mut app = on_empty_session();
    for c in "hello".chars() {
        press(&mut app, KeyCode::Char(c));
    }
    update(&mut app, key(KeyCode::Enter));

    let cmd = update(&mut app, Msg::SessionCreated(Ok(session())));
    match cmd {
        Cmd::StartChat(req) => {
            assert_eq!(req.messages.len(), 1, "user message must be in the request");
            assert_eq!(req.messages[0].role, Role::User);
        }
        other => panic!("expected StartChat, got {other:?}"),
    }
}

#[test]
fn session_created_failure_drops_creating_and_toasts() {
    let mut app = on_empty_session();
    for c in "hello".chars() {
        press(&mut app, KeyCode::Char(c));
    }
    update(&mut app, key(KeyCode::Enter));

    let cmd = update(
        &mut app,
        Msg::SessionCreated(Err(mewcode_client::runtime::model::CreateError::Other(
            "boom".into(),
        ))),
    );
    assert!(matches!(cmd, Cmd::None));
    let s = sess(&app);
    assert!(s.session.is_none());
    assert!(!s.creation.creating);
    assert!(s.creation.pending_chat.is_none());
    assert!(app.toast.is_some());
}

/// Regression for the data-loss path: when `POST /sessions` fails, the
/// user's typed text must stay in the composer so they can retry. The
/// previous flow cleared `s.input` on submit and then dropped
/// `pending_chat` on failure, leaving the user staring at an empty box.
#[test]
fn session_created_failure_preserves_input_for_retry() {
    let mut app = on_empty_session();
    for c in "retry me".chars() {
        press(&mut app, KeyCode::Char(c));
    }
    update(&mut app, key(KeyCode::Enter));

    update(
        &mut app,
        Msg::SessionCreated(Err(mewcode_client::runtime::model::CreateError::Other(
            "boom".into(),
        ))),
    );

    let s = sess(&app);
    assert!(s.session.is_none());
    assert!(!s.creation.creating);
    assert_eq!(s.input.lines().join("\n"), "retry me");
    assert!(
        s.creation.creation_started_at.is_none(),
        "spinner should stop"
    );
}

#[test]
fn creating_state_ignores_keypresses() {
    let mut app = on_empty_session();
    for c in "hello".chars() {
        press(&mut app, KeyCode::Char(c));
    }
    update(&mut app, key(KeyCode::Enter));
    let before_pending = sess(&app).creation.pending_chat.clone();
    let before_input = sess(&app).input.lines().join("\n");

    // All keypresses while creating should be ignored — pending_chat,
    // input, and the creating flag itself stay put.
    for c in "xyz".chars() {
        press(&mut app, KeyCode::Char(c));
    }
    let s = sess(&app);
    assert_eq!(s.creation.pending_chat, before_pending);
    assert_eq!(s.input.lines().join("\n"), before_input);
    assert!(s.creation.creating);
}

// --- existing-session flow ------------------------------------------------

fn press(app: &mut App, code: KeyCode) {
    update(app, Msg::Key(KeyEvent::new(code, KeyModifiers::NONE)));
}

fn type_chars(app: &mut App, s: &str) {
    for c in s.chars() {
        press(app, KeyCode::Char(c));
    }
}

/// Type a string into the Session input via key events, then return the app.
fn type_into_session(text: &str) -> App {
    let mut app = on_session();
    for c in text.chars() {
        update(&mut app, char_key(c));
    }
    app
}

#[test]
fn at_file_picker_inserts_selected_path() {
    let mut app = on_empty_session();
    type_chars(&mut app, "read ");

    assert!(matches!(update(&mut app, char_key('@')), Cmd::FetchFiles));
    update(
        &mut app,
        Msg::FilesFetched(Ok(vec![
            FileEntry {
                path: "README.md".to_string(),
                is_dir: false,
            },
            FileEntry {
                path: "src/main.rs".to_string(),
                is_dir: false,
            },
        ])),
    );
    type_chars(&mut app, "src");
    update(&mut app, key(KeyCode::Enter));

    let s = sess(&app);
    assert_eq!(s.overlay, Overlay::None);
    assert_eq!(s.input.lines().join("\n"), "read @src/main.rs");
}

#[test]
fn at_file_picker_prefers_basename_prefix_matches() {
    let mut app = on_empty_session();
    type_chars(&mut app, "read ");

    assert!(matches!(update(&mut app, char_key('@')), Cmd::FetchFiles));
    update(
        &mut app,
        Msg::FilesFetched(Ok(vec![
            FileEntry {
                path: "src/streaming.rs".to_string(),
                is_dir: false,
            },
            FileEntry {
                path: "crates/engine/src/tools/fs/read_file.rs".to_string(),
                is_dir: false,
            },
            FileEntry {
                path: "README.md".to_string(),
                is_dir: false,
            },
        ])),
    );
    type_chars(&mut app, "rea");
    update(&mut app, key(KeyCode::Enter));

    assert_eq!(sess(&app).input.lines().join("\n"), "read @README.md");
}

#[test]
fn at_file_picker_reopens_when_cursor_returns_to_mention() {
    let mut app = on_empty_session();

    assert!(matches!(update(&mut app, char_key('@')), Cmd::FetchFiles));
    update(
        &mut app,
        Msg::FilesFetched(Ok(vec![FileEntry {
            path: "README.md".to_string(),
            is_dir: false,
        }])),
    );
    type_chars(&mut app, "rea ");
    assert_eq!(sess(&app).overlay, Overlay::None);

    press(&mut app, KeyCode::Left);

    assert_eq!(sess(&app).overlay, Overlay::FilePicker);
}

#[test]
fn at_file_picker_hides_dotfiles_by_default() {
    let mut app = on_empty_session();

    assert!(matches!(update(&mut app, char_key('@')), Cmd::FetchFiles));
    update(
        &mut app,
        Msg::FilesFetched(Ok(vec![
            FileEntry {
                path: ".env".to_string(),
                is_dir: false,
            },
            FileEntry {
                path: "README.md".to_string(),
                is_dir: false,
            },
        ])),
    );
    update(&mut app, key(KeyCode::Enter));

    assert_eq!(sess(&app).input.lines().join("\n"), "@README.md");
}

#[test]
fn at_file_picker_shows_dotfiles_for_dot_query() {
    let mut app = on_empty_session();

    assert!(matches!(update(&mut app, char_key('@')), Cmd::FetchFiles));
    update(
        &mut app,
        Msg::FilesFetched(Ok(vec![
            FileEntry {
                path: ".env".to_string(),
                is_dir: false,
            },
            FileEntry {
                path: "README.md".to_string(),
                is_dir: false,
            },
        ])),
    );
    type_chars(&mut app, ".");
    update(&mut app, key(KeyCode::Enter));

    assert_eq!(sess(&app).input.lines().join("\n"), "@.env");
}

/// The text command `quit` (case-insensitive, exact match) is the new
/// way to exit. Surrounding whitespace is allowed; substrings like
/// "I want to quit" or "quit now" still go to the LLM.
#[test]
fn quit_command_exits_app() {
    let mut app = on_session();
    for c in "quit".chars() {
        update(&mut app, char_key(c));
    }
    let cmd = update(&mut app, key(KeyCode::Enter));
    assert!(
        matches!(cmd, Cmd::Quit),
        "typing `quit` + Enter must produce Cmd::Quit; got {cmd:?}"
    );
}

#[test]
fn ctrl_c_exits_app() {
    let mut app = on_session();

    assert!(matches!(update(&mut app, ctrl_key('c')), Cmd::Quit));
}

#[test]
fn multiline_paste_is_compacted_in_composer() {
    let mut app = on_session();

    update(&mut app, Msg::Paste("one\ntwo\nthree".to_string()));

    assert_eq!(sess(&app).input.lines().join("\n"), "[Pasted ~3 lines]");
}

#[test]
fn long_single_line_paste_is_compacted_in_composer() {
    let mut app = on_session();

    update(&mut app, Msg::Paste("x".repeat(121)));

    assert_eq!(sess(&app).input.lines().join("\n"), "[Pasted ~121 chars]");
}

#[test]
fn compacted_paste_submits_full_text() {
    let mut app = on_session();

    update(&mut app, Msg::Paste("one\ntwo\nthree".to_string()));
    match update(&mut app, key(KeyCode::Enter)) {
        Cmd::StartChat(req) => match &req.messages.last().unwrap().parts[0] {
            MessagePart::Text { text } => assert_eq!(text, "one\ntwo\nthree"),
            other => panic!("expected text message, got {other:?}"),
        },
        other => panic!("expected StartChat, got {other:?}"),
    }
}

#[test]
fn compacted_paste_starting_with_slash_is_not_command() {
    let mut app = on_session();

    update(&mut app, Msg::Paste("//! docs\nfn main() {}".to_string()));
    type_chars(&mut app, " tell me what file is it");

    match update(&mut app, key(KeyCode::Enter)) {
        Cmd::StartChat(req) => match &req.messages.last().unwrap().parts[0] {
            MessagePart::Text { text } => {
                assert_eq!(text, "//! docs\nfn main() {} tell me what file is it")
            }
            other => panic!("expected text message, got {other:?}"),
        },
        other => panic!("expected StartChat, got {other:?}"),
    }
}

#[test]
fn quit_command_is_case_insensitive() {
    for variant in ["QUIT", "Quit", "qUiT"] {
        let mut app = on_session();
        for c in variant.chars() {
            update(&mut app, char_key(c));
        }
        let cmd = update(&mut app, key(KeyCode::Enter));
        assert!(matches!(cmd, Cmd::Quit), "{variant:?} should quit");
    }
}

#[test]
fn quit_must_be_exact_match_not_substring() {
    for variant in ["I want to quit", "quit now", "quitting", "qu"] {
        let mut app = on_session();
        for c in variant.chars() {
            update(&mut app, char_key(c));
        }
        let cmd = update(&mut app, key(KeyCode::Enter));
        assert!(
            !matches!(cmd, Cmd::Quit),
            "{variant:?} should be sent as a normal message, not quit"
        );
    }
}

#[test]
fn slash_tools_opens_tools_overlay() {
    let mut app = type_into_session("/tools");
    assert!(matches!(update(&mut app, key(KeyCode::Enter)), Cmd::None));
    assert_eq!(sess(&app).overlay, Overlay::Tools);
    assert!(sess(&app).streaming.is_none());
}

#[test]
fn slash_skills_opens_skills_overlay_and_fetches_when_uncached() {
    let mut app = type_into_session("/skills");
    assert!(matches!(
        update(&mut app, key(KeyCode::Enter)),
        Cmd::FetchSkills
    ));
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
    assert_eq!(s.session.as_ref().unwrap().messages.len(), 1);
}

#[test]
fn submit_while_streaming_is_queued_not_dropped() {
    // A second submit while a turn is in flight must not orphan the
    // in-flight `StreamingState` — that would lose deltas and let a late
    // `Finished` commit garbage to history. Instead of being rejected, it
    // is queued and sent automatically once the in-flight turn finishes
    // (see `Msg::Stream`'s queue-draining branch).
    let mut app = type_into_session("first");
    update(&mut app, key(KeyCode::Enter));
    assert!(sess(&app).streaming.is_some());
    let before = sess(&app).session.as_ref().unwrap().messages.len();

    update(&mut app, char_key('s'));
    update(&mut app, char_key('e'));
    update(&mut app, char_key('c'));
    update(&mut app, char_key('o'));
    update(&mut app, char_key('n'));
    update(&mut app, char_key('d'));
    assert!(matches!(update(&mut app, key(KeyCode::Enter)), Cmd::None));

    let s = sess(&app);
    // No second user message committed yet, no second turn started yet.
    assert_eq!(s.session.as_ref().unwrap().messages.len(), before);
    // The composer is cleared — the message moved into the queue, not
    // left stuck in the input for the user to notice nothing happened.
    assert_eq!(s.input.lines().join("\n"), "");
    assert_eq!(s.message_queue.as_slice(), ["second"]);
}

// --- apply_stream_event cases --------------------------------------------

fn stream(app: &mut App, ev: StreamMsg) -> Cmd {
    update(app, Msg::Stream(ev))
}

use mewcode_client::runtime::model::StreamMsg;

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
    stream(&mut app, StreamMsg::Started { id, pwd: None });
    assert_eq!(sess(&app).streaming.as_ref().unwrap().assistant_id, id);
}

#[test]
fn stream_delta_accumulates_buffer() {
    let mut app = streaming_session();
    stream(&mut app, StreamMsg::Delta("Hel".to_string()));
    stream(&mut app, StreamMsg::Delta("lo".to_string()));
    assert_eq!(sess(&app).streaming.as_ref().unwrap().text(), "Hello");
}

#[test]
fn stream_tool_input_then_output_is_recorded() {
    use mewcode_client::runtime::model::TurnItem;
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
    let items = &sess(&app).streaming.as_ref().unwrap().items;
    let tools: Vec<_> = items
        .iter()
        .filter_map(|it| match it {
            TurnItem::Tool(v) => Some(v),
            TurnItem::Text(_) | TurnItem::Compaction(_) => None,
        })
        .collect();
    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0].name, "readFile");
    assert_eq!(tools[0].output, Some(serde_json::json!({"ok": true})));
}

#[test]
fn stream_preserves_interleaved_text_and_tool_order() {
    use mewcode_client::runtime::model::TurnItem;
    // Regression: text before and after a tool call must keep their order
    // (text -> tool -> text), not be collapsed into "all text then all tools".
    let mut app = streaming_session();
    stream(&mut app, StreamMsg::Delta("before ".to_string()));
    stream(
        &mut app,
        StreamMsg::ToolInput {
            id: "c1".to_string(),
            name: "bash".to_string(),
            input: serde_json::json!({"command": "ls"}),
        },
    );
    stream(&mut app, StreamMsg::Delta("after".to_string()));

    let items = &sess(&app).streaming.as_ref().unwrap().items;
    assert!(
        matches!(&items[0], TurnItem::Text(t) if t == "before "),
        "first item should be the leading text: {items:?}"
    );
    assert!(
        matches!(&items[1], TurnItem::Tool(v) if v.name == "bash"),
        "second item should be the tool call: {items:?}"
    );
    assert!(
        matches!(&items[2], TurnItem::Text(t) if t == "after"),
        "third item should be the trailing text (a new run): {items:?}"
    );
}

#[test]
fn stream_finished_commits_one_assistant_message() {
    let mut app = streaming_session();
    let before = sess(&app).session.as_ref().unwrap().messages.len();
    stream(
        &mut app,
        StreamMsg::Started {
            id: Uuid::new_v4(),
            pwd: None,
        },
    );
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
        stream(
            &mut app,
            StreamMsg::Finished {
                duration_ms: 12,
                session_tokens: None,
                context_limit: None
            }
        ),
        Cmd::PlayNotificationSound
    ));

    let s = sess(&app);
    assert!(s.streaming.is_none());
    assert_eq!(s.session.as_ref().unwrap().messages.len(), before + 1);

    let committed = s.session.as_ref().unwrap().messages.last().unwrap();
    assert_eq!(committed.role, Role::Assistant);
    // Text first, then the tool call, then its result.
    assert!(matches!(committed.parts[0], MessagePart::Text { .. }));
    assert!(matches!(committed.parts[1], MessagePart::ToolCall(_)));
    assert!(matches!(committed.parts[2], MessagePart::ToolResult(_)));
}

#[test]
fn stream_failed_discards_partial_and_toasts() {
    let mut app = streaming_session();
    let before = sess(&app).session.as_ref().unwrap().messages.len();
    stream(&mut app, StreamMsg::Delta("partial".to_string()));
    stream(&mut app, StreamMsg::Failed("boom".to_string()));
    let s = sess(&app);
    assert!(s.streaming.is_none());
    assert_eq!(s.session.as_ref().unwrap().messages.len(), before);
    assert!(app.toast.is_some());
}

#[test]
fn stream_event_without_streaming_is_ignored() {
    let mut app = on_session();
    assert!(sess(&app).streaming.is_none());
    let before = sess(&app).session.as_ref().unwrap().messages.len();
    stream(&mut app, StreamMsg::Delta("ignored".to_string()));
    stream(
        &mut app,
        StreamMsg::Finished {
            duration_ms: 1,
            session_tokens: None,
            context_limit: None,
        },
    );
    let s = sess(&app);
    assert!(s.streaming.is_none());
    assert_eq!(s.session.as_ref().unwrap().messages.len(), before);
    // Failed with no tracked turn raises no toast.
    assert!(app.toast.is_none());
}
