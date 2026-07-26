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

/// Property 5: `get_session` preserves append order even when message
/// timestamps are out of order.
#[tokio::test]
async fn get_session_preserves_message_append_order() {
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
    assert_eq!(order, vec![m_late.id, m_early.id, m_mid.id]);
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

/// Regression test: a session directory whose `meta.json` no longer parses
/// must be skipped, not fail the wholel listing.
/// Every other, still-valid session must still be returned.
#[tokio::test]
async fn compaction_anchor_round_trips_and_legacy_metadata_still_loads() {
    let (_tmp, store) = temp_store();
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

    store
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
    let fetched = store
        .get_session(created.id)
        .await
        .expect("get should succeed");
    assert_eq!(fetched.compacted_up_to_message_id, Some(anchor));

    let meta_path = store
        .data_dir()
        .join("sessions")
        .join(created.id.to_string())
        .join("meta.json");
    let mut metadata: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(&meta_path).expect("metadata should be readable"),
    )
    .expect("metadata should be JSON");
    metadata
        .as_object_mut()
        .expect("metadata should be an object")
        .remove("compacted_up_to_message_id");
    std::fs::write(
        &meta_path,
        serde_json::to_vec_pretty(&metadata).expect("metadata should serialize"),
    )
    .expect("legacy metadata should be writable");

    let legacy = store
        .get_session(created.id)
        .await
        .expect("legacy metadata should still load");
    assert_eq!(legacy.compacted_up_to_message_id, None);

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
                compacted_up_to: Some(0),
                ..Default::default()
            },
        )
        .await
        .expect("zero boundary should clear the checkpoint");
    assert!(cleared.compaction_summary.is_none());
    assert!(cleared.compacted_up_to.is_none());
    assert!(cleared.compacted_up_to_message_id.is_none());
}

#[tokio::test]
async fn compaction_checkpoint_must_match_filesystem_transcript() {
    let (_tmp, store) = temp_store();
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

#[tokio::test]
async fn list_sessions_skips_unreadable_session_instead_of_failing() {
    let (_tmp, store) = temp_store();

    let good = store
        .create_session(new_session("still readable"))
        .await
        .expect("create should succeed");

    // Hand-craft a session directory with a `model` value that cannot
    // deserialize into `ModelId`, simulating a stale/removed model id.
    let sessions_dir = store.data_dir().join("sessions");
    let corrupt_id = uuid::Uuid::new_v4();
    let corrupt_dir = sessions_dir.join(corrupt_id.to_string());
    std::fs::create_dir_all(&corrupt_dir).unwrap();
    std::fs::write(
        corrupt_dir.join("meta.json"),
        format!(
            r#"{{
                "id": "{corrupt_id}",
                "title": "stale model",
                "model": "claude-3.7-sonnet-copilot",
                "mode": "BUILD",
                "created_at": "2024-01-01T00:00:00Z",
                "updated_at": "2024-01-01T00:00:00Z"
            }}"#
        ),
    )
    .unwrap();
    std::fs::write(corrupt_dir.join("messages.jsonl"), "").unwrap();

    let summaries = store
        .list_sessions()
        .await
        .expect("a corrupt session must not fail the whole list");

    let ids: Vec<uuid::Uuid> = summaries.iter().map(|s| s.id).collect();
    assert!(
        ids.contains(&good.id),
        "the valid session must still be listed: {ids:?}"
    );
    assert!(
        !ids.contains(&corrupt_id),
        "the corrupt session must be skipped, not listed: {ids:?}"
    );
}
