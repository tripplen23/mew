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
fn session_app() -> App {
    let mut app = App::new();
    app.screen = Screen::Session(SessionState::new(Session {
        id: Uuid::new_v4(),
        title: "demo".to_string(),
        model: ModelId::default(),
        mode: Mode::default(),
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
        messages: vec![],
        compaction_summary: None,
        compacted_up_to: None,
    }));
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
    }
}

fn message_count(app: &App) -> usize {
    session_state(app)
        .session
        .as_ref()
        .expect("test fixture should have a hydrated session")
        .messages
        .len()
}

fn assistant_count(app: &App) -> usize {
    session_state(app)
        .session
        .as_ref()
        .expect("test fixture should have a hydrated session")
        .messages
        .iter()
        .filter(|m| m.role == Role::Assistant)
        .count()
}

/// A non-terminal streaming event (everything except `Finished`/`Failed`).
fn middle_event() -> impl Strategy<Value = StreamMsg> {
    prop_oneof![
        any::<u128>().prop_map(|n| StreamMsg::Started {
            id: Uuid::from_u128(n),
            pwd: None
        }),
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

        update(&mut app, Msg::Stream(StreamMsg::Finished { duration_ms: 0, session_tokens: None, context_limit: None }));

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
        update(&mut app, Msg::Stream(StreamMsg::Finished { duration_ms: 0, session_tokens: None, context_limit: None }));

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
    update(
        &mut app,
        Msg::Stream(StreamMsg::Started {
            id: Uuid::new_v4(),
            pwd: None,
        }),
    );
    update(
        &mut app,
        Msg::Stream(StreamMsg::Delta("hello ".to_string())),
    );
    update(&mut app, Msg::Stream(StreamMsg::Delta("world".to_string())));
    update(
        &mut app,
        Msg::Stream(StreamMsg::Finished {
            duration_ms: 7,
            session_tokens: None,
            context_limit: None,
        }),
    );

    let s = session_state(&app);
    assert!(s.streaming.is_none());
    let last = s
        .session
        .as_ref()
        .unwrap()
        .messages
        .last()
        .expect("a committed message");
    assert_eq!(last.role, Role::Assistant);
    assert_eq!(
        last.parts,
        vec![mewcode_protocol::MessagePart::Text {
            text: "hello world".to_string()
        }]
    );
}

/// Regression test: a message submitted while `/compact` is in flight must
/// still be sent once compaction finishes, even when the notification sound
/// is enabled. Previously `Cmd::PlayNotificationSound` was returned instead
/// of `Cmd::StartChat` on this path, silently dropping the queued message
/// while showing it as already "sent" in the transcript.
#[test]
fn compacting_pending_message_is_sent_even_with_sound_enabled() {
    use mewcode_client::runtime::model::Cmd;

    let mut app = session_app();
    {
        let s = session_state_mut(&mut app);
        s.sound_enabled = true;
        s.compaction.active = true;
        s.message_queue.push("queued while compacting".to_string());
    }

    let cmd = update(
        &mut app,
        Msg::Stream(StreamMsg::Finished {
            duration_ms: 5,
            session_tokens: None,
            context_limit: None,
        }),
    );

    // Both the sound and the deferred chat must fire — neither may be dropped.
    let cmds = match cmd {
        Cmd::Batch(cmds) => cmds,
        other => panic!("expected Cmd::Batch(sound, start_chat), got {other:?}"),
    };
    assert!(
        cmds.iter().any(|c| matches!(c, Cmd::PlayNotificationSound)),
        "sound was dropped: {cmds:?}"
    );
    assert!(
        cmds.iter().any(|c| matches!(c, Cmd::StartChat(_))),
        "queued message was dropped: {cmds:?}"
    );

    // The queued text was already appended to history by `update`.
    let s = session_state(&app);
    assert!(s.message_queue.is_empty());
    let last = s
        .session
        .as_ref()
        .unwrap()
        .messages
        .last()
        .expect("queued message should be appended");
    assert_eq!(last.role, Role::User);
}

fn session_state_mut(app: &mut App) -> &mut SessionState {
    match &mut app.screen {
        Screen::Session(s) => s,
    }
}

/// Regression test: automatic compaction firing mid-turn during a *normal*
/// `/chat` stream (not the manual `/compact` route) must leave a permanent
/// `CompactionEntry` behind after the turn commits. Previously the
/// `Compacted` event was only ever persisted in the manual-compaction branch
/// of the `Finished` handler; a compaction that happened as a side effect of
/// a regular chat turn rendered live while streaming and then vanished with
/// no trace once `Finished` committed the assistant message.
#[test]
fn mid_turn_auto_compaction_survives_normal_finish() {
    let mut app = session_app();
    start_turn(&mut app);

    // `s.compaction.active` stays false for this path — it's a plain chat
    // stream, not the manual `/compact` route.
    assert!(!session_state(&app).compaction.active);

    update(
        &mut app,
        Msg::Stream(StreamMsg::Compacted {
            tokens_before: 12_345,
            context_limit: 200_000,
            summary: "auto-compacted mid-turn".to_string(),
            thought_duration_ms: 42,
        }),
    );
    update(
        &mut app,
        Msg::Stream(StreamMsg::Delta("here's my reply".to_string())),
    );
    update(
        &mut app,
        Msg::Stream(StreamMsg::Finished {
            duration_ms: 10,
            session_tokens: Some(500),
            context_limit: Some(200_000),
        }),
    );

    let s = session_state(&app);
    assert!(s.streaming.is_none());
    assert_eq!(
        s.compaction.committed.len(),
        1,
        "compaction entry must survive a normal (non-manual) turn finish"
    );
    assert_eq!(
        s.compaction.committed[0].view.summary,
        "auto-compacted mid-turn"
    );

    // The assistant's reply is still committed normally alongside it.
    let last = s
        .session
        .as_ref()
        .unwrap()
        .messages
        .last()
        .expect("assistant reply should still be committed");
    assert_eq!(last.role, Role::Assistant);
}

/// Regression test: the compaction summary must build up progressively from
/// streamed `CompactionSummaryDelta` chunks, the same way normal chat text
/// builds up from `Delta`, instead of only ever appearing as one block once
/// `Compacted` arrives. This is what makes `/compact` render like a live
/// reply rather than a silent pause followed by a single dump.
#[test]
fn compaction_summary_builds_up_from_streamed_deltas() {
    let mut app = session_app();
    {
        let s = session_state_mut(&mut app);
        s.compaction.active = true;
        s.compaction.started_at = Some(std::time::Instant::now());
    }

    update(&mut app, Msg::Stream(StreamMsg::CompactionStarted));
    // Deltas arrive progressively, mirroring how chat text streams in.
    update(
        &mut app,
        Msg::Stream(StreamMsg::CompactionSummaryDelta(
            "**Objective**\n".to_string(),
        )),
    );
    update(
        &mut app,
        Msg::Stream(StreamMsg::CompactionSummaryDelta(
            "- Ship the compaction feature.".to_string(),
        )),
    );

    // Mid-stream, the in-progress summary is already visible in the live
    // streaming state — not empty, not waiting for a single final dump.
    {
        let s = session_state(&app);
        let live_summary = s
            .streaming
            .as_ref()
            .and_then(|st| {
                st.items.iter().find_map(|item| match item {
                    mewcode_client::runtime::model::TurnItem::Compaction(view) => {
                        Some(view.summary.clone())
                    }
                    _ => None,
                })
            })
            .expect("a compaction item should exist mid-stream");
        assert_eq!(
            live_summary,
            "**Objective**\n- Ship the compaction feature."
        );
    }

    update(
        &mut app,
        Msg::Stream(StreamMsg::Compacted {
            tokens_before: 5000,
            context_limit: 200_000,
            summary: "**Objective**\n- Ship the compaction feature.".to_string(),
            thought_duration_ms: 1500,
        }),
    );
    update(
        &mut app,
        Msg::Stream(StreamMsg::Finished {
            duration_ms: 10,
            session_tokens: Some(100),
            context_limit: Some(200_000),
        }),
    );

    let s = session_state(&app);
    assert!(!s.compaction.active);
    assert_eq!(s.compaction.committed.len(), 1);
    // The committed entry keeps the progressively-streamed summary text and
    // picks up the metadata (tokens/duration) from the `Compacted` event.
    assert_eq!(
        s.compaction.committed[0].view.summary,
        "**Objective**\n- Ship the compaction feature."
    );
    assert_eq!(s.compaction.committed[0].view.tokens_before, 5000);
    assert_eq!(s.compaction.committed[0].view.thought_duration_ms, 1500);
}

/// Diagnostic test reproducing the exact user flow: type `/compact`, press
/// Enter (through the slash picker, exactly like a real keypress sequence),
/// then while compaction is in flight type "hello" and press Enter. The
/// message must be queued into `message_queue`, not silently dropped or
/// left stuck in the composer.
#[test]
fn compact_command_then_message_is_queued_not_dropped() {
    use mewcode_client::runtime::model::Cmd;

    let mut app = session_app();

    // Type "/compact" character by character, the way a real user would,
    // so the slash picker opens exactly like it does in the TUI.
    for c in "/compact".chars() {
        update(&mut app, Msg::Key(KeyEvent::from(KeyCode::Char(c))));
    }
    let s = session_state(&app);
    assert_eq!(
        s.overlay,
        mewcode_client::runtime::model::Overlay::SlashPicker,
        "typing /compact should open the slash picker"
    );

    // Press Enter to submit /compact.
    let cmd = update(&mut app, Msg::Key(KeyEvent::from(KeyCode::Enter)));
    assert!(
        matches!(cmd, Cmd::Compact(_)),
        "expected Cmd::Compact, got {cmd:?}"
    );

    let s = session_state(&app);
    assert!(s.compaction.active, "compaction.active must be set");
    assert_eq!(
        s.overlay,
        mewcode_client::runtime::model::Overlay::None,
        "overlay must close after submitting /compact"
    );

    // Simulate the SSE events the manual /compact route actually sends,
    // in order, exactly as `run_compact_stream` forwards them.
    update(&mut app, Msg::Stream(StreamMsg::CompactionStarted));
    update(
        &mut app,
        Msg::Stream(StreamMsg::CompactionProgress {
            phase: mewcode_protocol::event::CompactionPhase::Pruning,
            message: "Pruning tool results and low-value content...".to_string(),
        }),
    );
    update(
        &mut app,
        Msg::Stream(StreamMsg::CompactionProgress {
            phase: mewcode_protocol::event::CompactionPhase::Summarizing,
            message: "Running LLM to summarize history...".to_string(),
        }),
    );

    // While compacting is still in flight, the user types "hello" and
    // presses Enter.
    for c in "hello".chars() {
        update(&mut app, Msg::Key(KeyEvent::from(KeyCode::Char(c))));
    }
    let cmd = update(&mut app, Msg::Key(KeyEvent::from(KeyCode::Enter)));

    let s = session_state(&app);
    assert!(
        matches!(cmd, Cmd::None),
        "queueing while compacting should not dispatch anything yet, got {cmd:?}"
    );
    assert_eq!(
        s.message_queue.as_slice(),
        ["hello"],
        "the message typed during compaction must be queued, not dropped"
    );
    assert_eq!(
        s.input.lines().join("\n"),
        "",
        "composer must be cleared once the message is queued"
    );
}

/// Diagnostic test: after a manual `/compact` fully completes (all the way
/// through `Finish`), the composer must accept a new message normally — no
/// leftover `streaming`/`compaction.active` state should block it. This
/// reproduces the reported bug: after `/compact` finishes and its summary
/// is fully rendered, typing "hello" and pressing Enter does nothing.
#[test]
fn message_after_compact_fully_finishes_is_sent_normally() {
    use mewcode_client::runtime::model::Cmd;

    let mut app = session_app();
    for c in "/compact".chars() {
        update(&mut app, Msg::Key(KeyEvent::from(KeyCode::Char(c))));
    }
    update(&mut app, Msg::Key(KeyEvent::from(KeyCode::Enter)));

    // Full real event sequence, in order, exactly as the manual /compact
    // route sends them.
    update(&mut app, Msg::Stream(StreamMsg::CompactionStarted));
    update(
        &mut app,
        Msg::Stream(StreamMsg::CompactionProgress {
            phase: mewcode_protocol::event::CompactionPhase::Pruning,
            message: "Pruning...".to_string(),
        }),
    );
    update(
        &mut app,
        Msg::Stream(StreamMsg::CompactionProgress {
            phase: mewcode_protocol::event::CompactionPhase::Summarizing,
            message: "Summarizing...".to_string(),
        }),
    );
    update(
        &mut app,
        Msg::Stream(StreamMsg::CompactionSummaryDelta(
            "**Objective**\n".to_string(),
        )),
    );
    update(
        &mut app,
        Msg::Stream(StreamMsg::CompactionSummaryDelta("- test".to_string())),
    );
    update(
        &mut app,
        Msg::Stream(StreamMsg::Compacted {
            tokens_before: 100,
            context_limit: 200_000,
            summary: "**Objective**\n- test".to_string(),
            thought_duration_ms: 4000,
        }),
    );
    update(
        &mut app,
        Msg::Stream(StreamMsg::CompactionProgress {
            phase: mewcode_protocol::event::CompactionPhase::Done,
            message: "Compaction complete.".to_string(),
        }),
    );
    update(
        &mut app,
        Msg::Stream(StreamMsg::Finished {
            duration_ms: 4000,
            session_tokens: Some(50),
            context_limit: Some(200_000),
        }),
    );

    let s = session_state(&app);
    assert!(
        !s.compaction.active,
        "compaction.active must be false after Finished"
    );
    assert!(
        s.streaming.is_none(),
        "streaming must be cleared after Finished — got {:?}",
        s.streaming
    );
    assert_eq!(s.compaction.committed.len(), 1);

    // Now type "hello" and press Enter, exactly like the reported bug.
    for c in "hello".chars() {
        update(&mut app, Msg::Key(KeyEvent::from(KeyCode::Char(c))));
    }
    let cmd = update(&mut app, Msg::Key(KeyEvent::from(KeyCode::Enter)));

    assert!(
        matches!(cmd, Cmd::StartChat(_)),
        "expected Cmd::StartChat after compaction fully finished, got {cmd:?}"
    );
    let s = session_state(&app);
    assert_eq!(
        s.input.lines().join("\n"),
        "",
        "composer must be cleared once the message is sent"
    );
}
