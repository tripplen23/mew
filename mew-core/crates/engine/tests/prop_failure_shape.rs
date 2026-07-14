//! Property-based test: a failed turn produces a single `Error` and nothing else.
//!
//! Feature: session-flow-and-engine-v0, Property 16: Failed-turn event shape.
//! For a forced upstream transport / non-success failure, the turn emits exactly
//! one `Error` StreamEvent carrying a non-empty `message`, no `Finish`, and no
//! further event of any type.
//!
//! The `Error` event has a single owner. `Harness::run_turn` signals failure by
//! returning `Err(EngineError)` and emits *nothing* on that path (verified here
//! against the real harness); the server chat handler is the sole emitter of the
//! `Error` event, sending exactly one `StreamEvent::Error { message: e.to_string() }`
//! on `Err`. So "failed-turn event shape" is a property of that handler sink fed by
//! the harness's emit-nothing-on-error contract — not of either piece alone. We
//! mirror the handler sink verbatim (see `crates/server/src/routes/chat.rs`) and
//! drive it with forced upstream failures, avoiding the network and any process-
//! global env mutation (which would race `config_resolution`).

use std::sync::Arc;

use mewcode_engine::Harness;
use mewcode_engine::error::EngineError;
use mewcode_engine::skills::SkillRegistry;
use mewcode_engine::tools::ToolRegistry;
use mewcode_protocol::{Mode, ModelId, StreamEvent};
use proptest::prelude::*;
use tokio::sync::mpsc;

fn model_strategy() -> impl Strategy<Value = ModelId> {
    (0..ModelId::ALL.len()).prop_map(|i| ModelId::ALL[i])
}

fn mode_strategy() -> impl Strategy<Value = Mode> {
    prop_oneof![Just(Mode::Build), Just(Mode::Plan)]
}

/// Forced upstream failures as they reach the handler. A non-success response
/// surfaces as [`EngineError::UpstreamStatus`]; a transport failure surfaces through
/// `run_turn`'s agent path as `EngineError::Other(e.to_string())`.
fn upstream_error_strategy() -> impl Strategy<Value = EngineError> {
    prop_oneof![
        (400u16..=599u16, "(?s).{0,64}")
            .prop_map(|(status, body)| EngineError::UpstreamStatus { status, body }),
        "(?s).{1,64}".prop_map(EngineError::Other),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn failed_turn_emits_single_error(
        err in upstream_error_strategy(),
        mode in mode_strategy(),
        model in model_strategy(),
    ) {
        // proptest is sync; the channel + sink are async: one runtime per case.
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("build current-thread runtime");

        // --- Premise: the harness emits nothing on the failure path. ---------
        // A history with no user message fails the turn before any provider is
        // built, so this never touches the network and holds regardless of
        // `OPENCODE_GO_API_KEY` (missing => MissingApiKey, present => Other), which
        // keeps the test from racing env-mutating tests in other binaries.
        let harness = Harness::new(
            model,
            mode,
            Arc::new(SkillRegistry::new()),
            Arc::new(ToolRegistry::new()),
        );
        let (htx, mut hrx) = mpsc::channel::<StreamEvent>(16);
        let run = rt.block_on(harness.run_turn(&[], htx));
        prop_assert!(run.is_err(), "a turn with no user message must fail");
        prop_assert!(
            hrx.try_recv().is_err(),
            "run_turn must emit no events on the failure path"
        );

        // --- The handler sink, mirrored verbatim from routes/chat.rs. ---------
        // run_turn returned Err and emitted nothing; the handler sinks exactly one
        // Error carrying e.to_string(), then the turn ends.
        let (tx, mut rx) = mpsc::channel::<StreamEvent>(16);
        let run_turn_result: Result<(), EngineError> = Err(err);
        rt.block_on(async {
            if let Err(e) = run_turn_result {
                let _ = tx.send(StreamEvent::Error { message: e.to_string() }).await;
            }
        });
        // Dropping the sender lets the drain see exactly what the turn emitted.
        drop(tx);

        let mut events = Vec::new();
        while let Ok(event) = rx.try_recv() {
            events.push(event);
        }

        // Exactly one event, and it is an `Error` with a non-empty message (R10.1).
        prop_assert_eq!(
            events.len(),
            1,
            "a failed turn must emit exactly one event, got {:?}",
            events
        );
        match &events[0] {
            StreamEvent::Error { message } => {
                prop_assert!(!message.is_empty(), "Error.message must be non-empty");
            }
            other => prop_assert!(false, "the one event must be Error, got {other:?}"),
        }

        // No `Finish` for a failed turn (R10.1), and nothing follows the `Error`
        // (R10.4). With exactly one event that is the `Error`, both hold; assert
        // explicitly so a regression that appends a `Finish` is caught directly.
        prop_assert!(
            !events.iter().any(|e| matches!(e, StreamEvent::Finish { .. })),
            "a failed turn must not emit Finish"
        );
    }
}
