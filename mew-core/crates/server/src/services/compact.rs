//! Manual compaction workflow: load session, compact history, persist summary.

use std::collections::HashMap;

use mewcode_engine::history::{CHARS_PER_TOKEN, prune_messages, split_for_compaction};
use mewcode_engine::{EngineConfig, compact_history};
use mewcode_protocol::event::CompactionPhase;
use mewcode_protocol::{Message, ModelId, StreamEvent};
use tokio::sync::{RwLock, mpsc};

use crate::AppState;
use crate::store::{SessionPatch, SessionStore, StoreError};

use super::runtime::{project_memory, project_root};

#[doc(hidden)]
pub const GENERIC_COMPACTION_ERROR: &str = "compaction failed";

#[doc(hidden)]
pub fn client_store_error_message(error: &StoreError) -> &'static str {
    match error {
        StoreError::NotFound => "session not found",
        StoreError::Invalid(_) | StoreError::Io(_) | StoreError::Serde(_) => {
            GENERIC_COMPACTION_ERROR
        }
    }
}

#[doc(hidden)]
pub fn forward_compaction_event(
    client: &mut Option<mpsc::Sender<StreamEvent>>,
    event: StreamEvent,
) {
    let event = match event {
        StreamEvent::Error { message } => {
            tracing::error!(%message, "compaction stream error");
            StreamEvent::Error {
                message: GENERIC_COMPACTION_ERROR.into(),
            }
        }
        event => event,
    };
    let Some(sender) = client else {
        return;
    };
    if sender.try_send(event).is_err() {
        *client = None;
    }
}

#[doc(hidden)]
pub fn prepare_compaction(messages: &[Message]) -> (Vec<Message>, Vec<Message>) {
    let mut pruned = prune_messages(messages);
    let head_len = split_for_compaction(&pruned).0.len();
    let tail = pruned.split_off(head_len);
    (pruned, tail)
}

#[doc(hidden)]
pub fn validated_summary(summary: String) -> Result<String, &'static str> {
    let summary = summary.trim();
    if summary.is_empty() {
        return Err("compaction returned an empty summary");
    }
    Ok(summary.to_owned())
}

#[doc(hidden)]
pub async fn persist_compaction(
    store: &dyn SessionStore,
    session_tokens: &RwLock<HashMap<uuid::Uuid, u64>>,
    id: uuid::Uuid,
    patch: SessionPatch,
    estimated_tokens: u64,
) -> Result<(), StoreError> {
    store.patch_session(id, patch).await?;
    session_tokens.write().await.insert(id, estimated_tokens);
    Ok(())
}

pub(crate) async fn start_compaction(
    state: AppState,
    id: uuid::Uuid,
) -> mpsc::Receiver<StreamEvent> {
    let (client_tx, rx) = mpsc::channel::<StreamEvent>(64);

    let store = state.store.clone();
    let session_tokens = state.session_tokens.clone();
    let root = project_root();
    let memory = project_memory(&state.memory, &root);

    tokio::spawn(async move {
        let (tx, mut events) = mpsc::channel::<StreamEvent>(64);
        let worker = tokio::spawn(async move {
            let operation_lock = match state.existing_session_operation_lock(id).await {
                Ok(lock) => lock,
                Err(error) => {
                    if !matches!(error, StoreError::NotFound) {
                        tracing::error!(%error, session_id = %id, "failed to load compaction session lock");
                    }
                    let _ = tx
                        .send(StreamEvent::Error {
                            message: client_store_error_message(&error).into(),
                        })
                        .await;
                    return;
                }
            };
            let _operation_guard = operation_lock.lock().await;

            let session = match store.get_session(id).await {
                Ok(session) => session,
                Err(error) => {
                    if !matches!(error, StoreError::NotFound) {
                        tracing::error!(%error, session_id = %id, "failed to reload compaction session");
                    }
                    let _ = tx
                        .send(StreamEvent::Error {
                            message: client_store_error_message(&error).into(),
                        })
                        .await;
                    return;
                }
            };

            let model: ModelId = session.model;

            let tokens_before = {
                let map = session_tokens.read().await;
                map.get(&id).copied().unwrap_or(0)
            };

            let checkpoint = match (
                session.compaction_summary.as_deref(),
                session.compacted_up_to,
                session.compacted_up_to_message_id,
            ) {
                (Some(summary), Some(up_to), Some(message_id))
                    if !summary.trim().is_empty()
                        && up_to > 0
                        && session
                            .messages
                            .get(up_to - 1)
                            .is_some_and(|message| message.id == message_id) =>
                {
                    Some((up_to, summary))
                }
                _ => None,
            };
            let already_covered = checkpoint.map_or(0, |(up_to, _)| up_to);
            let previous_summary = checkpoint.map(|(_, summary)| summary);
            let uncovered = &session.messages[already_covered..];
            let (head, tail) = prepare_compaction(uncovered);

            if head.is_empty() {
                let _ = tx
                    .send(StreamEvent::Error {
                        message: "not enough history to compact (need at least 2 turns)".into(),
                    })
                    .await;
                return;
            }

            let _ = tx
                .send(StreamEvent::CompactionStarted { session_id: id })
                .await;

            let _ = tx
                .send(StreamEvent::CompactionProgress {
                    phase: CompactionPhase::Pruning,
                    message: "Pruning tool results and low-value content...".into(),
                })
                .await;

            let cfg = match EngineConfig::from_env() {
                Ok(config) => config,
                Err(error) => {
                    tracing::error!(%error, session_id = %id, "failed to load compaction configuration");
                    let _ = tx
                        .send(StreamEvent::Error {
                            message: GENERIC_COMPACTION_ERROR.into(),
                        })
                        .await;
                    return;
                }
            };

            let _ = tx
                .send(StreamEvent::CompactionProgress {
                    phase: CompactionPhase::Summarizing,
                    message: "Running LLM to summarize history...".into(),
                })
                .await;

            let result = match compact_history(
                &head,
                model,
                &cfg,
                Some(memory),
                tokens_before,
                previous_summary,
                &tx,
            )
            .await
            {
                Ok(result) => result,
                Err(error) => {
                    tracing::error!(%error, session_id = %id, "compaction worker failed");
                    let _ = tx
                        .send(StreamEvent::Error {
                            message: GENERIC_COMPACTION_ERROR.into(),
                        })
                        .await;
                    return;
                }
            };
            let summary = match validated_summary(result.summary) {
                Ok(summary) => summary,
                Err(message) => {
                    let _ = tx
                        .send(StreamEvent::Error {
                            message: message.into(),
                        })
                        .await;
                    return;
                }
            };

            let new_boundary = already_covered + head.len();
            let Some(compacted_up_to_message_id) = head.last().map(|message| message.id) else {
                let _ = tx
                    .send(StreamEvent::Error {
                        message: "compaction produced an empty history prefix".into(),
                    })
                    .await;
                return;
            };
            let patch = SessionPatch {
                compaction_summary: Some(summary.clone()),
                compacted_up_to: Some(new_boundary),
                compacted_up_to_message_id: Some(compacted_up_to_message_id),
                ..Default::default()
            };

            let tail_chars: usize = tail
                .iter()
                .flat_map(|message| &message.parts)
                .filter_map(|part| match part {
                    mewcode_protocol::MessagePart::Text { text } => Some(text.len()),
                    mewcode_protocol::MessagePart::ToolResult(_) => None,
                    _ => Some(0),
                })
                .sum();
            let estimated_tokens = ((summary.len() + tail_chars) / CHARS_PER_TOKEN) as u64;
            if let Err(error) = persist_compaction(
                store.as_ref(),
                session_tokens.as_ref(),
                id,
                patch,
                estimated_tokens,
            )
            .await
            {
                tracing::error!(%error, "failed to persist compaction summary");
                let _ = tx
                    .send(StreamEvent::Error {
                        message: "failed to persist compaction state".into(),
                    })
                    .await;
                return;
            }

            let _ = tx
                .send(StreamEvent::Compacted {
                    tokens_before: result.tokens_before,
                    context_limit: result.context_limit,
                    summary,
                    thought_duration_ms: result.thought_duration_ms,
                })
                .await;

            let _ = tx
                .send(StreamEvent::CompactionProgress {
                    phase: CompactionPhase::Done,
                    message: "Compaction complete.".into(),
                })
                .await;

            let _ = tx
                .send(StreamEvent::Finish {
                    duration_ms: result.thought_duration_ms,
                    input_tokens: None,
                    output_tokens: None,
                    session_tokens: Some(estimated_tokens),
                    context_limit: Some(result.context_limit),
                })
                .await;
        });

        let mut client = Some(client_tx);
        // ponytail: The public channel has a 64-event burst ceiling; a
        // stalled client loses the rest. Upgrade to a detached bounded spool
        // with cancellation if lossless slow-client delivery matters.
        while let Some(event) = events.recv().await {
            forward_compaction_event(&mut client, event);
        }
        if let Err(error) = worker.await {
            tracing::error!(%error, session_id = %id, "compaction worker panicked");
            forward_compaction_event(
                &mut client,
                StreamEvent::Error {
                    message: GENERIC_COMPACTION_ERROR.into(),
                },
            );
        }
    });

    rx
}
