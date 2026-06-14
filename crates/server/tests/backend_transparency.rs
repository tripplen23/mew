//! Property 1: Backend transparency.
//!
//! Generates a random sequence of store operations and applies the SAME
//! sequence to a [`MemoryStore`] and to an [`FsStore`] over a `tempfile`
//! directory, asserting that the two backends produce observationally equal
//! response shapes and ordering.
//!
//! Ids and timestamps are generated independently by each backend, so they are
//! never compared across backends. Instead we compare observable shape: the
//! ok/err classification, titles, message text ordering, and list ordering.
//!
//! Both backends run unconditionally in CI (no `#[ignore]`, no env gate).

use chrono::{DateTime, Utc};
use mewcode_protocol::{Message, MessagePart, Mode, ModelId, Role};
use mewcode_server::store::{NewSession, SessionStore, StoreError};
use proptest::prelude::*;
use uuid::Uuid;

/// One generated store operation. `session_index` is an abstract handle that is
/// mapped (per backend) into that backend's own created-id list by position.
#[derive(Debug, Clone)]
enum Op {
    /// Create a session with the given title.
    Create { title: String },
    /// Append a text message to the session at `session_index`.
    Append { session_index: usize, text: String },
    /// Delete the session at `session_index`.
    Delete { session_index: usize },
    /// Fetch the session at `session_index`.
    Get { session_index: usize },
    /// List all session summaries.
    List,
}

/// Observable outcome of applying an [`Op`], stripped of ids/timestamps so the
/// two backends can be compared directly.
#[derive(Debug, PartialEq, Eq)]
enum Outcome {
    /// `create` succeeded: (title, message_count_is_zero).
    Created { title: String, empty: bool },
    /// `get` succeeded: (title, ordered message texts).
    Got { title: String, texts: Vec<String> },
    /// `list` succeeded: ordered titles (newest-first).
    Listed { titles: Vec<String> },
    /// A non-`get`/`list` op succeeded (append/delete).
    Ok,
    /// The op failed with `NotFound`.
    NotFound,
    /// The op failed with some other store error (carries the variant label).
    OtherErr(&'static str),
}

/// Classify a `StoreError` into a stable, backend-independent label.
fn err_label(e: &StoreError) -> &'static str {
    match e {
        StoreError::NotFound => "not_found",
        StoreError::Invalid(_) => "invalid",
        StoreError::Io(_) => "io",
        StoreError::Serde(_) => "serde",
    }
}

/// Build a user message carrying a single text part at the given instant.
fn text_message(text: &str, created_at: DateTime<Utc>) -> Message {
    Message {
        id: Uuid::new_v4(),
        role: Role::User,
        parts: vec![MessagePart::Text {
            text: text.to_string(),
        }],
        model: None,
        created_at,
    }
}

/// Extract the ordered `Text` payloads from a message history.
fn texts_of(messages: &[Message]) -> Vec<String> {
    messages
        .iter()
        .filter_map(|m| {
            m.parts.iter().find_map(|p| match p {
                MessagePart::Text { text } => Some(text.clone()),
                _ => None,
            })
        })
        .collect()
}

/// A uuid that no backend will ever generate for a real session. Used to map an
/// out-of-range `session_index` so BOTH backends consistently see `NotFound`.
const NONEXISTENT: Uuid = Uuid::nil();

/// Resolve a `session_index` into a concrete id for one backend. When the
/// backend has created sessions, the index wraps into its own id list; when it
/// has none, both backends fall back to the same nonexistent id.
fn resolve_id(ids: &[Uuid], session_index: usize) -> Uuid {
    if ids.is_empty() {
        NONEXISTENT
    } else {
        ids[session_index % ids.len()]
    }
}

/// Apply one op to a store, mutating that backend's per-backend id list, and
/// return the observable outcome. `clock` provides strictly increasing
/// timestamps so appended message ordering is deterministic across backends.
async fn apply(
    store: &dyn SessionStore,
    ids: &mut Vec<Uuid>,
    op: &Op,
    clock: DateTime<Utc>,
) -> Outcome {
    match op {
        Op::Create { title } => {
            let new = NewSession {
                title: title.clone(),
                model: ModelId::default(),
                mode: Mode::default(),
            };
            match store.create_session(new).await {
                Ok(s) => {
                    // Newest-first: front, mirroring list ordering.
                    ids.insert(0, s.id);
                    Outcome::Created {
                        title: s.title,
                        empty: s.messages.is_empty(),
                    }
                }
                Err(e) => Outcome::OtherErr(err_label(&e)),
            }
        }
        Op::Append {
            session_index,
            text,
        } => {
            let id = resolve_id(ids, *session_index);
            match store.append_message(id, text_message(text, clock)).await {
                Ok(()) => Outcome::Ok,
                Err(StoreError::NotFound) => Outcome::NotFound,
                Err(e) => Outcome::OtherErr(err_label(&e)),
            }
        }
        Op::Delete { session_index } => {
            let id = resolve_id(ids, *session_index);
            match store.delete_session(id).await {
                Ok(()) => {
                    ids.retain(|x| *x != id);
                    Outcome::Ok
                }
                Err(StoreError::NotFound) => Outcome::NotFound,
                Err(e) => Outcome::OtherErr(err_label(&e)),
            }
        }
        Op::Get { session_index } => {
            let id = resolve_id(ids, *session_index);
            match store.get_session(id).await {
                Ok(s) => Outcome::Got {
                    title: s.title,
                    texts: texts_of(&s.messages),
                },
                Err(StoreError::NotFound) => Outcome::NotFound,
                Err(e) => Outcome::OtherErr(err_label(&e)),
            }
        }
        Op::List => match store.list_sessions().await {
            Ok(summaries) => Outcome::Listed {
                titles: summaries.into_iter().map(|s| s.title).collect(),
            },
            Err(e) => Outcome::OtherErr(err_label(&e)),
        },
    }
}

/// Strategy for a single op. Titles/texts are short ascii-ish strings; the
/// `session_index` is a modest usize that `resolve_id` wraps into range.
fn op_strategy() -> impl Strategy<Value = Op> {
    prop_oneof![
        "[a-z ]{0,8}".prop_map(|title| Op::Create { title }),
        (0usize..8, "[a-z ]{0,8}")
            .prop_map(|(session_index, text)| Op::Append { session_index, text }),
        (0usize..8).prop_map(|session_index| Op::Delete { session_index }),
        (0usize..8).prop_map(|session_index| Op::Get { session_index }),
        Just(Op::List),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(32))]

    /// Both backends must produce identical observable outcomes for the same
    /// op sequence.
    #[test]
    fn backends_are_observationally_equal(ops in prop::collection::vec(op_strategy(), 1..20)) {
        // One runtime per case; the store ops are async but proptest is sync.
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let mem = mewcode_server::store::memory::MemoryStore::new();
            let dir = tempfile::tempdir().unwrap();
            let fs = mewcode_server::store::fs::FsStore::new(dir.path().to_path_buf()).unwrap();

            let mut mem_ids: Vec<Uuid> = Vec::new();
            let mut fs_ids: Vec<Uuid> = Vec::new();

            // A monotonically increasing clock so appended message ordering is
            // deterministic and identical across both backends.
            let base = Utc::now();
            for (i, op) in ops.iter().enumerate() {
                let clock = base + chrono::Duration::milliseconds(i as i64);
                let mem_out = apply(&mem, &mut mem_ids, op, clock).await;
                let fs_out = apply(&fs, &mut fs_ids, op, clock).await;
                prop_assert_eq!(mem_out, fs_out, "divergence at op {}: {:?}", i, op);
            }
            Ok(())
        })?;
    }
}
