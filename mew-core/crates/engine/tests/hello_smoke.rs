//! Opt-in live smoke test: one real chat turn end-to-end against OpenCode Go.
//!
//! Ignored by default so a plain `cargo test` stays offline and fast; run it
//! explicitly with `cargo test -p mewcode-engine -- --ignored`. With a non-blank
//! `OPENCODE_GO_API_KEY` it drives one turn against the live endpoint using the
//! default Anthropic-compatible model and asserts the assembled reply is
//! non-empty with exactly one `Finish` and no `Error`. With a blank/unset key it
//! returns early naming the variable and issues no request, so an explicit
//! `--ignored` run without a key neither hangs nor hits the network.

use std::sync::Arc;
use std::time::Duration;

use mewcode_engine::Harness;
use mewcode_engine::skills::SkillRegistry;
use mewcode_engine::tools::ToolRegistry;
use mewcode_protocol::env::OPENCODE_GO_API_KEY;
use mewcode_protocol::{Message, MessagePart, Mode, ModelId, ModelKind, StreamEvent};
use tokio::sync::mpsc;

#[tokio::test]
#[ignore = "live: hits OpenCode Go and needs a non-blank OPENCODE_GO_API_KEY"]
async fn hello_round_trip() {
    // A blank/unset/whitespace key terminates early, names the variable,
    // and issues no request — no provider is built, nothing touches the network.
    let key_present = std::env::var(OPENCODE_GO_API_KEY)
        .ok()
        .is_some_and(|v| !v.trim().is_empty());
    if !key_present {
        eprintln!(
            "skipping hello smoke test: set {OPENCODE_GO_API_KEY} to a non-blank value to run it"
        );
        return;
    }

    // Drive the default Anthropic-compatible model.
    let model = ModelId::DEFAULT;
    assert_eq!(
        model.kind(),
        ModelKind::AnthropicMessages,
        "smoke test must drive the default Anthropic-compatible model"
    );

    let harness = Harness::new(
        model,
        Mode::Build,
        Arc::new(SkillRegistry::load_defaults()),
        Arc::new(ToolRegistry::new()),
    );

    let history = vec![Message::user(vec![MessagePart::Text {
        text: "Say hello in one short sentence.".to_string(),
    }])];

    // Mirror the server's channel buffer (64) and drain concurrently with
    // `run_turn`, which only returns after it has sent `Finish`; receiving in
    // parallel avoids any buffer-fill deadlock.
    let (tx, mut rx) = mpsc::channel::<StreamEvent>(64);
    let turn = tokio::spawn(async move { harness.run_turn(&history, tx).await });

    let collect = async {
        let mut reply = String::new();
        let mut finishes = 0usize;
        let mut errors = 0usize;
        while let Some(event) = rx.recv().await {
            match event {
                StreamEvent::TextDelta { delta } => reply.push_str(&delta),
                StreamEvent::Finish { .. } => finishes += 1,
                StreamEvent::Error { .. } => errors += 1,
                _ => {}
            }
        }
        (reply, finishes, errors)
    };

    // Bound the whole round-trip; fail with a timeout rather than block.
    let (reply, finishes, errors) = tokio::time::timeout(Duration::from_secs(60), collect)
        .await
        .expect("live round-trip exceeded the 60s timeout");

    let turn_result = turn.await.expect("run_turn task panicked");
    assert!(turn_result.is_ok(), "run_turn failed: {turn_result:?}");

    // Non-empty reply, exactly one Finish, no Error.
    assert!(
        !reply.trim().is_empty(),
        "assembled reply must be non-empty"
    );
    assert_eq!(finishes, 1, "expected exactly one Finish event");
    assert_eq!(errors, 0, "expected no Error event");
}
