//! Property 6: Stream commit.
//!
//! Exercised through the public `update` path (no terminal, no network):
//!
//! - For any SSE sequence ending in `Finished`, exactly one assistant message
//!   is appended and `streaming` returns to `None`.
//! - A `Failed` terminal event discards the partial buffer and keeps the
//!   existing history.
//! - Events that arrive while no `StreamingState` exists are ignored.

use crossterm::event::{KeyCode, KeyEvent};
use proptest::prelude::*;
use uuid::Uuid;

use mewcode_client::net::Session;
use mewcode_client::runtime::model::{App, Msg, Screen, SessionState, StreamMsg};
use mewcode_client::runtime::update;
use mewcode_protocol::{Mode, ModelId, Role};

/// A blank app whose current screen is a hydrated, empty `Session`.
///
/// Reaching the Session screen goes through the real `update` path
/// (`Msg::SessionOpened`), so the test never reaches into private state.
fn session_app() -> App {
    let mut app = App::new();
    let session = Session {
        id: Uuid::new_v4(),
        title: "demo".to_string(),
        model: ModelId::default(),
        mode: Mode::default(),
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
        messages: vec![],
    };
    update(&mut app, Msg::SessionOpened(Ok(session)));
    assert!(matches!(app.screen, Screen::Session(_)));
    app
}

/// Start an assistant turn the way a user would: type a character, press Enter.
/// This appends the user message and puts a single `StreamingState` in flight.
fn start_turn(app: &mut App) {
    update(app, Msg::Key(KeyEvent::from(KeyCode::Char('h'))));
    update(app, Msg::Key(KeyEvent::from(KeyCode::Enter)));
    assert!(session_state(app).streaming.is_some());
}

fn session_state(app: &App) -> &SessionState {
    match &app.screen {
        Screen::Session(s) => s,
        _ => panic!("expected Session screen"),
    }
}

fn message_count(app: &App) -> usize {
    session_state(app).session.messages.len()
}

fn assistant_count(app: &App) -> usize {
    session_state(app)
        .session
        .messages
        .iter()
        .filter(|m| m.role == Role::Assistant)
        .count()
}

/// A non-terminal streaming event (everything except `Finished`/`Failed`).
fn middle_event() -> impl Strategy<Value = StreamMsg> {
    prop_oneof![
        any::<u128>().prop_map(|n| StreamMsg::Started(Uuid::from_u128(n))),
        ".*".prop_map(StreamMsg::Delta),
        (".*", ".*").prop_map(|(id, name)| StreamMsg::ToolInput {
            id,
            name,
            input: serde_json::Value::Null,
        }),
        ".*".prop_map(|id| StreamMsg::ToolOutput {
            id,
            output: serde_json::Value::Null,
        }),
    ]
}

proptest! {
    /// Any in-flight sequence ending in `Finished` commits exactly one
    /// assistant message and clears the streaming state.
    #[test]
    fn finish_commits_exactly_one_assistant(events in prop::collection::vec(middle_event(), 0..12)) {
        let mut app = session_app();
        start_turn(&mut app);
        let base_total = message_count(&app);
        let base_assistant = assistant_count(&app);

        for ev in events {
            update(&mut app, Msg::Stream(ev));
        }
        // No message is committed before `Finished`.
        prop_assert_eq!(message_count(&app), base_total);

        update(&mut app, Msg::Stream(StreamMsg::Finished { duration_ms: 0 }));

        let s = session_state(&app);
        prop_assert!(s.streaming.is_none());
        prop_assert_eq!(assistant_count(&app), base_assistant + 1);
        prop_assert_eq!(message_count(&app), base_total + 1);
    }

    /// A `Failed` terminal event discards the partial buffer, commits no
    /// assistant message, and leaves history untouched.
    #[test]
    fn failed_discards_buffer_keeps_history(events in prop::collection::vec(middle_event(), 0..12)) {
        let mut app = session_app();
        start_turn(&mut app);
        let base_total = message_count(&app);
        let base_assistant = assistant_count(&app);

        for ev in events {
            update(&mut app, Msg::Stream(ev));
        }
        update(&mut app, Msg::Stream(StreamMsg::Failed("boom".to_string())));

        let s = session_state(&app);
        prop_assert!(s.streaming.is_none());
        prop_assert_eq!(message_count(&app), base_total);
        prop_assert_eq!(assistant_count(&app), base_assistant);
    }

    /// With no `StreamingState`, every streaming event — including a terminal
    /// `Finished` — is ignored and history is unchanged.
    #[test]
    fn events_without_streaming_state_are_ignored(events in prop::collection::vec(middle_event(), 0..12)) {
        let mut app = session_app();
        // No `start_turn`: streaming is None.
        prop_assert!(session_state(&app).streaming.is_none());
        let base_total = message_count(&app);

        for ev in events {
            update(&mut app, Msg::Stream(ev));
        }
        update(&mut app, Msg::Stream(StreamMsg::Finished { duration_ms: 0 }));

        let s = session_state(&app);
        prop_assert!(s.streaming.is_none());
        prop_assert_eq!(message_count(&app), base_total);
    }
}

/// Example-based check: a Started+Delta+Finished sequence commits the buffered
/// text as a single assistant message.
#[test]
fn finish_commits_buffered_text() {
    let mut app = session_app();
    start_turn(&mut app);
    update(&mut app, Msg::Stream(StreamMsg::Started(Uuid::new_v4())));
    update(
        &mut app,
        Msg::Stream(StreamMsg::Delta("hello ".to_string())),
    );
    update(&mut app, Msg::Stream(StreamMsg::Delta("world".to_string())));
    update(
        &mut app,
        Msg::Stream(StreamMsg::Finished { duration_ms: 7 }),
    );

    let s = session_state(&app);
    assert!(s.streaming.is_none());
    let last = s.session.messages.last().expect("a committed message");
    assert_eq!(last.role, Role::Assistant);
    assert_eq!(
        last.parts,
        vec![mewcode_protocol::MessagePart::Text {
            text: "hello world".to_string()
        }]
    );
}
