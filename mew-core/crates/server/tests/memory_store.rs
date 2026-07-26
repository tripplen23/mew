//! Unit tests for the in-memory [`SessionStore`] implementation.
//!
//! Covers the create -> get round-trip, delete semantics (`NotFound`),
//! `append_message` bumping `updated_at`, and message ordering by `created_at`.

use chrono::{Duration, Utc};
use mewcode_protocol::{Message, MessagePart, Mode, ModelId, Role};
use mewcode_server::store::memory::MemoryStore;
use mewcode_server::store::{Backend, NewSession, SessionPatch, SessionStore, StoreError};

/// Build a `NewSession` with the given title and sensible defaults.
fn new_session(title: &str) -> NewSession {
    NewSession {
        title: title.to_string(),
        model: ModelId::default(),
        mode: Mode::default(),
    }
}

/// Build a user message with an explicit `created_at` and text body.
fn message_at(text: &str, created_at: chrono::DateTime<Utc>) -> Message {
    Message {
        id: uuid::Uuid::new_v4(),
        role: Role::User,
        parts: vec![MessagePart::Text {
            text: text.to_string(),
        }],
        model: None,
        created_at,
    }
}

#[tokio::test]
async fn backend_reports_memory() {
    let store = MemoryStore::new();
    assert_eq!(store.backend(), Backend::Memory);
}

#[tokio::test]
async fn create_then_get_round_trip() {
    let store = MemoryStore::new();

    let created = store
        .create_session(new_session("hello"))
        .await
        .expect("create should succeed");

    assert_eq!(created.title, "hello");
    assert_eq!(created.model, ModelId::default());
    assert_eq!(created.mode, Mode::default());
    assert!(created.messages.is_empty());
    assert_eq!(created.created_at, created.updated_at);

    let fetched = store
        .get_session(created.id)
        .await
        .expect("get should succeed");

    assert_eq!(fetched.id, created.id);
    assert_eq!(fetched.title, created.title);
    assert_eq!(fetched.model, created.model);
    assert_eq!(fetched.mode, created.mode);
    assert_eq!(fetched.created_at, created.created_at);
    assert!(fetched.messages.is_empty());
}

#[tokio::test]
async fn get_missing_id_returns_not_found() {
    let store = MemoryStore::new();
    let err = store
        .get_session(uuid::Uuid::new_v4())
        .await
        .expect_err("missing id should error");
    assert!(matches!(err, StoreError::NotFound));
}

#[tokio::test]
async fn compaction_patch_for_missing_session_returns_not_found() {
    let store = MemoryStore::new();
    let error = store
        .patch_session(
            uuid::Uuid::new_v4(),
            SessionPatch {
                compaction_summary: Some("summary".into()),
                compacted_up_to: Some(1),
                compacted_up_to_message_id: Some(uuid::Uuid::new_v4()),
                ..Default::default()
            },
        )
        .await
        .expect_err("missing session should win over checkpoint validation");

    assert!(matches!(error, StoreError::NotFound));
}

#[tokio::test]
async fn delete_removes_session_then_get_not_found() {
    let store = MemoryStore::new();
    let created = store
        .create_session(new_session("doomed"))
        .await
        .expect("create should succeed");

    store
        .delete_session(created.id)
        .await
        .expect("delete should succeed");

    let err = store
        .get_session(created.id)
        .await
        .expect_err("deleted session should be gone");
    assert!(matches!(err, StoreError::NotFound));
}

#[tokio::test]
async fn delete_missing_id_returns_not_found() {
    let store = MemoryStore::new();
    let err = store
        .delete_session(uuid::Uuid::new_v4())
        .await
        .expect_err("missing id should error");
    assert!(matches!(err, StoreError::NotFound));
}

#[tokio::test]
async fn append_to_missing_session_returns_not_found() {
    let store = MemoryStore::new();
    let err = store
        .append_message(uuid::Uuid::new_v4(), message_at("hi", Utc::now()))
        .await
        .expect_err("append to missing session should error");
    assert!(matches!(err, StoreError::NotFound));
}

#[tokio::test]
async fn append_message_bumps_updated_at() {
    let store = MemoryStore::new();
    let created = store
        .create_session(new_session("chatty"))
        .await
        .expect("create should succeed");

    // Ensure a strictly later wall-clock instant for the append.
    tokio::time::sleep(std::time::Duration::from_millis(2)).await;

    store
        .append_message(created.id, message_at("first", Utc::now()))
        .await
        .expect("append should succeed");

    let fetched = store
        .get_session(created.id)
        .await
        .expect("get should succeed");

    assert_eq!(fetched.messages.len(), 1);
    assert!(
        fetched.updated_at > created.updated_at,
        "updated_at should advance after append: {} !> {}",
        fetched.updated_at,
        created.updated_at
    );
    // created_at is immutable across an append.
    assert_eq!(fetched.created_at, created.created_at);
}

#[tokio::test]
async fn get_session_preserves_message_append_order() {
    let store = MemoryStore::new();
    let created = store
        .create_session(new_session("ordered"))
        .await
        .expect("create should succeed");

    let base = Utc::now();
    // Append out of chronological order on purpose.
    let m_late = message_at("late", base + Duration::seconds(30));
    let m_early = message_at("early", base);
    let m_mid = message_at("mid", base + Duration::seconds(10));

    store
        .append_message(created.id, m_late.clone())
        .await
        .unwrap();
    store
        .append_message(created.id, m_early.clone())
        .await
        .unwrap();
    store
        .append_message(created.id, m_mid.clone())
        .await
        .unwrap();

    let fetched = store
        .get_session(created.id)
        .await
        .expect("get should succeed");

    let order: Vec<uuid::Uuid> = fetched.messages.iter().map(|m| m.id).collect();
    assert_eq!(order, vec![m_late.id, m_early.id, m_mid.id]);
}

#[tokio::test]
async fn list_sessions_returns_summaries_newest_first() {
    let store = MemoryStore::new();
    let first = store
        .create_session(new_session("first"))
        .await
        .expect("create should succeed");
    let second = store
        .create_session(new_session("second"))
        .await
        .expect("create should succeed");

    let summaries = store.list_sessions().await.expect("list should succeed");
    let ids: Vec<uuid::Uuid> = summaries.iter().map(|s| s.id).collect();
    assert_eq!(ids, vec![second.id, first.id]);
}

#[tokio::test]
async fn compaction_anchor_round_trips() {
    let store = MemoryStore::new();
    let created = store
        .create_session(new_session("checkpointed"))
        .await
        .expect("create should succeed");
    let anchor_message = message_at("covered", Utc::now());
    let anchor = anchor_message.id;
    store
        .append_message(created.id, anchor_message)
        .await
        .expect("anchor message should persist");

    let patched = store
        .patch_session(
            created.id,
            SessionPatch {
                compaction_summary: Some("summary".into()),
                compacted_up_to: Some(1),
                compacted_up_to_message_id: Some(anchor),
                ..Default::default()
            },
        )
        .await
        .expect("patch should succeed");

    assert_eq!(patched.compacted_up_to_message_id, Some(anchor));
    let fetched = store
        .get_session(created.id)
        .await
        .expect("get should succeed");
    assert_eq!(fetched.compacted_up_to_message_id, Some(anchor));

    let error = store
        .patch_session(
            created.id,
            SessionPatch {
                compaction_summary: Some("incomplete".into()),
                ..Default::default()
            },
        )
        .await
        .expect_err("partial checkpoint must be rejected");
    assert!(matches!(error, StoreError::Invalid(_)));

    let cleared = store
        .patch_session(
            created.id,
            SessionPatch {
                compaction_summary: Some(String::new()),
                ..Default::default()
            },
        )
        .await
        .expect("empty summary should clear the checkpoint");
    assert!(cleared.compaction_summary.is_none());
    assert!(cleared.compacted_up_to.is_none());
    assert!(cleared.compacted_up_to_message_id.is_none());
}

#[tokio::test]
async fn compaction_checkpoint_must_match_memory_transcript() {
    let store = MemoryStore::new();
    let created = store
        .create_session(new_session("validated checkpoint"))
        .await
        .expect("create should succeed");
    let message = message_at("covered", Utc::now());
    store
        .append_message(created.id, message.clone())
        .await
        .expect("message should persist");

    for (up_to, message_id) in [(2, message.id), (1, uuid::Uuid::new_v4())] {
        let error = store
            .patch_session(
                created.id,
                SessionPatch {
                    compaction_summary: Some("summary".into()),
                    compacted_up_to: Some(up_to),
                    compacted_up_to_message_id: Some(message_id),
                    ..Default::default()
                },
            )
            .await
            .expect_err("checkpoint must reference its exact transcript boundary");
        assert!(matches!(error, StoreError::Invalid(_)));
    }
}
