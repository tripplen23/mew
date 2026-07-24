//! Conversation history strategy and message mapping.
//!
//! [`HistoryStrategy`] controls how session messages are windowed before
//! being handed to the agent. The strategy is pluggable so future memory
//! modes (observational compaction, durable-fact-injected) can slot in
//! without changing the call site in [`crate::harness::Harness`].

use mewcode_protocol::Message as MewMessage;
use rig_core::OneOrMany;
use rig_core::completion::message::{AssistantContent, Message as RigMessage, Text, UserContent};

/// Number of recent user-assistant turns to preserve verbatim during compaction.
pub const COMPACTION_PRESERVE_TURNS: usize = 2;

/// Fraction of context limit at which compaction triggers (75%).
pub const COMPACTION_THRESHOLD: f64 = 0.75;

/// Rough token-estimation heuristic used wherever actual token counts
/// aren't available (e.g. estimating savings from pruning before an LLM
/// reports real usage). Single source of truth so this ratio can't drift
/// between call sites.
pub const CHARS_PER_TOKEN: usize = 4;

/// How conversation history is presented to the agent.
///
/// The window is based on message count (`max_turns * 2` messages), not
/// token count. A true token-aware window is deferred until observational
/// memory lands in a later phase.
#[derive(Debug, Clone)]
pub enum HistoryStrategy {
    /// Pass up to `max_turns` most-recent conversation turns verbatim.
    /// Older turns beyond the window are dropped. Tool-result entries are
    /// also dropped (they carry no standalone meaning without the
    /// corresponding tool-call round).
    Raw { max_turns: usize },
}

impl HistoryStrategy {
    /// Default window: keep the last 20 user-assistant exchanges.
    pub const DEFAULT_MAX_TURNS: usize = 20;

    /// Build the default strategy.
    pub fn default_raw() -> Self {
        Self::Raw {
            max_turns: Self::DEFAULT_MAX_TURNS,
        }
    }

    /// Convert session messages into Rig messages, applying the window.
    /// Tool-result messages are excluded — they carry no standalone meaning.
    pub fn build(&self, messages: &[MewMessage]) -> Vec<RigMessage> {
        match self {
            Self::Raw { max_turns } => {
                // Walk from the end, collecting complete turns (user + assistant)
                // until we reach the window limit or run out of messages.
                // Tool-result messages are skipped entirely.
                let mut result: Vec<RigMessage> = Vec::new();
                let mut turns_collected = 0usize;

                for msg in messages.iter().rev() {
                    if msg.role == mewcode_protocol::Role::Tool {
                        continue;
                    }

                    let rig_msg = map_message(msg);
                    result.push(rig_msg);

                    // Each user or assistant message counts as half a turn.
                    // A complete turn is one user + one assistant.
                    if matches!(
                        msg.role,
                        mewcode_protocol::Role::User | mewcode_protocol::Role::Assistant
                    ) {
                        turns_collected += 1;
                        if turns_collected >= max_turns * 2 {
                            break;
                        }
                    }
                }

                result.reverse();
                result
            }
        }
    }
}

/// Map a single mewcode protocol message to a Rig completion message.
fn map_message(msg: &MewMessage) -> RigMessage {
    match msg.role {
        mewcode_protocol::Role::User => {
            let text = text_of(msg);
            RigMessage::User {
                content: OneOrMany::one(UserContent::Text(Text {
                    text,
                    additional_params: None,
                })),
            }
        }
        mewcode_protocol::Role::Assistant => {
            let text = text_of(msg);
            RigMessage::Assistant {
                id: None,
                content: OneOrMany::one(AssistantContent::Text(Text {
                    text,
                    additional_params: None,
                })),
            }
        }
        // Tool-result messages are filtered before reaching this function.
        mewcode_protocol::Role::Tool => unreachable!(),
    }
}

/// Concatenate all text parts of a message.
pub fn text_of(msg: &MewMessage) -> String {
    msg.parts
        .iter()
        .filter_map(|p| match p {
            mewcode_protocol::MessagePart::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("")
}

/// Split messages into head (to summarize) and tail (to preserve verbatim).
///
/// The tail contains the last `COMPACTION_PRESERVE_TURNS` user-assistant
/// exchanges. The head contains everything before that.
pub fn split_for_compaction(messages: &[MewMessage]) -> (&[MewMessage], &[MewMessage]) {
    // Walk backward to find the boundary: last N user/assistant pairs.
    let mut turns_found = 0usize;
    let mut boundary = 0;

    for (i, msg) in messages.iter().enumerate().rev() {
        if matches!(
            msg.role,
            mewcode_protocol::Role::User | mewcode_protocol::Role::Assistant
        ) {
            turns_found += 1;
            if turns_found >= COMPACTION_PRESERVE_TURNS * 2 {
                boundary = i;
                break;
            }
        }
    }

    (&messages[..boundary], &messages[boundary..])
}

/// Build a compacted history from a summary and preserved tail messages,
/// mapped verbatim (windowed by the default raw strategy).
///
/// Thin convenience wrapper over [`build_history_with_summary_tail`] for
/// call sites (and tests) that don't need a custom window.
pub fn build_compacted_history(summary: &str, tail: &[MewMessage]) -> Vec<RigMessage> {
    build_history_with_summary_tail(summary, tail, &HistoryStrategy::default_raw())
}

/// Prune messages to free up context space without calling LLM.
///
/// Removes:
/// - Tool result outputs (keeps tool call structure)
/// - File read contents (keeps path, removes content)
/// - Grep/search result contents (keeps summary, removes details)
///
/// Returns pruned messages. This is a free operation (no LLM cost).
pub fn prune_messages(messages: &[MewMessage]) -> Vec<MewMessage> {
    use mewcode_protocol::MessagePart;

    messages
        .iter()
        .map(|msg| {
            let pruned_parts: Vec<MessagePart> = msg
                .parts
                .iter()
                .filter_map(|part| match part {
                    // Keep text parts as-is
                    MessagePart::Text { .. } => Some(part.clone()),
                    // Keep tool calls but remove outputs
                    MessagePart::ToolCall(call) => Some(MessagePart::ToolCall(call.clone())),
                    // Remove tool results entirely (they're large and low-value)
                    MessagePart::ToolResult(_) => None,
                    // Keep file mentions but they're already lightweight
                    MessagePart::FileMention { .. } => Some(part.clone()),
                })
                .collect();

            MewMessage {
                id: msg.id,
                role: msg.role,
                parts: pruned_parts,
                model: msg.model.clone(),
                created_at: msg.created_at,
            }
        })
        .collect()
}

/// Build history from a stored compaction summary plus the messages that
/// come after the compaction boundary.
///
/// `summary` stands in for everything the caller has already folded away —
/// those original messages are never passed here, only `tail`. `tail` is
/// windowed by `raw_strategy` same as any other history build, so a long
/// uncovered tail still respects the normal turn-count window. This is what
/// makes compaction actually shrink what's sent to the model on the next
/// turn: the caller must persist the boundary (see
/// [`crate::harness::Harness::updated_compaction`]) so `tail` stays just the
/// uncovered remainder on subsequent turns, not the full history again.
pub fn build_history_with_summary_tail(
    summary: &str,
    tail: &[MewMessage],
    raw_strategy: &HistoryStrategy,
) -> Vec<RigMessage> {
    let mut result = Vec::new();

    result.push(RigMessage::User {
        content: OneOrMany::one(UserContent::Text(Text {
            text: format!(
                "[Previous conversation summary]\n\n{}\n\n[End of summary — continue from here]",
                summary
            ),
            additional_params: None,
        })),
    });
    result.push(RigMessage::Assistant {
        id: None,
        content: OneOrMany::one(AssistantContent::Text(Text {
            text: "Understood. I'll continue from where we left off.".to_string(),
            additional_params: None,
        })),
    });

    result.extend(raw_strategy.build(tail));
    result
}
