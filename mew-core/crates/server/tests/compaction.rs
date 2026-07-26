use std::collections::HashMap;

use mewcode_protocol::{Message, MessagePart, StreamEvent, ToolResult};
use mewcode_server::services::compact::{
    GENERIC_COMPACTION_ERROR, client_store_error_message, forward_compaction_event,
    persist_compaction, prepare_compaction, validated_summary,
};
use mewcode_server::store::memory::MemoryStore;
use mewcode_server::store::{SessionPatch, StoreError};
use serde_json::json;
use tokio::sync::{RwLock, mpsc};

#[test]
fn manual_compaction_prunes_before_splitting() {
    let mut messages = vec![Message::assistant(
        vec![MessagePart::ToolResult(ToolResult {
            call_id: "call-1".into(),
            name: "bash".into(),
            output: json!("large tool output"),
            is_error: false,
            display: None,
        })],
        "test-model",
    )];
    messages.extend((0..5).map(|index| {
        Message::user(vec![MessagePart::Text {
            text: format!("turn {index}"),
        }])
    }));

    let (head, _) = prepare_compaction(&messages);

    assert!(
        head.iter()
            .flat_map(|message| &message.parts)
            .all(|part| { !matches!(part, MessagePart::ToolResult(_)) })
    );
}

#[test]
fn manual_compaction_rejects_blank_summary() {
    assert!(validated_summary(" \n ".into()).is_err());
    assert_eq!(validated_summary(" summary \n".into()).unwrap(), "summary");
}

#[tokio::test]
async fn failed_checkpoint_persistence_does_not_update_tokens() {
    let store = MemoryStore::new();
    let session_id = uuid::Uuid::new_v4();
    let tokens = RwLock::new(HashMap::from([(session_id, 99)]));
    let patch = SessionPatch {
        compaction_summary: Some("summary".into()),
        compacted_up_to: Some(1),
        compacted_up_to_message_id: Some(uuid::Uuid::new_v4()),
        ..Default::default()
    };

    let result = persist_compaction(&store, &tokens, session_id, patch, 10).await;

    assert!(result.is_err());
    assert_eq!(tokens.read().await.get(&session_id), Some(&99));
}

#[test]
fn compaction_store_errors_expose_only_stable_messages() {
    assert_eq!(
        client_store_error_message(&StoreError::NotFound),
        "session not found"
    );
    assert_eq!(
        client_store_error_message(&StoreError::Io(std::io::Error::other("/secret/path"))),
        GENERIC_COMPACTION_ERROR
    );
}

#[test]
fn compaction_forwarder_sanitizes_errors_and_drops_full_clients() {
    let (tx, mut rx) = mpsc::channel(1);
    let mut client = Some(tx);
    forward_compaction_event(
        &mut client,
        StreamEvent::Error {
            message: "/secret/provider/error".into(),
        },
    );
    assert_eq!(
        rx.try_recv().expect("generic error should be forwarded"),
        StreamEvent::Error {
            message: GENERIC_COMPACTION_ERROR.into()
        }
    );

    let sender = client.as_ref().expect("client should still be attached");
    sender
        .try_send(StreamEvent::CompactionStarted {
            session_id: uuid::Uuid::new_v4(),
        })
        .expect("prefill should succeed");
    forward_compaction_event(
        &mut client,
        StreamEvent::CompactionStarted {
            session_id: uuid::Uuid::new_v4(),
        },
    );
    assert!(client.is_none());
}
