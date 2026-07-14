//! Unit tests for the in-memory [`SessionStore`] implementation.
//!
//! Covers the create -> get round-trip, delete semantics (`NotFound`),
//! `append_message` bumping `updated_at`, and message ordering by `created_at`.

use chrono::{Duration, Utc};
use mewcode_protocol::{Message, MessagePart, Mode, ModelId, Role};
use mewcode_server::store::memory::MemoryStore;
use mewcode_server::store::{Backend, NewSession, SessionStore, StoreError};

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
async fn get_session_orders_messages_by_created_at_ascending() {
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
    assert_eq!(order, vec![m_early.id, m_mid.id, m_late.id]);
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
