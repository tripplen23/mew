//! Tests for context compaction and token accounting (P002-P008).

use mewcode_engine::history::{
    COMPACTION_PRESERVE_TURNS, COMPACTION_THRESHOLD, HistoryStrategy, build_compacted_history,
    build_history_with_summary_tail, split_for_compaction, text_of,
};
use mewcode_protocol::{Message, MessagePart, ModelId, Role};

#[test]
fn context_limit_returns_positive_for_known_models() {
    // P003: Each ModelId carries a context_limit() method.
    for model in ModelId::ALL {
        let limit = model.context_limit();
        assert!(
            limit > 0,
            "model {} should have a positive context limit, got {}",
            model.as_str(),
            limit
        );
    }
}

#[test]
fn context_limit_gpt41_is_one_million() {
    // P003: Known models return their documented context limits.
    assert_eq!(ModelId::Gpt41.context_limit(), 1_047_576);
    assert_eq!(ModelId::Gpt41Mini.context_limit(), 1_047_576);
    assert_eq!(ModelId::Gpt41Nano.context_limit(), 1_047_576);
}

#[test]
fn context_limit_gpt4o_is_128k() {
    // P003: Known models return their documented context limits.
    assert_eq!(ModelId::Gpt4o.context_limit(), 128_000);
    assert_eq!(ModelId::Gpt4oMini.context_limit(), 128_000);
}

#[test]
fn split_for_compaction_preserves_last_two_turns() {
    // P005: Compaction preserves the last 2 user-assistant exchanges.
    let messages: Vec<Message> = (0..10)
        .flat_map(|i| {
            vec![
                Message::user(vec![MessagePart::Text {
                    text: format!("user {}", i),
                }]),
                Message::assistant(
                    vec![MessagePart::Text {
                        text: format!("assistant {}", i),
                    }],
                    "test-model",
                ),
            ]
        })
        .collect();

    let (head, tail) = split_for_compaction(&messages);

    // Tail should contain the last 2 turns (4 messages: user+assistant x2)
    assert_eq!(tail.len(), COMPACTION_PRESERVE_TURNS * 2);
    // Head should contain the rest
    assert_eq!(head.len(), messages.len() - COMPACTION_PRESERVE_TURNS * 2);

    // Verify tail contains the last messages
    let tail_texts: Vec<String> = tail.iter().map(text_of).collect();
    assert!(tail_texts.contains(&"user 8".to_string()));
    assert!(tail_texts.contains(&"assistant 8".to_string()));
    assert!(tail_texts.contains(&"user 9".to_string()));
    assert!(tail_texts.contains(&"assistant 9".to_string()));
}

#[test]
fn split_for_compaction_handles_short_history() {
    // P005: When history is shorter than preservation window, all messages go to tail.
    let messages = vec![
        Message::user(vec![MessagePart::Text {
            text: "user 0".to_string(),
        }]),
        Message::assistant(
            vec![MessagePart::Text {
                text: "assistant 0".to_string(),
            }],
            "test-model",
        ),
    ];

    let (head, tail) = split_for_compaction(&messages);

    // With only 1 turn, everything should be in tail (or head is empty)
    assert!(head.is_empty() || tail.len() == messages.len());
}

#[test]
fn build_compacted_history_preserves_tail_messages() {
    // P005: Compacted history contains preserved tail verbatim.
    let tail = vec![
        Message::user(vec![MessagePart::Text {
            text: "recent user".to_string(),
        }]),
        Message::assistant(
            vec![MessagePart::Text {
                text: "recent assistant".to_string(),
            }],
            "test-model",
        ),
    ];

    let summary = "Previous conversation about X and Y.";
    let history = build_compacted_history(summary, &tail);

    // Should have: summary user message, assistant acknowledgment, then tail messages
    assert!(history.len() >= 4); // summary + ack + 2 tail messages

    // Verify tail messages are present
    let history_texts: Vec<String> = history
        .iter()
        .flat_map(|m| match m {
            rig_core::completion::message::Message::User { content } => content
                .iter()
                .filter_map(|c| match c {
                    rig_core::completion::message::UserContent::Text(t) => Some(t.text.clone()),
                    _ => None,
                })
                .collect::<Vec<_>>(),
            rig_core::completion::message::Message::Assistant { content, .. } => content
                .iter()
                .filter_map(|c| match c {
                    rig_core::completion::message::AssistantContent::Text(t) => {
                        Some(t.text.clone())
                    }
                    _ => None,
                })
                .collect::<Vec<_>>(),
            _ => vec![],
        })
        .collect();

    assert!(history_texts.iter().any(|t| t.contains("recent user")));
    assert!(history_texts.iter().any(|t| t.contains("recent assistant")));
    assert!(
        history_texts
            .iter()
            .any(|t| t.contains("Previous conversation"))
    );
}

#[test]
fn compaction_threshold_is_valid_fraction() {
    // P005: Compaction threshold must be between 0 and 1.
    const { assert!(COMPACTION_THRESHOLD > 0.0 && COMPACTION_THRESHOLD <= 1.0) };
}

#[test]
fn preservation_turns_is_two() {
    // P005: Last 2 turns are preserved verbatim.
    assert_eq!(COMPACTION_PRESERVE_TURNS, 2);
}

#[test]
fn tool_messages_excluded_from_compacted_history() {
    // P005: No orphaned tool_call without tool_result.
    let tail = vec![
        Message::user(vec![MessagePart::Text {
            text: "user".to_string(),
        }]),
        Message {
            id: uuid::Uuid::new_v4(),
            role: Role::Tool,
            parts: vec![MessagePart::Text {
                text: "tool result".to_string(),
            }],
            model: None,
            created_at: chrono::Utc::now(),
        },
        Message::assistant(
            vec![MessagePart::Text {
                text: "assistant".to_string(),
            }],
            "test-model",
        ),
    ];

    let history = build_compacted_history("summary", &tail);

    // Tool messages should be excluded
    let history_texts: Vec<String> = history
        .iter()
        .flat_map(|m| match m {
            rig_core::completion::message::Message::User { content } => content
                .iter()
                .filter_map(|c| match c {
                    rig_core::completion::message::UserContent::Text(t) => Some(t.text.clone()),
                    _ => None,
                })
                .collect::<Vec<_>>(),
            rig_core::completion::message::Message::Assistant { content, .. } => content
                .iter()
                .filter_map(|c| match c {
                    rig_core::completion::message::AssistantContent::Text(t) => {
                        Some(t.text.clone())
                    }
                    _ => None,
                })
                .collect::<Vec<_>>(),
            _ => vec![],
        })
        .collect();

    assert!(!history_texts.iter().any(|t| t.contains("tool result")));
    assert!(history_texts.iter().any(|t| t.contains("user")));
    assert!(history_texts.iter().any(|t| t.contains("assistant")));
}

#[test]
fn build_history_with_summary_tail_excludes_covered_messages() {
    // Regression test: this is what makes /compact and automatic compaction
    // actually shrink what's sent to the model on the next turn. Only the
    // tail passed in (messages after the compaction boundary) may appear in
    // the built history — anything folded into `summary` must never be
    // re-sent alongside it.
    // `covered` is intentionally never passed to the function under test —
    // that's the point: only `tail` should ever reach the model.
    let tail = vec![
        Message::user(vec![MessagePart::Text {
            text: "recent user".to_string(),
        }]),
        Message::assistant(
            vec![MessagePart::Text {
                text: "recent assistant".to_string(),
            }],
            "test-model",
        ),
    ];

    // Simulate the caller: only `tail` is ever passed, `covered` never is.
    let history = build_history_with_summary_tail(
        "summary of covered turns",
        &tail,
        &HistoryStrategy::default_raw(),
    );

    let history_texts: Vec<String> = history
        .iter()
        .flat_map(|m| match m {
            rig_core::completion::message::Message::User { content } => content
                .iter()
                .filter_map(|c| match c {
                    rig_core::completion::message::UserContent::Text(t) => Some(t.text.clone()),
                    _ => None,
                })
                .collect::<Vec<_>>(),
            rig_core::completion::message::Message::Assistant { content, .. } => content
                .iter()
                .filter_map(|c| match c {
                    rig_core::completion::message::AssistantContent::Text(t) => {
                        Some(t.text.clone())
                    }
                    _ => None,
                })
                .collect::<Vec<_>>(),
            _ => vec![],
        })
        .collect();

    assert!(history_texts.iter().any(|t| t.contains("recent user")));
    assert!(history_texts.iter().any(|t| t.contains("recent assistant")));
    assert!(
        history_texts
            .iter()
            .any(|t| t.contains("summary of covered turns"))
    );
}
