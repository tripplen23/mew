//! Manual compaction workflow: load session, compact history, persist summary.

use mewcode_engine::history::{CHARS_PER_TOKEN, prune_messages, split_for_compaction};
use mewcode_engine::{EngineConfig, compact_history};
use mewcode_protocol::event::CompactionPhase;
use mewcode_protocol::{ModelId, StreamEvent};
use tokio::sync::mpsc;

use crate::AppState;
use crate::store::SessionPatch;

use super::runtime::{project_memory, project_root};

pub(crate) async fn start_compaction(
    state: AppState,
    id: uuid::Uuid,
) -> mpsc::Receiver<StreamEvent> {
    let (tx, rx) = mpsc::channel::<StreamEvent>(64);

    let store = state.store.clone();
    let session_tokens = state.session_tokens.clone();
    let root = project_root();
    let memory = project_memory(&state.memory, &root);

    tokio::spawn(async move {
        let session = match store.get_session(id).await {
            Ok(s) => s,
            Err(e) => {
                let _ = tx
                    .send(StreamEvent::Error {
                        message: format!("session not found: {e}"),
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

        let already_covered = session
            .compacted_up_to
            .unwrap_or(0)
            .min(session.messages.len());
        let uncovered = &session.messages[already_covered..];

        let (head, tail) = split_for_compaction(uncovered);

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
            Ok(c) => c,
            Err(e) => {
                let _ = tx
                    .send(StreamEvent::Error {
                        message: format!("config error: {e}"),
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
            head,
            model,
            &cfg,
            Some(memory),
            tokens_before,
            session.compaction_summary.as_deref(),
            &tx,
        )
        .await
        {
            Ok(r) => r,
            Err(e) => {
                let _ = tx
                    .send(StreamEvent::Error {
                        message: format!("compaction failed: {e}"),
                    })
                    .await;
                return;
            }
        };

        let new_boundary = already_covered + head.len();
        let patch = SessionPatch {
            compaction_summary: Some(result.summary.clone()),
            compacted_up_to: Some(new_boundary),
            ..Default::default()
        };
        if let Err(e) = store.patch_session(id, patch).await {
            tracing::warn!(error = %e, "failed to persist compaction summary");
        }

        let estimated_tokens = {
            let summary_chars = result.summary.len();
            let pruned_tail = prune_messages(tail);
            let tail_chars: usize = pruned_tail
                .iter()
                .map(|m| {
                    m.parts
                        .iter()
                        .filter_map(|p| match p {
                            mewcode_protocol::MessagePart::Text { text } => Some(text.len()),
                            mewcode_protocol::MessagePart::ToolResult(_) => None,
                            _ => Some(0),
                        })
                        .sum::<usize>()
                })
                .sum::<usize>();
            ((summary_chars + tail_chars) / CHARS_PER_TOKEN) as u64
        };
        {
            let mut map = session_tokens.write().await;
            map.insert(id, estimated_tokens);
        }

        let _ = tx
            .send(StreamEvent::Compacted {
                tokens_before: result.tokens_before,
                context_limit: result.context_limit,
                summary: result.summary,
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

    rx
}
