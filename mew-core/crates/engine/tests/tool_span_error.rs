//! Failed tool calls must mark the OTel span ERROR so LangFuse shows them at
//! ERROR level instead of DEFAULT. Verified with an in-memory exporter.

use std::sync::Arc;

use async_trait::async_trait;
use mewcode_engine::tools::adapter::RigToolAdapter;
use mewcode_protocol::{ToolContracts, ToolDescriptor, ToolError, ToolOutput};
use opentelemetry::trace::Status;
use opentelemetry::trace::TracerProvider as _;
use opentelemetry_sdk::trace::SimpleSpanProcessor;
use opentelemetry_sdk::trace::{InMemorySpanExporter, SdkTracerProvider};
use rig_core::tool::ToolDyn;
use serde_json::json;
use tracing_subscriber::prelude::*;

struct FailingTool;

#[async_trait]
impl ToolContracts for FailingTool {
    fn name(&self) -> &'static str {
        "failing_tool"
    }

    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            name: "failing_tool".to_string(),
            description: "always fails".to_string(),
            input_schema: json!({"type": "object", "properties": {}}),
            annotations: Default::default(),
            examples: vec![],
            max_response_chars: 100_000,
        }
    }

    async fn execute(&self, _input: serde_json::Value) -> Result<ToolOutput, ToolError> {
        Err(ToolError::Other {
            message: "boom".to_string(),
            hint: None,
        })
    }
}

fn failing_adapter() -> RigToolAdapter {
    RigToolAdapter::new(Arc::new(FailingTool))
}

#[tokio::test(flavor = "current_thread")]
async fn failed_tool_call_marks_span_error() {
    let exporter = InMemorySpanExporter::default();
    let provider = SdkTracerProvider::builder()
        .with_span_processor(SimpleSpanProcessor::new(exporter.clone()))
        .build();
    let tracer = provider.tracer("test");
    let subscriber = tracing_subscriber::registry()
        .with(tracing_opentelemetry::layer().with_tracer(tracer))
        .with(
            tracing_subscriber::fmt::layer()
                .with_filter(tracing_subscriber::filter::LevelFilter::OFF),
        );

    let _guard = tracing::subscriber::set_default(subscriber);
    let _span = tracing::info_span!("execute_tool").entered();

    let adapter = failing_adapter();
    let result = adapter.call("{}".to_string()).await;
    assert!(result.is_ok(), "errors are serialized as Ok for the model");
    drop(_span);

    let spans = exporter.get_finished_spans().unwrap();
    let span = spans
        .iter()
        .find(|s| s.name == "execute_tool")
        .expect("execute_tool span should be exported");
    assert!(
        matches!(span.status, Status::Error { .. }),
        "failed tool call should set span status to Error — got {:?}",
        span.status
    );
}

struct SucceedingTool;

#[async_trait]
impl ToolContracts for SucceedingTool {
    fn name(&self) -> &'static str {
        "succeeding_tool"
    }

    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            name: "succeeding_tool".to_string(),
            description: "always succeeds".to_string(),
            input_schema: json!({"type": "object", "properties": {}}),
            annotations: Default::default(),
            examples: vec![],
            max_response_chars: 100_000,
        }
    }

    async fn execute(&self, _input: serde_json::Value) -> Result<ToolOutput, ToolError> {
        Ok(ToolOutput::text("ok"))
    }
}

fn succeeding_adapter() -> RigToolAdapter {
    RigToolAdapter::new(Arc::new(SucceedingTool))
}

#[tokio::test(flavor = "current_thread")]
async fn successful_tool_call_keeps_unset_status() {
    let exporter = InMemorySpanExporter::default();
    let provider = SdkTracerProvider::builder()
        .with_span_processor(SimpleSpanProcessor::new(exporter.clone()))
        .build();
    let tracer = provider.tracer("test");
    let subscriber = tracing_subscriber::registry()
        .with(tracing_opentelemetry::layer().with_tracer(tracer))
        .with(
            tracing_subscriber::fmt::layer()
                .with_filter(tracing_subscriber::filter::LevelFilter::OFF),
        );

    let _guard = tracing::subscriber::set_default(subscriber);

    let span = tracing::info_span!("execute_tool").entered();
    let adapter = succeeding_adapter();
    let result = adapter.call("{}".to_string()).await;
    assert!(result.is_ok(), "successful tool call should return Ok");
    drop(span);

    let spans = exporter.get_finished_spans().unwrap();
    let span = spans
        .iter()
        .find(|s| s.name == "execute_tool")
        .expect("execute_tool span should be exported");
    assert_eq!(span.status, Status::Unset);
}
