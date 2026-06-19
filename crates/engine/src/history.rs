//! Conversation history strategy and message mapping.
//!
//! [`HistoryStrategy`] controls how session messages are windowed before
//! being handed to the agent. The strategy is pluggable so future memory
//! modes (observational compaction, durable-fact-injected) can slot in
//! without changing the call site in [`crate::harness::Harness`].

use mewcode_protocol::Message as MewMessage;
use rig_core::OneOrMany;
use rig_core::completion::message::{AssistantContent, Message as RigMessage, Text, UserContent};

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
