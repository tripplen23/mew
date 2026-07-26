use std::sync::Arc;

use rig_core::completion::message::{AssistantContent, Message as RigMessage, UserContent};
use tokio::sync::mpsc;

use mewcode_engine::{
    EngineConfig, SkillRegistry,
    harness::{Harness, estimate_compacted_context, should_retry_after_compaction},
};
use mewcode_protocol::{Message, Mode, ModelId};

fn test_harness() -> Harness {
    Harness::new(
        ModelId::Gpt4o,
        Mode::Build,
        Arc::new(SkillRegistry::new()),
        Arc::new(mewcode_engine::tools::ToolRegistry::new()),
    )
}

fn test_config() -> EngineConfig {
    EngineConfig {
        api_key: "test-key".into(),
        openai_api_key: Some("test-openai-key".into()),
        openai_base_url: None,
        default_model: ModelId::Gpt4o,
        base_url: "https://example.invalid".into(),
    }
}

fn prior_messages() -> Vec<Message> {
    vec![
        Message::user(vec![mewcode_protocol::MessagePart::Text {
            text: "first user".into(),
        }]),
        Message::assistant(
            vec![mewcode_protocol::MessagePart::Text {
                text: "first assistant".into(),
            }],
            "test-model",
        ),
    ]
}

fn history_texts(history: &[RigMessage]) -> Vec<&str> {
    history
        .iter()
        .flat_map(|message| match message {
            RigMessage::User { content } => content
                .iter()
                .filter_map(|part| match part {
                    UserContent::Text(text) => Some(text.text.as_str()),
                    _ => None,
                })
                .collect::<Vec<_>>(),
            RigMessage::Assistant { content, .. } => content
                .iter()
                .filter_map(|part| match part {
                    AssistantContent::Text(text) => Some(text.text.as_str()),
                    _ => None,
                })
                .collect::<Vec<_>>(),
            _ => Vec::new(),
        })
        .collect()
}

#[tokio::test]
async fn invalid_checkpoint_falls_back_to_full_history() {
    let messages = prior_messages();
    let cfg = test_config();

    for boundary in [0, messages.len() + 1] {
        let mut harness = test_harness().with_compaction_summary(
            Some("invalid checkpoint".into()),
            boundary,
            messages.first().map(|message| message.id),
        );
        let (tx, _rx) = mpsc::channel(8);

        let history = harness.build_turn_history(&messages, &cfg, &tx).await;
        let texts = history_texts(&history);

        assert!(texts.contains(&"first user"));
        assert!(texts.contains(&"first assistant"));
        assert!(!texts.iter().any(|text| text.contains("invalid checkpoint")));
    }
}

#[tokio::test]
async fn checkpoint_requires_matching_boundary_message_id() {
    let messages = prior_messages();
    let cfg = test_config();

    for anchor in [None, Some(uuid::Uuid::new_v4())] {
        let mut harness =
            test_harness().with_compaction_summary(Some("stale summary".into()), 1, anchor);
        let (tx, _rx) = mpsc::channel(8);

        let history = harness.build_turn_history(&messages, &cfg, &tx).await;
        let texts = history_texts(&history);

        assert!(texts.contains(&"first user"));
        assert!(texts.contains(&"first assistant"));
        assert!(!texts.iter().any(|text| text.contains("stale summary")));
    }
}

#[tokio::test]
async fn checkpoint_with_matching_boundary_message_id_replaces_prefix() {
    let messages = prior_messages();
    let cfg = test_config();
    let mut harness = test_harness().with_compaction_summary(
        Some("valid summary".into()),
        1,
        Some(messages[0].id),
    );
    let (tx, _rx) = mpsc::channel(8);

    let history = harness.build_turn_history(&messages, &cfg, &tx).await;
    let texts = history_texts(&history);

    assert!(!texts.contains(&"first user"));
    assert!(texts.contains(&"first assistant"));
    assert!(texts.iter().any(|text| text.contains("valid summary")));
}

#[test]
fn observed_context_tokens_replace_previous_snapshot() {
    let mut harness = test_harness().with_session_tokens(90_000);

    harness.record_context_usage(1_200);
    assert_eq!(harness.session_tokens(), 1_200);

    harness.record_context_usage(800);
    assert_eq!(harness.session_tokens(), 800);
}

#[test]
fn compacted_context_estimate_counts_summary_and_tail_text() {
    let tail = vec![Message::user(vec![mewcode_protocol::MessagePart::Text {
        text: "12345678".into(),
    }])];

    assert_eq!(estimate_compacted_context("abcd", &tail), 3);
}

#[test]
fn context_overflow_retries_only_before_agent_activity() {
    use mewcode_engine::EngineError;
    use mewcode_engine::harness::AttemptError;

    let idle_overflow = AttemptError::new(EngineError::ContextOverflow("too large".into()), false);
    let active_overflow = AttemptError::new(EngineError::ContextOverflow("too large".into()), true);
    let idle_other = AttemptError::new(EngineError::Other("failed".into()), false);

    assert!(should_retry_after_compaction(&idle_overflow));
    assert!(!should_retry_after_compaction(&active_overflow));
    assert!(!should_retry_after_compaction(&idle_other));
}
