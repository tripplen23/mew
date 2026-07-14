//! Unit tests for the filesystem-backed [`SessionStore`] implementation.
//!
//! Exercises the real [`FsStore`] backend over a `tempfile` throwaway data
//! dir so the suite is unconditional in CI (no `#[ignore]`, no env gate) and
//! never touches the user's real data directory.
//!
//! Covers create -> read-back (Property 2), delete -> `NotFound` (Property 3),
//! cascade delete of messages (Property 4), message ordering by `created_at`
//! (Property 5), and `append_message` bumping `updated_at`.

use chrono::{Duration, Utc};
use mewcode_protocol::{Message, MessagePart, Mode, ModelId, Role};
use mewcode_server::store::fs::FsStore;
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

/// Build an `FsStore` rooted at a fresh throwaway dir. The returned `TempDir`
/// guard must be kept alive for the duration of the test (drop deletes it).
fn temp_store() -> (tempfile::TempDir, FsStore) {
    let tmp = tempfile::tempdir().expect("tempdir should be created");
    let store = FsStore::new(tmp.path().to_path_buf()).expect("store should be constructed");
    (tmp, store)
}

#[tokio::test]
async fn backend_reports_filesystem() {
    let (_tmp, store) = temp_store();
    assert_eq!(store.backend(), Backend::Filesystem);
}

/// Property 2: a created session reads back with identical metadata and an
/// empty message history.
#[tokio::test]
async fn create_then_get_round_trip() {
    let (_tmp, store) = temp_store();

    let created = store
        .create_session(new_session("hello"))
        .await
        .expect("create should succeed");

    assert_eq!(created.title, "hello");
    assert_eq!(created.model, ModelId::default());
    assert_eq!(created.mode, Mode::default());
    assert!(created.messages.is_empty());

    let fetched = store
        .get_session(created.id)
        .await
        .expect("get should succeed");

    assert_eq!(fetched.id, created.id);
    assert_eq!(fetched.title, created.title);
    assert_eq!(fetched.model, created.model);
    assert_eq!(fetched.mode, created.mode);
    assert!(fetched.messages.is_empty());
}

/// Property 3: deleting a session makes subsequent reads return `NotFound`.
#[tokio::test]
async fn delete_removes_session_then_get_not_found() {
    let (_tmp, store) = temp_store();
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

/// Property 4: deleting a session cascades to its messages — appending after a
/// delete fails with `NotFound`, and a same-id session would not resurrect the
/// old history.
#[tokio::test]
async fn delete_cascades_to_messages() {
    let (_tmp, store) = temp_store();
    let created = store
        .create_session(new_session("cascade"))
        .await
        .expect("create should succeed");

    store
        .append_message(created.id, message_at("first", Utc::now()))
        .await
        .expect("append should succeed");

    store
        .delete_session(created.id)
        .await
        .expect("delete should succeed");

    // The session directory (meta + messages) is gone, so a further append
    // against the same id fails rather than re-creating an orphaned log.
    let err = store
        .append_message(created.id, message_at("ghost", Utc::now()))
        .await
        .expect_err("append to deleted session should error");
    assert!(matches!(err, StoreError::NotFound));

    let err = store
        .get_session(created.id)
        .await
        .expect_err("deleted session messages should be gone");
    assert!(matches!(err, StoreError::NotFound));
}

/// Property 5: `get_session` returns messages ordered by `created_at`
/// ascending, even when appended out of chronological order.
#[tokio::test]
async fn get_session_orders_messages_by_created_at_ascending() {
    let (_tmp, store) = temp_store();
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

/// Appending a message advances `updated_at` while leaving `created_at`
/// untouched.
#[tokio::test]
async fn append_message_bumps_updated_at() {
    let (_tmp, store) = temp_store();
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
