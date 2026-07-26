use std::collections::HashMap;

use mewcode_protocol::{Message, MessagePart, Role, StreamEvent};
use mewcode_server::services::chat::{
    CommitTurnError, canonical_turn_messages, commit_successful_turn, stage_harness_event,
    try_forward_event,
};
use mewcode_server::store::{NewSession, SessionStore, memory::MemoryStore};
use tokio::sync::mpsc;

fn text_message(role: Role, text: &str) -> Message {
    Message {
        id: uuid::Uuid::new_v4(),
        role,
        parts: vec![MessagePart::Text { text: text.into() }],
        model: None,
        created_at: chrono::Utc::now(),
    }
}

fn finish_event() -> StreamEvent {
    StreamEvent::Finish {
        duration_ms: 1,
        input_tokens: None,
        output_tokens: None,
        session_tokens: Some(10),
        context_limit: Some(100),
    }
}

#[test]
fn canonical_turn_uses_stored_history_and_only_the_new_user_message() {
    let stored = text_message(Role::Assistant, "stored");
    let forged_old = text_message(Role::Assistant, "client copy");
    let new_user = text_message(Role::User, "new request");

    let (messages, accepted_user) =
        canonical_turn_messages(vec![stored.clone()], &[forged_old, new_user.clone()])
            .expect("trailing user message should be accepted");

    assert_eq!(accepted_user.id, new_user.id);
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0].id, stored.id);
    assert_eq!(messages[1].id, new_user.id);
}

#[test]
fn canonical_turn_rejects_missing_or_replayed_user_message() {
    let existing = text_message(Role::User, "already stored");
    assert!(canonical_turn_messages(vec![], &[]).is_err());
    assert!(
        canonical_turn_messages(vec![existing.clone()], &[existing]).is_err(),
        "a client must not replay an existing message id as a new turn"
    );
}

#[test]
fn finish_is_staged_instead_of_forwarded() {
    let (tx, mut rx) = mpsc::channel(4);
    let mut client = Some(tx);
    let mut reply = String::new();
    let mut assistant_message_id = None;
    let mut finish = None;
    let mut engine_failed = false;
    let event = finish_event();

    stage_harness_event(
        event.clone(),
        &mut reply,
        &mut assistant_message_id,
        &mut finish,
        &mut engine_failed,
        &mut client,
    );

    assert!(!engine_failed);
    assert_eq!(finish, Some(event));
    assert!(matches!(
        rx.try_recv(),
        Err(mpsc::error::TryRecvError::Empty)
    ));
}

#[test]
fn harness_error_is_sanitized_and_not_forwarded() {
    let (tx, mut rx) = mpsc::channel(4);
    let mut client = Some(tx);
    let mut reply = String::new();
    let mut assistant_message_id = None;
    let mut finish = None;
    let mut engine_failed = false;

    stage_harness_event(
        StreamEvent::Error {
            message: "/secret/provider/error".into(),
        },
        &mut reply,
        &mut assistant_message_id,
        &mut finish,
        &mut engine_failed,
        &mut client,
    );

    assert!(engine_failed);
    assert!(matches!(
        rx.try_recv(),
        Err(mpsc::error::TryRecvError::Empty)
    ));
}

#[tokio::test]
async fn full_client_channel_is_dropped_without_holding_operation_lock() {
    let operation_lock = tokio::sync::Mutex::new(());
    let guard = operation_lock.lock().await;
    let (tx, _rx) = mpsc::channel(1);
    tx.try_send(StreamEvent::TextDelta {
        delta: "already full".into(),
    })
    .expect("prefill should succeed");
    let mut client = Some(tx);

    try_forward_event(
        &mut client,
        StreamEvent::TextDelta {
            delta: "must be dropped".into(),
        },
    );

    assert!(client.is_none(), "stalled client sender should be dropped");
    drop(guard);
    assert!(operation_lock.try_lock().is_ok());
}

#[tokio::test]
async fn successful_turn_commit_persists_before_returning_finish() {
    let store = MemoryStore::new();
    let session = store
        .create_session(NewSession {
            title: "chat".into(),
            model: mewcode_protocol::ModelId::default(),
            mode: mewcode_protocol::Mode::default(),
        })
        .await
        .expect("session should be created");
    let tokens = tokio::sync::RwLock::new(HashMap::new());
    let message_id = uuid::Uuid::new_v4();
    let finish = finish_event();

    let returned = commit_successful_turn(
        &store,
        &tokens,
        session.id,
        mewcode_protocol::ModelId::default(),
        "reply".into(),
        Some(message_id),
        finish.clone(),
        10,
        None,
    )
    .await
    .expect("commit should succeed");

    assert_eq!(returned, finish);
    let persisted = store.get_session(session.id).await.expect("reload session");
    assert_eq!(
        persisted.messages.last().map(|message| message.id),
        Some(message_id)
    );
    assert_eq!(tokens.read().await.get(&session.id), Some(&10));
}

#[tokio::test]
async fn successful_empty_reply_still_persists_assistant_message() {
    let store = MemoryStore::new();
    let session = store
        .create_session(NewSession {
            title: "chat".into(),
            model: mewcode_protocol::ModelId::default(),
            mode: mewcode_protocol::Mode::default(),
        })
        .await
        .expect("session should be created");
    let tokens = tokio::sync::RwLock::new(HashMap::new());
    let message_id = uuid::Uuid::new_v4();

    commit_successful_turn(
        &store,
        &tokens,
        session.id,
        mewcode_protocol::ModelId::default(),
        String::new(),
        Some(message_id),
        finish_event(),
        0,
        None,
    )
    .await
    .expect("empty assistant reply should still commit");

    let persisted = store.get_session(session.id).await.expect("reload session");
    assert_eq!(
        persisted.messages.last().map(|message| message.id),
        Some(message_id)
    );
}

#[tokio::test]
async fn missing_assistant_id_does_not_persist_checkpoint() {
    let store = MemoryStore::new();
    let session = store
        .create_session(NewSession {
            title: "chat".into(),
            model: mewcode_protocol::ModelId::default(),
            mode: mewcode_protocol::Mode::default(),
        })
        .await
        .expect("session should be created");
    let anchor = text_message(Role::User, "anchor");
    store
        .append_message(session.id, anchor.clone())
        .await
        .expect("anchor should persist");
    let tokens = tokio::sync::RwLock::new(HashMap::new());

    let result = commit_successful_turn(
        &store,
        &tokens,
        session.id,
        mewcode_protocol::ModelId::default(),
        "reply".into(),
        None,
        finish_event(),
        10,
        Some(("summary", 1, anchor.id)),
    )
    .await;

    assert!(matches!(
        result,
        Err(CommitTurnError::MissingAssistantMessageId)
    ));
    let persisted = store.get_session(session.id).await.expect("reload session");
    assert!(persisted.compaction_summary.is_none());
    assert_eq!(persisted.messages.len(), 1);
}

#[tokio::test]
async fn failed_turn_commit_does_not_return_finish_or_update_tokens() {
    let store = MemoryStore::new();
    let session = store
        .create_session(NewSession {
            title: "chat".into(),
            model: mewcode_protocol::ModelId::default(),
            mode: mewcode_protocol::Mode::default(),
        })
        .await
        .expect("session should be created");
    let tokens = tokio::sync::RwLock::new(HashMap::from([(session.id, 99)]));

    let result = commit_successful_turn(
        &store,
        &tokens,
        session.id,
        mewcode_protocol::ModelId::default(),
        "reply".into(),
        Some(uuid::Uuid::new_v4()),
        finish_event(),
        10,
        Some(("summary", 99, uuid::Uuid::new_v4())),
    )
    .await;

    assert!(result.is_err());
    assert_eq!(tokens.read().await.get(&session.id), Some(&99));
    assert!(
        store
            .get_session(session.id)
            .await
            .expect("reload session")
            .messages
            .is_empty(),
        "checkpoint failure must not leave a partial assistant commit"
    );
}
