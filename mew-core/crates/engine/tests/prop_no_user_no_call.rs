//! Property-based test: a history with no user message produces no agent call.
//!
//! Feature: session-flow-and-engine-v0, a history with no `Role::User`
//! message yields no agent invocation — `last_user_text` is `None` and `run_turn`
//! fails the turn before any provider is built or any request is issued,
//! so the stream channel never sees an event.
//!
//! The assertion is deliberately credential-independent: `run_turn` only emits
//! on the success path, so whether resolution fails for a missing key or
//! the turn fails for a missing user message, the channel stays empty.

use std::sync::Arc;

use mewcode_engine::skills::SkillRegistry;
use mewcode_engine::tools::ToolRegistry;
use mewcode_engine::{Harness, harness::last_user_text};
use mewcode_protocol::{Message, MessagePart, Mode, ModelId, StreamEvent};
use proptest::prelude::*;
use tokio::sync::mpsc;

/// A message's parts: each entry is text (`true`) or a non-text file mention
/// (`false`). Mixing them keeps the generated assistant/tool history realistic.
fn parts_strategy() -> impl Strategy<Value = Vec<(bool, String)>> {
    proptest::collection::vec((any::<bool>(), "[a-z ]{0,8}"), 0..6)
}

fn to_parts(spec: &[(bool, String)]) -> Vec<MessagePart> {
    spec.iter()
        .map(|(is_text, s)| {
            if *is_text {
                MessagePart::Text { text: s.clone() }
            } else {
                MessagePart::FileMention { path: s.clone() }
            }
        })
        .collect()
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    #[test]
    fn no_user_message_yields_no_agent_invocation(
        // Every message is an assistant message, so the history holds zero
        // `Role::User` messages by construction.
        history_spec in proptest::collection::vec(parts_strategy(), 0..8),
    ) {
        let history: Vec<Message> = history_spec
            .iter()
            .map(|parts| Message::assistant(to_parts(parts), "test-model"))
            .collect();

        // No user message => nothing to answer.
        prop_assert_eq!(last_user_text(&history), None);

        let harness = Harness::new(
            ModelId::MiniMaxM3,
            Mode::Build,
            Arc::new(SkillRegistry::new()),
            Arc::new(ToolRegistry::new()),
        );

        // `run_turn` is async and proptest is sync: one runtime per case.
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("build current-thread runtime");

        let (tx, mut rx) = mpsc::channel::<StreamEvent>(16);
        let result = rt.block_on(harness.run_turn(&history, tx));

        // The turn fails (no user message), and `run_turn` dropped its `tx`,
        // so draining the receiver sees every event that ever reached it.
        prop_assert!(result.is_err(), "turn must fail without a user message");

        let mut observed = Vec::new();
        while let Ok(event) = rx.try_recv() {
            observed.push(event);
        }
        prop_assert!(
            observed.is_empty(),
            "no provider call: expected empty channel, got {observed:?}"
        );
    }
}
