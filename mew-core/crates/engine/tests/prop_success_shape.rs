// Feature: session-flow-and-engine-v0, successful-turn event shape
//
// Driving `run_turn`'s success-path emitter (`Harness::emit_reply`) with a
// stubbed reply and collecting the channel yields, in order:
//   1. exactly one `Start`, carrying the turn's mode and model, first;
//   2. zero or more `TextDelta` whose payloads concatenate to the reply;
//   3. exactly one `Finish`;
// with zero tool events and nothing after `Finish`.
//
// The emitter is the pure event-shape core of `run_turn` split out from the
// network path, so the sequence is testable without a provider or a request.

use std::sync::Arc;

use mewcode_engine::Harness;
use mewcode_engine::skills::SkillRegistry;
use mewcode_engine::tools::ToolRegistry;
use mewcode_protocol::{Mode, ModelId, StreamEvent};
use proptest::prelude::*;
use tokio::sync::mpsc;

/// Any supported model, by index into the canonical list.
fn any_model() -> impl Strategy<Value = ModelId> {
    (0..ModelId::ALL.len()).prop_map(|i| ModelId::ALL[i])
}

/// Either mode.
fn any_mode() -> impl Strategy<Value = Mode> {
    prop_oneof![Just(Mode::Build), Just(Mode::Plan)]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    #[test]
    fn success_turn_event_shape(
        reply in any::<String>(),
        model in any_model(),
        mode in any_mode(),
    ) {
        let harness = Harness::new(
            model,
            mode,
            Arc::new(SkillRegistry::new()),
            Arc::new(ToolRegistry::new()),
        );

        // `emit_reply` is async and proptest is sync: one runtime per case.
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("build current-thread runtime");

        // Capacity comfortably exceeds the at-most-three events so every send
        // completes and the channel preserves emission order.
        let (tx, mut rx) = mpsc::channel::<StreamEvent>(16);
        let result = rt.block_on(harness.emit_reply(&reply, &tx));
        prop_assert!(result.is_ok(), "success-path emission must not error");

        // Drop our sender so the channel is fully drained by `try_recv`.
        drop(tx);
        let mut events = Vec::new();
        while let Ok(event) = rx.try_recv() {
            events.push(event);
        }

        // Zero tool events anywhere (R9.4).
        prop_assert!(
            !events.iter().any(|e| matches!(
                e,
                StreamEvent::ToolInputAvailable { .. } | StreamEvent::ToolOutputAvailable { .. }
            )),
            "no tool events expected, got {events:?}"
        );
        // No aborts or errors on the success path.
        prop_assert!(
            !events.iter().any(|e| matches!(
                e,
                StreamEvent::Aborted | StreamEvent::Error { .. }
            )),
            "no Aborted/Error expected, got {events:?}"
        );

        // Exactly one `Start`, first, carrying this turn's mode and model (R9.1).
        let (first, rest) = events.split_first().expect("at least Start + Finish");
        match first {
            StreamEvent::Start { mode: m, model: md, .. } => {
                prop_assert_eq!(*m, mode, "Start must carry the turn's mode");
                prop_assert_eq!(*md, model, "Start must carry the turn's model");
            }
            other => prop_assert!(false, "first event must be Start, got {other:?}"),
        }
        prop_assert!(
            !rest.iter().any(|e| matches!(e, StreamEvent::Start { .. })),
            "only one Start expected"
        );

        // Exactly one `Finish`, and it is the last event — nothing after it (R9.3, R9.4).
        let (last, middle) = rest.split_last().expect("at least one Finish");
        prop_assert!(
            matches!(last, StreamEvent::Finish { .. }),
            "last event must be Finish, got {last:?}"
        );
        prop_assert!(
            !middle.iter().any(|e| matches!(e, StreamEvent::Finish { .. })),
            "only one Finish expected"
        );

        // Between Start and Finish: only `TextDelta`, concatenating to the reply (R9.2).
        let mut assembled = String::new();
        for e in middle {
            match e {
                StreamEvent::TextDelta { delta } => assembled.push_str(delta),
                other => prop_assert!(
                    false,
                    "only TextDelta expected between Start and Finish, got {other:?}"
                ),
            }
        }
        prop_assert_eq!(assembled, reply, "TextDelta payloads must concatenate to the reply");
    }
}
