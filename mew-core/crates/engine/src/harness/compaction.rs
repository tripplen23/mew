use mewcode_protocol::{Message, Role, StreamEvent};
use tokio::sync::mpsc;

use super::Harness;
use crate::config::EngineConfig;
use crate::history;

/// A summary paired with the message-prefix boundary it is intended to replace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompactionCheckpoint {
    pub summary: String,
    pub up_to: usize,
}

impl CompactionCheckpoint {
    /// Returns `None` when `summary` is blank, empty, or `up_to` is zero.
    pub fn new(summary: Option<String>, up_to: usize) -> Option<Self> {
        let summary = summary?.trim().to_string();
        if summary.is_empty() || up_to == 0 {
            return None;
        }
        Some(Self { summary, up_to })
    }
}

/// Mutable context-compaction state for one harness.
#[derive(Debug, Clone, Default)]
pub struct CompactionState {
    pub(super) context_tokens: u64,
    pub checkpoint: Option<CompactionCheckpoint>,
    pub pending_update: Option<CompactionCheckpoint>,
}

impl CompactionState {
    fn checkpoint_for_history(&mut self, history_len: usize) -> Option<CompactionCheckpoint> {
        match self.checkpoint.clone() {
            Some(checkpoint) if checkpoint.up_to <= history_len => Some(checkpoint),
            Some(_) => {
                self.checkpoint = None;
                self.pending_update = None;
                None
            }
            None => None,
        }
    }

    /// Install a new checkpoint. Returns `false` when the summary is blank or empty.
    pub fn install_checkpoint(&mut self, summary: String, up_to: usize) -> bool {
        let Some(checkpoint) = CompactionCheckpoint::new(Some(summary), up_to) else {
            return false;
        };
        self.checkpoint = Some(checkpoint.clone());
        self.pending_update = Some(checkpoint);
        true
    }
}

pub fn truncate_fallback(mut text: String, max_bytes: usize) -> String {
    if text.len() <= max_bytes {
        return text;
    }

    const MARKER: &str = "\n[...truncated]";
    let marker = if max_bytes >= MARKER.len() {
        MARKER
    } else {
        ""
    };
    let mut end = max_bytes.saturating_sub(marker.len());
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text.truncate(end);
    text.push_str(marker);
    text
}

fn fallback_summary(messages: &[Message]) -> String {
    const FALLBACK_BYTE_CAP: usize = 8_000;
    let text = messages
        .iter()
        .map(|message| {
            let role = match message.role {
                Role::User => "User",
                Role::Assistant => "Assistant",
                Role::Tool => "Tool",
            };
            format!("{}: {}", role, history::text_of(message))
        })
        .collect::<Vec<_>>()
        .join("\n");
    truncate_fallback(text, FALLBACK_BYTE_CAP)
}

pub fn estimate_compacted_context(summary: &str, tail: &[Message]) -> u64 {
    // ponytail: Four chars per token is intentionally coarse; replace this
    // with the provider tokenizer when one is available without an API call.
    let chars = tail.iter().fold(summary.len(), |total, message| {
        total.saturating_add(history::text_of(message).len())
    });
    (chars / history::CHARS_PER_TOKEN) as u64
}

impl Harness {
    /// Check if compaction should trigger based on estimated context usage.
    fn should_compact(&self) -> bool {
        let limit = self.model.context_limit();
        if limit == 0 {
            return false;
        }
        let threshold = (limit as f64 * history::COMPACTION_THRESHOLD) as u64;
        self.compaction.context_tokens >= threshold
    }

    /// Build the history to send this turn, running automatic compaction
    /// first if estimated context usage is near the model's limit.
    pub async fn build_turn_history(
        &mut self,
        prior_messages: &[Message],
        cfg: &EngineConfig,
        tx: &mpsc::Sender<StreamEvent>,
    ) -> Vec<rig_core::completion::Message> {
        // A checkpoint is valid only for the transcript prefix it references.
        // Fail closed to full history when persisted metadata is stale.
        let checkpoint = self.compaction.checkpoint_for_history(prior_messages.len());
        let compacted_up_to = checkpoint.as_ref().map_or(0, |value| value.up_to);
        let uncovered = &prior_messages[compacted_up_to..];
        let needs_compaction =
            self.should_compact() && uncovered.len() > history::COMPACTION_PRESERVE_TURNS * 2;
        if !needs_compaction {
            return match checkpoint.as_ref() {
                Some(checkpoint) => history::build_history_with_summary_tail(
                    &checkpoint.summary,
                    uncovered,
                    &self.history_strategy,
                ),
                None => self.history_strategy.build(prior_messages),
            };
        }

        // Step 1: Prune (free, no LLM cost) — remove tool results, truncate file contents.
        let pruned = history::prune_messages(uncovered);

        // Heuristic: tool results are typically 50-70% of context tokens.
        // If we have many tool results, pruning alone might be enough.
        let has_tool_results = uncovered.iter().any(|m| {
            m.parts
                .iter()
                .any(|p| matches!(p, mewcode_protocol::MessagePart::ToolResult(_)))
        });

        // Estimate token savings from pruning (rough: 60% of tool result content).
        let tool_result_chars: usize = uncovered
            .iter()
            .flat_map(|m| &m.parts)
            .filter_map(|p| match p {
                mewcode_protocol::MessagePart::ToolResult(r) => {
                    serde_json::to_string(&r.output).ok().map(|s| s.len())
                }
                _ => None,
            })
            .sum();
        let estimated_token_savings = (tool_result_chars / history::CHARS_PER_TOKEN) as u64;
        let estimated_tokens_after_prune = self
            .compaction
            .context_tokens
            .saturating_sub(estimated_token_savings);

        let limit = self.model.context_limit();
        let threshold = (limit as f64 * history::COMPACTION_THRESHOLD) as u64;

        if has_tool_results && estimated_tokens_after_prune < threshold {
            // Pruning alone brings us back under threshold. Skip the LLM
            // call, but the pruned tail is still what's actually sent — no
            // summary is stored, so the compaction boundary does not move;
            // this is a per-turn optimization only.
            tracing::info!(
                estimated_tokens_after_prune,
                threshold,
                "pruned history, skipping LLM compaction"
            );
            let tokens_before = self.compaction.context_tokens;
            self.compaction.context_tokens = estimated_tokens_after_prune;
            let _ = tx
                .send(StreamEvent::Compacted {
                    tokens_before,
                    context_limit: limit,
                    summary: "[Pruned tool results — no LLM summary needed]".to_string(),
                    thought_duration_ms: 0,
                })
                .await;
            return match checkpoint.as_ref() {
                Some(checkpoint) => history::build_history_with_summary_tail(
                    &checkpoint.summary,
                    &pruned,
                    &self.history_strategy,
                ),
                None => self.history_strategy.build(&pruned),
            };
        }

        // Still over threshold after pruning (or nothing to prune). Split
        // the *uncovered, pruned* tail: fold the head into a new summary,
        // keep the recent turns verbatim. This actually shrinks what's sent
        // going forward, because the resulting boundary is persisted by the
        // caller via `updated_compaction()`.
        let (compact_head, tail) = history::split_for_compaction(&pruned);
        let tokens_before = self.compaction.context_tokens;
        let context_limit = limit;
        tracing::info!(
            head_count = compact_head.len(),
            tail_count = tail.len(),
            tokens_before,
            context_limit,
            "compacting history with LLM after prune"
        );

        let result = crate::compact::compact_history(
            compact_head,
            self.model,
            cfg,
            self.memory.clone(),
            tokens_before,
            checkpoint
                .as_ref()
                .map(|checkpoint| checkpoint.summary.as_str()),
            tx,
        )
        .await;
        let (mut summary, mut thought_duration_ms) = match result {
            Ok(r) => (r.summary, r.thought_duration_ms),
            Err(e) => {
                tracing::warn!(error = %e, "LLM compaction failed, using concatenation");
                (fallback_summary(compact_head), 0)
            }
        };

        // The new boundary: everything already covered, plus the messages
        // just folded into this summary. `pruned` mirrors `uncovered`
        // message-for-message (pruning only strips parts, never whole
        // messages), so `compact_head.len()` maps directly onto
        // `uncovered`/`prior_messages` indices.
        let new_up_to = compacted_up_to + compact_head.len();
        if !self
            .compaction
            .install_checkpoint(summary.clone(), new_up_to)
        {
            tracing::warn!("LLM compaction returned an empty summary, using concatenation");
            summary = fallback_summary(compact_head);
            thought_duration_ms = 0;
            let installed = self
                .compaction
                .install_checkpoint(summary.clone(), new_up_to);
            debug_assert!(installed, "fallback summary must create a checkpoint");
        }
        self.compaction.context_tokens = estimate_compacted_context(&summary, tail);

        let _ = tx
            .send(StreamEvent::Compacted {
                tokens_before,
                context_limit,
                summary: summary.clone(),
                thought_duration_ms,
            })
            .await;
        history::build_history_with_summary_tail(&summary, tail, &self.history_strategy)
    }
}
