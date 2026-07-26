use mewcode_engine::EngineError;
use mewcode_engine::compaction::{COMPACTION_PRESERVE_TURNS, CompactionResult};
use mewcode_engine::harness::{
    CompactionCheckpoint, CompactionMode, CompactionState, accept_summary, should_compact_history,
};
use mewcode_protocol::{Message, MessagePart};

#[test]
fn forced_compaction_bypasses_threshold_but_requires_a_compactable_head() {
    let minimum = COMPACTION_PRESERVE_TURNS * 2;

    assert!(should_compact_history(
        CompactionMode::Forced,
        false,
        minimum + 1
    ));
    assert!(!should_compact_history(
        CompactionMode::Automatic,
        false,
        minimum + 1
    ));
    assert!(!should_compact_history(
        CompactionMode::Forced,
        false,
        minimum
    ));
}

/// A failed or empty compaction must be rejected outright, never turned into
/// a substitute summary. Accepting one would install a checkpoint whose
/// "summary" is not a summary, permanently dropping the real messages it
/// claims to replace from everything sent afterwards.
#[test]
fn failed_or_blank_compaction_is_rejected_instead_of_substituted() {
    let good = CompactionResult {
        summary: "**Objective**\n- ship".into(),
        thought_duration_ms: 12,
        tokens_before: 100,
        context_limit: 200_000,
    };
    assert_eq!(
        accept_summary(Ok(good)).expect("usable summary"),
        ("**Objective**\n- ship".to_string(), 12)
    );

    let blank = CompactionResult {
        summary: "  \n ".into(),
        thought_duration_ms: 5,
        tokens_before: 100,
        context_limit: 200_000,
    };
    assert!(
        accept_summary(Ok(blank)).is_err(),
        "blank summary is unusable"
    );

    // The provider's own error must survive, not be flattened into a generic
    // "nothing to compact" — that misdiagnosis is what this preserves.
    let error = accept_summary(Err(EngineError::Other("provider timeout".into())))
        .expect_err("a failed compaction must not yield a summary");
    assert!(error.to_string().contains("provider timeout"));
}

#[test]
fn pending_checkpoint_survives_repeated_history_validation() {
    let messages = vec![Message::user(vec![MessagePart::Text {
        text: "covered".into(),
    }])];
    let checkpoint = CompactionCheckpoint::new(Some("summary".into()), 1, Some(messages[0].id))
        .expect("valid checkpoint");
    let mut state = CompactionState {
        checkpoint: Some(checkpoint.clone()),
        pending_update: Some(checkpoint),
        ..Default::default()
    };

    assert!(state.checkpoint_for_history(&messages).is_some());
    assert!(state.checkpoint_for_history(&messages).is_some());
    assert!(state.pending_update.is_some());
}

#[test]
fn blank_generated_summary_does_not_advance_checkpoint() {
    let mut state = CompactionState::default();

    assert!(!state.install_checkpoint(" \n ".into(), 4, uuid::Uuid::new_v4()));
    assert!(state.checkpoint.is_none());
    assert!(state.pending_update.is_none());
}

#[test]
fn forged_zero_boundary_checkpoint_is_rejected() {
    let messages = vec![Message::user(vec![MessagePart::Text {
        text: "first".into(),
    }])];
    let checkpoint = CompactionCheckpoint {
        summary: "forged".into(),
        up_to: 0,
        compacted_up_to_message_id: messages[0].id,
    };
    let mut state = CompactionState {
        checkpoint: Some(checkpoint.clone()),
        pending_update: Some(checkpoint),
        ..Default::default()
    };

    assert!(state.checkpoint_for_history(&messages).is_none());
    assert!(state.checkpoint.is_none());
    assert!(state.pending_update.is_none());
}
