//! Property-based test: the single turn uses the most recent user message.
//!
//! Feature: session-flow-and-engine-v0, Property 12: The turn uses the most
//! recent user message — for any history holding at least one `Role::User`
//! message, `last_user_text` returns that message's text (its `Text` parts
//! concatenated in order, with non-text parts dropped).

use proptest::prelude::*;

use mewcode_engine::harness::last_user_text;
use mewcode_protocol::{Message, MessagePart};

/// A message's parts: each entry is either text (`true`) or a non-text file
/// mention (`false`). Mixing the two exercises `last_user_text`'s text-only
/// fold — non-text parts must never leak into the result.
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

/// The oracle: the concatenated text of a parts spec, in order.
fn expected_text(spec: &[(bool, String)]) -> String {
    spec.iter()
        .filter(|(is_text, _)| *is_text)
        .map(|(_, s)| s.as_str())
        .collect()
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    #[test]
    fn turn_uses_most_recent_user_message(
        before in proptest::collection::vec((any::<bool>(), parts_strategy()), 0..5),
        target in parts_strategy(),
        after in proptest::collection::vec(parts_strategy(), 0..5),
    ) {
        let mut history: Vec<Message> = Vec::new();

        // Earlier messages of arbitrary role (user or assistant). Any user
        // messages here are older than `target`.
        for (is_user, parts) in &before {
            let parts = to_parts(parts);
            history.push(if *is_user {
                Message::user(parts)
            } else {
                Message::assistant(parts, "test-model")
            });
        }

        // The most recent user message — its concatenated text is the oracle.
        history.push(Message::user(to_parts(&target)));

        // Later messages are never `User`, so `target` stays the most recent
        // user message in the history.
        for parts in &after {
            history.push(Message::assistant(to_parts(parts), "test-model"));
        }

        prop_assert_eq!(last_user_text(&history), Some(expected_text(&target)));
    }
}
