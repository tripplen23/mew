//! Tests for [`HistoryStrategy`] message windowing and protocol mapping.

use mewcode_engine::history::HistoryStrategy;
use mewcode_protocol::{Message, MessagePart, Role};
use rig_core::completion::Message as RigMessage;
use rig_core::completion::message::UserContent;
use uuid::Uuid;

fn user_msg(text: &str) -> Message {
    Message {
        id: Uuid::new_v4(),
        role: Role::User,
        parts: vec![MessagePart::Text {
            text: text.to_string(),
        }],
        model: None,
        created_at: chrono::Utc::now(),
    }
}

fn assistant_msg(text: &str) -> Message {
    Message {
        id: Uuid::new_v4(),
        role: Role::Assistant,
        parts: vec![MessagePart::Text {
            text: text.to_string(),
        }],
        model: Some("minimax-m3".into()),
        created_at: chrono::Utc::now(),
    }
}

fn text_of_rig(msg: &RigMessage) -> String {
    match msg {
        RigMessage::User { content } => content
            .iter()
            .filter_map(|c| match c {
                UserContent::Text(t) => Some(t.text.as_str()),
                _ => None,
            })
            .collect(),
        RigMessage::Assistant { content, .. } => content
            .iter()
            .filter_map(|c| match c {
                rig_core::completion::message::AssistantContent::Text(t) => Some(t.text.as_str()),
                _ => None,
            })
            .collect(),
        RigMessage::System { content } => content.clone(),
    }
}

#[test]
fn raw_strategy_within_window_preserves_all_messages() {
    let messages = vec![
        user_msg("hello"),
        assistant_msg("hi there"),
        user_msg("how are you"),
        assistant_msg("I'm good"),
    ];
    let strategy = HistoryStrategy::Raw { max_turns: 10 };
    let result = strategy.build(&messages);

    assert_eq!(result.len(), 4);
    assert_eq!(text_of_rig(&result[0]), "hello");
    assert_eq!(text_of_rig(&result[3]), "I'm good");
}

#[test]
fn raw_strategy_truncates_old_turns() {
    let mut messages = Vec::new();
    for i in 0..6 {
        messages.push(user_msg(&format!("user msg {}", i)));
        messages.push(assistant_msg(&format!("assistant msg {}", i)));
    }
    // 12 messages = 6 turns, window of 2 turns = keep last 4 messages
    let strategy = HistoryStrategy::Raw { max_turns: 2 };
    let result = strategy.build(&messages);

    assert_eq!(result.len(), 4);
    assert_eq!(text_of_rig(&result[0]), "user msg 4");
    assert_eq!(text_of_rig(&result[3]), "assistant msg 5");
}

#[test]
fn empty_history_yields_empty_result() {
    let strategy = HistoryStrategy::Raw { max_turns: 20 };
    let result = strategy.build(&[]);
    assert!(result.is_empty());
}

#[test]
fn single_message_under_window_is_preserved() {
    let messages = vec![user_msg("only me")];
    let strategy = HistoryStrategy::Raw { max_turns: 5 };
    let result = strategy.build(&messages);
    assert_eq!(result.len(), 1);
    assert_eq!(text_of_rig(&result[0]), "only me");
}
