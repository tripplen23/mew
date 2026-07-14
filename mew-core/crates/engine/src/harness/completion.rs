//! Pure helpers for selecting the user prompt for a harness turn. These stay
//! free of the runtime, the network, and the mpsc channel.

use mewcode_protocol::{Message, MessagePart, Role};

/// Text of the most recent [`Role::User`] message or
/// `None` when the history holds no user message.
pub fn last_user_text(messages: &[Message]) -> Option<String> {
    messages
        .iter()
        .rev()
        .find(|m| m.role == Role::User)
        .map(|m| {
            m.parts
                .iter()
                .filter_map(|p| match p {
                    MessagePart::Text { text } => Some(text.as_str()),
                    _ => None,
                })
                .collect()
        })
}
