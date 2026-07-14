//! Property-based test: a failed turn's error message never leaks the API key.
//!
//! Feature: session-flow-and-engine-v0, Property 17: for any sentinel
//! `OPENCODE_GO_API_KEY` value, the `Error` message emitted for a failed turn
//! never contains that value as a substring. The `Error` event's message is
//! `EngineError::to_string()`, so the guarantee reduces to: `EngineError`'s
//! `Display` never formats `api_key`. We exercise the real failure path — a key
//! IS present, the turn gets far enough to build a provider and issue a request,
//! and that request fails — then assert the resulting `EngineError` (and the
//! `Error.message` the handler would emit, which is identical) is key-free.
//!
//! The failure is forced deterministically and offline by pointing
//! `MEWCODE_ENGINE_BASE_URL` at a closed local endpoint (`http://127.0.0.1:1`),
//! so the transport errors immediately without any network.

use std::sync::Arc;

use mewcode_engine::skills::SkillRegistry;
use mewcode_engine::tools::ToolRegistry;
use mewcode_engine::{Harness, config::ENV_BASE_URL};
use mewcode_protocol::env::OPENCODE_GO_API_KEY;
use mewcode_protocol::{Message, MessagePart, Mode, ModelId, StreamEvent};
use proptest::prelude::*;
use tokio::sync::mpsc;

// `OPENCODE_GO_API_KEY` / `MEWCODE_ENGINE_BASE_URL` are process-global.
// This is a standalone test binary (its only test is the one below, and proptest
// runs its cases sequentially on a single thread), so there is no in-process
// reader to race with; the sibling `config_resolution.rs` lives in a separate
// binary/process. We still snapshot and restore the two vars we touch.
fn set(key: &str, value: &str) {
    // SAFETY: single-threaded test; no other thread reads the env concurrently.
    unsafe { std::env::set_var(key, value) };
}

fn remove(key: &str) {
    // SAFETY: single-threaded test; no other thread reads the env concurrently.
    unsafe { std::env::remove_var(key) };
}

fn restore(key: &str, prior: Option<String>) {
    match prior {
        Some(v) => set(key, &v),
        None => remove(key),
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    #[test]
    fn error_message_never_leaks_api_key(
        // Non-empty, varied sentinel keys so the substring check is meaningful.
        // The `sk-` prefix plus a body of varied chars keeps the value non-trivial
        // and survives `from_env`'s non-blank filter.
        sentinel in "sk-[A-Za-z0-9_\\-]{8,40}",
    ) {
        let prior_key = std::env::var(OPENCODE_GO_API_KEY).ok();
        let prior_base = std::env::var(ENV_BASE_URL).ok();

        // A key IS present (so failure happens *downstream* of credential
        // resolution), and the base URL points at a closed local endpoint so the
        // request fails immediately, offline.
        set(OPENCODE_GO_API_KEY, &sentinel);
        set(ENV_BASE_URL, "http://127.0.0.1:1");

        let harness = Harness::new(
            ModelId::MiniMaxM3,
            Mode::Build,
            Arc::new(SkillRegistry::new()),
            Arc::new(ToolRegistry::new()),
        );

        // A user message is present, so the turn proceeds past the no-user guard
        // and actually builds the provider and issues the (doomed) request.
        let history = vec![Message::user(vec![MessagePart::Text {
            text: "hello".to_string(),
        }])];

        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("build current-thread runtime");

        let (tx, mut rx) = mpsc::channel::<StreamEvent>(16);
        let result = rt.block_on(harness.run_turn(&history, tx));

        // Restore env before any assertion can short-circuit the case.
        restore(OPENCODE_GO_API_KEY, prior_key);
        restore(ENV_BASE_URL, prior_base);

        // The turn must fail (closed endpoint), and the message the handler would
        // emit is exactly `EngineError::to_string()`.
        let err = result.expect_err("turn must fail against a closed endpoint");
        let message = err.to_string();
        prop_assert!(
            !message.contains(&sentinel),
            "error message leaked the API key: {message:?}"
        );

        // Belt and suspenders: the success-only events never fire on failure, but
        // if any `Error` ever rode the channel its message must also be key-free.
        while let Ok(event) = rx.try_recv() {
            if let StreamEvent::Error { message } = event {
                prop_assert!(
                    !message.contains(&sentinel),
                    "Error event leaked the API key: {message:?}"
                );
            }
        }
    }
}
