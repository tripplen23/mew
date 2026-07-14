//! Property 8: Atomic `meta.json` write.
//!
//! An interrupted or concurrent write never leaves a partially-written
//! `meta.json`: writes go to a temp file and are then `rename`d over the
//! target, so a reader always observes either the previous complete file or
//! the new complete file — never a torn write. After a successful append, no
//! `*.tmp` file remains in the session directory.
//!
//! **Validates: Requirements 2.6**

use std::path::PathBuf;
use std::sync::Arc;

use mewcode_protocol::{Message, MessagePart, Mode, ModelId};
use mewcode_server::store::fs::FsStore;
use mewcode_server::store::{NewSession, SessionStore};
use proptest::prelude::*;
use serde_json::Value;
use tempfile::TempDir;
use uuid::Uuid;

/// Build a throwaway `FsStore` rooted at a fresh temp dir.
fn temp_store() -> (TempDir, FsStore) {
    let tmp = TempDir::new().expect("create temp data dir");
    let store = FsStore::new(tmp.path().to_path_buf()).expect("init FsStore");
    (tmp, store)
}

/// Build a simple text user message.
fn text_message(body: &str) -> Message {
    Message::user(vec![MessagePart::Text {
        text: body.to_string(),
    }])
}

/// Path to a session's `meta.json` on disk.
fn meta_path(store: &FsStore, id: Uuid) -> PathBuf {
    store
        .data_dir()
        .join("sessions")
        .join(id.to_string())
        .join("meta.json")
}

/// Path to a session's directory on disk.
fn session_dir(store: &FsStore, id: Uuid) -> PathBuf {
    store.data_dir().join("sessions").join(id.to_string())
}

/// Assert no leftover `*.tmp` files remain in the session directory.
fn assert_no_tmp_files(store: &FsStore, id: Uuid) {
    let dir = session_dir(store, id);
    for entry in std::fs::read_dir(&dir).expect("read session dir") {
        let entry = entry.expect("dir entry");
        let name = entry.file_name();
        let name = name.to_string_lossy();
        assert!(
            !name.ends_with(".tmp"),
            "leftover temp file after write: {name}"
        );
    }
}

#[tokio::test]
async fn no_tmp_file_remains_after_append() {
    let (_tmp, store) = temp_store();
    let session = store
        .create_session(NewSession {
            title: "atomic".to_string(),
            model: ModelId::DEFAULT,
            mode: Mode::Build,
        })
        .await
        .expect("create session");

    // create_session itself writes meta.json atomically.
    assert_no_tmp_files(&store, session.id);

    for i in 0..16 {
        store
            .append_message(session.id, text_message(&format!("msg {i}")))
            .await
            .expect("append message");
        // Each append rewrites meta.json via temp-then-rename.
        assert_no_tmp_files(&store, session.id);
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn meta_read_during_writes_is_never_torn() {
    let (_tmp, store) = temp_store();
    let store = Arc::new(store);
    let session = store
        .create_session(NewSession {
            title: "concurrent".to_string(),
            model: ModelId::DEFAULT,
            mode: Mode::Build,
        })
        .await
        .expect("create session");
    let id = session.id;
    let path = meta_path(&store, id);

    // Writer: continuously rewrites meta.json (each append bumps updated_at and
    // rewrites meta via temp-then-rename).
    let writer = {
        let store = Arc::clone(&store);
        tokio::spawn(async move {
            for i in 0..400 {
                store
                    .append_message(id, text_message(&format!("m{i}")))
                    .await
                    .expect("append message");
            }
        })
    };

    // Readers: repeatedly read the raw bytes of meta.json while writes are in
    // flight. Every observed file must parse as a complete JSON object whose
    // `id` matches — a torn/half write would fail to parse.
    let mut readers = Vec::new();
    for _ in 0..3 {
        let path = path.clone();
        readers.push(tokio::spawn(async move {
            for _ in 0..2_000 {
                match std::fs::read(&path) {
                    Ok(bytes) => {
                        let parsed: Value = serde_json::from_slice(&bytes).unwrap_or_else(|e| {
                            panic!("torn meta.json observed (failed to parse): {e}")
                        });
                        assert_eq!(
                            parsed.get("id").and_then(Value::as_str),
                            Some(id.to_string().as_str()),
                            "meta.json id field missing or mismatched"
                        );
                        tokio::task::yield_now().await;
                    }
                    // A transient NotFound is impossible here (meta.json always
                    // exists once created), but never observe a torn file.
                    Err(e) => panic!("failed to read meta.json: {e}"),
                }
            }
        }));
    }

    writer.await.expect("writer task");
    for reader in readers {
        reader.await.expect("reader task");
    }

    // Final state is a complete, parseable file with the full history.
    assert_no_tmp_files(&store, id);
    let reloaded = store.get_session(id).await.expect("get session");
    assert_eq!(reloaded.messages.len(), 400);
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(32))]

    /// For any sequence of appended messages, the on-disk `meta.json` always
    /// parses cleanly into a complete object and no `*.tmp` file is left behind.
    #[test]
    fn meta_json_stays_complete_across_appends(
        title in "[ -~]{1,40}",
        bodies in proptest::collection::vec("[ -~]{0,50}", 0..12),
    ) {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("build runtime");

        rt.block_on(async move {
            let (_tmp, store) = temp_store();
            let session = store
                .create_session(NewSession {
                    title: title.clone(),
                    model: ModelId::DEFAULT,
                    mode: Mode::Plan,
                })
                .await
                .expect("create session");

            for body in &bodies {
                store
                    .append_message(session.id, text_message(body))
                    .await
                    .expect("append message");

                // The on-disk meta.json must always be a complete object.
                let bytes = std::fs::read(meta_path(&store, session.id))
                    .expect("read meta.json");
                let parsed: Value =
                    serde_json::from_slice(&bytes).expect("meta.json parses");
                prop_assert!(parsed.is_object());
                assert_no_tmp_files(&store, session.id);
            }

            // Full reload reflects every appended message in order.
            let reloaded = store.get_session(session.id).await.expect("get session");
            prop_assert_eq!(reloaded.messages.len(), bodies.len());
            prop_assert_eq!(reloaded.title, title);
            Ok(())
        })?;
    }
}
