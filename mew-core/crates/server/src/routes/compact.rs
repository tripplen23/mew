//! `POST /sessions/:id/compact` — manually trigger context compaction.
//!
//! Loads the session's message history, runs the LLM compaction agent,
//! persists the summary to the session, and streams progress as SSE.

use std::convert::Infallible;

use axum::extract::{Path, State};
use axum::response::sse::Sse;
use futures::stream::Stream;
use mewcode_engine::history::{CHARS_PER_TOKEN, prune_messages, split_for_compaction};
use mewcode_engine::{EngineConfig, compact_history};
use mewcode_protocol::event::CompactionPhase;
use mewcode_protocol::{ModelId, StreamEvent};

use crate::AppState;
use crate::sse::from_channel;
use crate::store::SessionPatch;

/// `POST /sessions/:id/compact` — manually trigger context compaction.
///
/// Loads the session's message history, runs the LLM compaction agent,
/// persists the summary to the session, and streams progress as SSE.
#[utoipa::path(
    post,
    path = "/sessions/{id}/compact",
    tag = "sessions",
    params(
        ("id" = Uuid, Path, description = "Session identifier"),
    ),
    responses(
        (status = 200, description = "SSE stream of compaction events", body = StreamEvent, content_type = "text/event-stream"),
        (status = 404, description = "Session not found"),
        (status = 500, description = "Compaction failed"),
    ),
)]
pub async fn compact_session(
    State(state): State<AppState>,
    Path(id): Path<uuid::Uuid>,
) -> Sse<impl Stream<Item = Result<axum::response::sse::Event, Infallible>>> {
    let (tx, rx) = tokio::sync::mpsc::channel::<StreamEvent>(64);

    // Spawn the compaction task.
    let store = state.store.clone();
    let session_tokens = state.session_tokens.clone();
    // Scope memory to this server's project (its CWD), same as the chat
    // route, so a durable fact learned in one project's sessions never
    // leaks into another project's compaction summaries.
    let root = std::env::current_dir()
        .or_else(|_| std::fs::canonicalize("."))
        .unwrap_or_else(|_| ".".into());
    let memory = match state.memory.data_dir() {
        Some(data_dir) => {
            mewcode_engine::memory::MemoryStore::for_project(data_dir.to_path_buf(), &root)
        }
        None => state.memory.clone(),
    };

    tokio::spawn(async move {
        // Load the session to get the model and message history.
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

        // Get the accumulated token count for this session.
        let tokens_before = {
            let map = session_tokens.read().await;
            map.get(&id).copied().unwrap_or(0)
        };

        // Only the portion not already covered by a prior compaction is
        // eligible to be folded into the new summary — messages before that
        // boundary are represented by `session.compaction_summary` already.
        let already_covered = session
            .compacted_up_to
            .unwrap_or(0)
            .min(session.messages.len());
        let uncovered = &session.messages[already_covered..];

        // Split the uncovered tail: preserve last 2 turns, compact the rest.
        // Reuses the engine's boundary-finding logic so this never drifts
        // from the automatic-compaction path's definition of "a turn".
        let (head, tail) = split_for_compaction(uncovered);

        if head.is_empty() {
            let _ = tx
                .send(StreamEvent::Error {
                    message: "not enough history to compact (need at least 2 turns)".into(),
                })
                .await;
            return;
        }

        // Emit started event.
        let _ = tx
            .send(StreamEvent::CompactionStarted { session_id: id })
            .await;

        // Phase 1: Pruning.
        let _ = tx
            .send(StreamEvent::CompactionProgress {
                phase: CompactionPhase::Pruning,
                message: "Pruning tool results and low-value content...".into(),
            })
            .await;

        // Build engine config from environment.
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

        // Phase 2: Summarizing.
        let _ = tx
            .send(StreamEvent::CompactionProgress {
                phase: CompactionPhase::Summarizing,
                message: "Running LLM to summarize history...".into(),
            })
            .await;

        // Run compaction. Fold any existing summary in as prior context so
        // repeated manual compactions don't lose everything before the last one.
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

        // Persist the summary AND the new boundary: everything up to
        // `already_covered + head.len()` is now represented by the summary
        // alone, so the next turn (and the next manual compaction) only
        // re-sends/re-examines the tail. This is what actually shrinks what
        // gets sent to the model, rather than the summary being advisory-only.
        let new_boundary = already_covered + head.len();
        let patch = SessionPatch {
            compaction_summary: Some(result.summary.clone()),
            compacted_up_to: Some(new_boundary),
            ..Default::default()
        };
        if let Err(e) = store.patch_session(id, patch).await {
            tracing::warn!(error = %e, "failed to persist compaction summary");
        }

        // Reset token counter after compaction.
        // Estimate tokens based on remaining content (summary + preserved
        // turns). Must mirror the harness's heuristic so the post-compaction
        // counter is aligned with what the next turn will actually send —
        // otherwise automatic compaction either fires too late (counter too
        // high) or thrashes (counter too low). Specifically: count the
        // summary as full text, then for the preserved `tail` count only
        // `prune_messages(tail)` so large tool-result payloads are
        // discounted the same way the harness discounts them.
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
                            // Tool results are pruned away before the model
                            // sees them on the next turn, so they don't count
                            // toward the context budget any more. Counting
                            // them would make automatic compaction fire late.
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

        // Emit final compacted event.
        let _ = tx
            .send(StreamEvent::Compacted {
                tokens_before: result.tokens_before,
                context_limit: result.context_limit,
                summary: result.summary,
                thought_duration_ms: result.thought_duration_ms,
            })
            .await;

        // Emit done progress.
        let _ = tx
            .send(StreamEvent::CompactionProgress {
                phase: CompactionPhase::Done,
                message: "Compaction complete.".into(),
            })
            .await;

        // Emit finish.
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

    from_channel(rx)
}
