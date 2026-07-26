use mewcode_protocol::{Message, StreamEvent};
use tokio::sync::mpsc;

use super::Harness;
use crate::compaction;
use crate::config::EngineConfig;
use crate::error::EngineError;

/// A summary paired with the exact message-prefix boundary it replaces.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompactionCheckpoint {
    pub summary: String,
    pub up_to: usize,
    pub compacted_up_to_message_id: uuid::Uuid,
}

impl CompactionCheckpoint {
    /// Returns `None` unless summary, boundary, and boundary identity are complete.
    pub fn new(
        summary: Option<String>,
        up_to: usize,
        compacted_up_to_message_id: Option<uuid::Uuid>,
    ) -> Option<Self> {
        let summary = summary?.trim().to_string();
        if summary.is_empty() || up_to == 0 {
            return None;
        }
        Some(Self {
            summary,
            up_to,
            compacted_up_to_message_id: compacted_up_to_message_id?,
        })
    }
}

/// Mutable context-compaction state for one harness.
#[derive(Debug, Clone, Default)]
pub struct CompactionState {
    pub context_tokens: u64,
    pub checkpoint: Option<CompactionCheckpoint>,
    pub pending_update: Option<CompactionCheckpoint>,
}

impl CompactionState {
    #[doc(hidden)]
    pub fn checkpoint_for_history(&mut self, messages: &[Message]) -> Option<CompactionCheckpoint> {
        match self.checkpoint.clone() {
            Some(checkpoint)
                if checkpoint.up_to > 0
                    && messages.get(checkpoint.up_to - 1).is_some_and(|message| {
                        message.id == checkpoint.compacted_up_to_message_id
                    }) =>
            {
                Some(checkpoint)
            }
            Some(_) => {
                self.checkpoint = None;
                self.pending_update = None;
                None
            }
            None => None,
        }
    }

    /// Install a new checkpoint. Returns `false` when the summary is blank or empty.
    #[doc(hidden)]
    pub fn install_checkpoint(
        &mut self,
        summary: String,
        up_to: usize,
        compacted_up_to_message_id: uuid::Uuid,
    ) -> bool {
        let Some(checkpoint) =
            CompactionCheckpoint::new(Some(summary), up_to, Some(compacted_up_to_message_id))
        else {
            return false;
        };
        self.checkpoint = Some(checkpoint.clone());
        self.pending_update = Some(checkpoint);
        true
    }
}

/// Why a turn could not be given a freshly compacted history.
#[derive(Debug)]
#[doc(hidden)]
pub enum CompactionBlocked {
    /// There is not enough uncovered history to fold into a summary.
    NothingToCompact,
    /// Compaction ran but produced no usable summary; carries the cause.
    SummaryUnavailable(EngineError),
}

/// Accept a compaction result only if it actually produced a summary.
///
/// Fail closed: a provider error or an empty summary is an error, and the
/// caller then keeps the existing boundary rather than installing a
/// checkpoint. There is deliberately no concatenation fallback — a truncated
/// transcript dump is not a summary, and installing it as one would
/// permanently drop the messages it claims to replace from every later turn.
///
/// The provider error is returned rather than logged and discarded, so a
/// forced compaction can report *why* it failed instead of claiming there was
/// nothing left to compact.
#[doc(hidden)]
pub fn accept_summary(
    result: Result<compaction::CompactionResult, EngineError>,
) -> Result<(String, u64), EngineError> {
    match result {
        Ok(result) if !result.summary.trim().is_empty() => {
            Ok((result.summary, result.thought_duration_ms))
        }
        Ok(_) => Err(EngineError::Other(
            "compaction returned an empty summary".into(),
        )),
        Err(error) => Err(error),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[doc(hidden)]
pub enum CompactionMode {
    Automatic,
    Forced,
}

#[doc(hidden)]
pub fn should_compact_history(
    mode: CompactionMode,
    automatic_triggered: bool,
    uncovered_messages: usize,
) -> bool {
    (mode == CompactionMode::Forced || automatic_triggered)
        && uncovered_messages > compaction::COMPACTION_PRESERVE_TURNS * 2
}

impl Harness {
    /// Check if compaction should trigger based on estimated context usage.
    fn should_compact(&self) -> bool {
        let limit = self.model.context_limit();
        if limit == 0 {
            return false;
        }
        let threshold = (limit as f64 * compaction::COMPACTION_THRESHOLD) as u64;
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
        // `Automatic` never reports blocked — it degrades to the uncompacted
        // history instead — so the fallback here is belt-and-braces only.
        self.build_turn_history_mode(prior_messages, cfg, tx, CompactionMode::Automatic)
            .await
            .unwrap_or_else(|_| self.history_strategy.build(prior_messages))
    }

    pub(super) async fn build_forced_turn_history(
        &mut self,
        prior_messages: &[Message],
        cfg: &EngineConfig,
        tx: &mpsc::Sender<StreamEvent>,
    ) -> Result<Vec<rig_core::completion::Message>, CompactionBlocked> {
        self.build_turn_history_mode(prior_messages, cfg, tx, CompactionMode::Forced)
            .await
    }

    /// History to send when no new summary was produced — either compaction
    /// was not needed, or it failed and the boundary must stay put.
    ///
    /// `Forced` returns `Err`: the caller asked for compaction specifically
    /// because the context already overflowed, so handing back the same
    /// oversized history would just fail again. Propagating the reason lets it
    /// report the real cause instead of a generic overflow.
    #[doc(hidden)]
    pub fn history_without_new_summary(
        &self,
        mode: CompactionMode,
        prior_messages: &[Message],
        checkpoint: Option<&CompactionCheckpoint>,
        uncovered: &[Message],
        blocked: CompactionBlocked,
    ) -> Result<Vec<rig_core::completion::Message>, CompactionBlocked> {
        if mode == CompactionMode::Forced {
            return Err(blocked);
        }
        Ok(match checkpoint {
            Some(checkpoint) => compaction::build_history_with_summary_tail(
                &checkpoint.summary,
                uncovered,
                &self.history_strategy,
            ),
            None => self.history_strategy.build(prior_messages),
        })
    }

    async fn build_turn_history_mode(
        &mut self,
        prior_messages: &[Message],
        cfg: &EngineConfig,
        tx: &mpsc::Sender<StreamEvent>,
        mode: CompactionMode,
    ) -> Result<Vec<rig_core::completion::Message>, CompactionBlocked> {
        // A checkpoint is valid only for the transcript prefix it references.
        // Fail closed to full history when persisted metadata is stale.
        let checkpoint = self.compaction.checkpoint_for_history(prior_messages);
        let compacted_up_to = checkpoint.as_ref().map_or(0, |value| value.up_to);
        let uncovered = &prior_messages[compacted_up_to..];
        let needs_compaction = should_compact_history(mode, self.should_compact(), uncovered.len());
        if !needs_compaction {
            return self.history_without_new_summary(
                mode,
                prior_messages,
                checkpoint.as_ref(),
                uncovered,
                CompactionBlocked::NothingToCompact,
            );
        }

        // Prune tool results, then split into head (for LLM summary) and
        // tail (kept verbatim).
        let pruned = compaction::prune_messages(uncovered);
        let (compact_head, tail) = compaction::split_for_compaction(&pruned);
        let tokens_before = self.compaction.context_tokens;
        let context_limit = self.model.context_limit();
        tracing::info!(
            head_count = compact_head.len(),
            tail_count = tail.len(),
            tokens_before,
            context_limit,
            "compacting history with LLM after prune"
        );

        let previous_summary = checkpoint
            .as_ref()
            .map(|checkpoint| checkpoint.summary.as_str());
        let result = compaction::compact_history(
            compact_head,
            self.model,
            cfg,
            self.memory.clone(),
            tokens_before,
            previous_summary,
            tx,
        )
        .await;
        // Fail closed: without a real summary the boundary must not move, so
        // this turn keeps the full history instead of silently losing the
        // messages the summary would have replaced.
        let (summary, thought_duration_ms) = match accept_summary(result) {
            Ok(accepted) => accepted,
            Err(error) => {
                tracing::warn!(%error, "compaction produced no usable summary");
                return self.history_without_new_summary(
                    mode,
                    prior_messages,
                    checkpoint.as_ref(),
                    uncovered,
                    CompactionBlocked::SummaryUnavailable(error),
                );
            }
        };

        // The new boundary: everything already covered, plus the messages
        // just folded into this summary. `pruned` mirrors `uncovered`
        // message-for-message (pruning only strips parts, never whole
        // messages), so `compact_head.len()` maps directly onto
        // `uncovered`/`prior_messages` indices.
        let new_up_to = compacted_up_to + compact_head.len();
        let Some(compacted_up_to_message_id) = compact_head.last().map(|message| message.id) else {
            tracing::warn!("compaction produced an empty head; keeping uncompressed history");
            return self.history_without_new_summary(
                mode,
                prior_messages,
                checkpoint.as_ref(),
                uncovered,
                CompactionBlocked::NothingToCompact,
            );
        };
        if !self.compaction.install_checkpoint(
            summary.clone(),
            new_up_to,
            compacted_up_to_message_id,
        ) {
            // `accept_summary` already rejected blank summaries, so this is
            // unreachable; fail closed rather than send a phantom summary.
            debug_assert!(false, "accepted summary must create a checkpoint");
            return self.history_without_new_summary(
                mode,
                prior_messages,
                checkpoint.as_ref(),
                uncovered,
                CompactionBlocked::SummaryUnavailable(EngineError::Other(
                    "compaction checkpoint could not be installed".into(),
                )),
            );
        }
        self.compaction.context_tokens = compaction::estimate_compacted_context(&summary, tail);

        let _ = tx
            .send(StreamEvent::Compacted {
                tokens_before,
                context_limit,
                summary: summary.clone(),
                thought_duration_ms,
            })
            .await;
        Ok(compaction::build_history_with_summary_tail(
            &summary,
            tail,
            &self.history_strategy,
        ))
    }
}
