//! The `chat-turn` span declares fields at creation and records values at the
//! input/output boundary. If a field was not declared in the [`info_span!`]
//! macro, [`tracing::Span::record`] silently drops it and the recorded
//! value is lost. This test catches that class of bug.
//!
//! Also a regression guard for the Langfuse OTel export path: if a span field
//! is missing, the Langfuse observation will show `input = null` or
//! `output = null`.

use std::sync::{Arc, Mutex};

use mewcode_engine::harness::{chat_turn_span, record_turn_input, record_turn_output};
use mewcode_protocol::{Mode, ModelId};
use tracing::field::{Field, Visit};
use tracing::{Id, Subscriber};
use tracing_subscriber::layer::{Context, SubscriberExt};
use tracing_subscriber::{Layer, Registry};

#[derive(Clone, Default)]
struct Records(Arc<Mutex<Vec<(String, String)>>>);

impl Records {
    fn contains(&self, field: &str, value: &str) -> bool {
        self.0
            .lock()
            .expect("records lock")
            .iter()
            .any(|(f, v)| f == field && v.contains(value))
    }
}

struct CaptureLayer(Records);

impl<S: Subscriber> Layer<S> for CaptureLayer {
    fn on_record(&self, _span: &Id, values: &tracing::span::Record<'_>, _ctx: Context<'_, S>) {
        values.record(&mut CaptureVisitor(&self.0));
    }
}

struct CaptureVisitor<'a>(&'a Records);

impl Visit for CaptureVisitor<'_> {
    fn record_str(&mut self, field: &Field, value: &str) {
        self.0
            .0
            .lock()
            .expect("records lock")
            .push((field.name().to_string(), value.to_string()));
    }

    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        self.0
            .0
            .lock()
            .expect("records lock")
            .push((field.name().to_string(), format!("{value:?}")));
    }
}

#[test]
fn chat_turn_span_records_langfuse_io_fields() {
    let records = Records::default();
    let subscriber = Registry::default().with(CaptureLayer(records.clone()));
    let _guard = tracing::subscriber::set_default(subscriber);

    let span = chat_turn_span(ModelId::default(), Mode::default());

    record_turn_input(&span, "system", "hello");
    record_turn_output(&span, "pong");

    assert!(
        records.contains("langfuse.trace.input", "system\n\nhello"),
        "trace input should include system prompt and user text"
    );
    assert!(
        records.contains("langfuse.observation.input", "hello"),
        "observation input should include the user message"
    );
    assert!(
        records.contains("langfuse.observation.input", "system"),
        "observation input should still include the system prompt"
    );
    // Verify user message comes first (for the list-view preview).
    let observation_input = records
        .0
        .lock()
        .unwrap()
        .iter()
        .find(|(f, _)| f == "langfuse.observation.input")
        .map(|(_, v)| v.clone())
        .expect("observation input should be recorded");
    let hello_pos = observation_input
        .find("hello")
        .expect("hello should appear");
    let system_pos = observation_input
        .find("system")
        .expect("system should appear");
    assert!(
        hello_pos < system_pos,
        "user message should come before system prompt in observation input — got: {observation_input}"
    );
    assert!(records.contains("langfuse.trace.output", "pong"));
    assert!(records.contains("langfuse.observation.output", "pong"));
}
