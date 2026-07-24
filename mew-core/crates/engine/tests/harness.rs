use std::sync::Arc;

use rig_core::completion::message::{AssistantContent, Message as RigMessage, UserContent};
use tokio::sync::mpsc;

use mewcode_engine::{
    EngineConfig, SkillRegistry,
    harness::{
        CompactionCheckpoint, CompactionState, Harness, estimate_compacted_context,
        truncate_fallback,
    },
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
        let mut harness =
            test_harness().with_compaction_summary(Some("invalid checkpoint".into()), boundary);
        let (tx, _rx) = mpsc::channel(8);

        let history = harness.build_turn_history(&messages, &cfg, &tx).await;
        let texts = history_texts(&history);

        assert!(texts.contains(&"first user"));
        assert!(texts.contains(&"first assistant"));
        assert!(!texts.iter().any(|text| text.contains("invalid checkpoint")));
    }
}

#[test]
fn fallback_truncation_preserves_utf8_boundary() {
    let input = format!("{}é-tail", "a".repeat(7_999));

    let output = truncate_fallback(input.clone(), 8_000);
    let prefix = output
        .strip_suffix("\n[...truncated]")
        .expect("truncated output should carry its marker");

    assert!(output.len() <= 8_000);
    assert!(prefix.len() <= 8_000);
    assert!(input.starts_with(prefix));
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

#[tokio::test]
async fn pending_checkpoint_survives_repeated_history_preparation() {
    let messages = prior_messages();
    let checkpoint =
        CompactionCheckpoint::new(Some("summary".into()), 1).expect("valid checkpoint");
    let mut harness = test_harness();
    harness.compaction.checkpoint = Some(checkpoint.clone());
    harness.compaction.pending_update = Some(checkpoint);
    let cfg = test_config();
    let (tx, _rx) = mpsc::channel(8);

    let _ = harness.build_turn_history(&messages, &cfg, &tx).await;
    assert_eq!(harness.updated_compaction(), Some(("summary", 1)));

    let _ = harness.build_turn_history(&messages, &cfg, &tx).await;
    assert_eq!(harness.updated_compaction(), Some(("summary", 1)));
}

#[test]
fn blank_generated_summary_does_not_advance_checkpoint() {
    let mut state = CompactionState::default();

    assert!(!state.install_checkpoint(" \n ".into(), 4));
    assert!(state.checkpoint.is_none());
    assert!(state.pending_update.is_none());
}
