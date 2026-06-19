//! E2E integration test: user asks about README → agent invokes `read_file`
//! → tool returns file content → agent composes a reply referencing it.
//!
//! This test makes **real LLM calls** through OpenCode Go and verifies the
//! full multi-turn tool-calling loop end-to-end:
//!
//! 1. `Start` event emitted
//! 2. `ToolInputAvailable` — agent decides to call `read_file`
//! 3. `ToolOutputAvailable` — tool result fed back to the agent
//! 4. `TextDelta`(s) — agent streams the final answer
//! 5. `Finish` — turn complete
//! 6. The reply text references the actual README content
//! 7. Langfuse trace shows a `chat-turn` generation with a nested `read_file`
//!    tool observation
//!
//! ## Running
//!
//! Requires real credentials in the environment:
//! - `OPENCODE_GO_API_KEY` — for LLM calls
//! - `LANGFUSE_PUBLIC_KEY`, `LANGFUSE_SECRET_KEY`, `LANGFUSE_BASE_URL` — for trace verification
//!
//! ```sh
//! cargo test --test agent_tool_e2e -- --ignored
//! ```

#![cfg(test)]

use std::sync::Arc;
use std::time::Duration;

use mewcode_engine::Harness;
use mewcode_engine::memory::MemoryStore;
use mewcode_engine::skills::SkillRegistry;
use mewcode_engine::tools::{ProjectContext, default_registry};
use mewcode_protocol::{Message, MessagePart, Mode, ModelId, StreamEvent};

use opentelemetry::KeyValue;
use opentelemetry::trace::TracerProvider as _;
use opentelemetry_langfuse::ExporterBuilder;
use opentelemetry_sdk::Resource;
use opentelemetry_sdk::runtime::Tokio;
use opentelemetry_sdk::trace::SdkTracerProvider;
use opentelemetry_sdk::trace::span_processor_with_async_runtime::BatchSpanProcessor;

use tracing_subscriber::{EnvFilter, fmt, prelude::*};

/// Unique marker written into the temp README so we can verify the agent's
/// reply actually read the file (not hallucinated).
const README_MARKER: &str = "MewcodeE2EMarker_42_zebra";

/// Set up the Langfuse OTLP exporter so spans are sent to Langfuse.
/// Returns the provider so the caller can flush + shut it down after the test.
fn init_langfuse_tracing() -> Option<SdkTracerProvider> {
    let public_key = std::env::var("LANGFUSE_PUBLIC_KEY")
        .ok()
        .filter(|s| !s.is_empty())?;
    let secret_key = std::env::var("LANGFUSE_SECRET_KEY")
        .ok()
        .filter(|s| !s.is_empty())?;
    let host = std::env::var("LANGFUSE_BASE_URL")
        .unwrap_or_else(|_| "https://cloud.langfuse.com".to_string());

    let exporter = ExporterBuilder::new()
        .with_host(&host)
        .with_basic_auth(&public_key, &secret_key)
        .with_timeout(Duration::from_secs(10))
        .build()
        .ok()?;

    let provider = SdkTracerProvider::builder()
        .with_resource(
            Resource::builder()
                .with_attributes([KeyValue::new("service.name", "mewcode-e2e-test")])
                .build(),
        )
        .with_span_processor(BatchSpanProcessor::builder(exporter, Tokio).build())
        .build();

    let tracer = provider.tracer("mewcode-e2e-test");
    let otel_layer = tracing_opentelemetry::layer()
        .with_tracer(tracer)
        .with_filter(EnvFilter::new(
            "info,rig::agent_chat=off,rig::completions=off",
        ));

    tracing_subscriber::registry()
        .with(otel_layer)
        .with(fmt::layer())
        .try_init()
        .ok();

    Some(provider)
}

/// Create a temp project directory with a README containing a unique marker.
fn fresh_project() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "mewcode-e2e-agent-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
    ));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("README.md"),
        format!(
            "# Test Project\n\nThis project contains the secret marker: {README_MARKER}\n\
             It is used for e2e testing of the mewcode agent tool-calling loop.\n"
        ),
    )
    .unwrap();
    dir
}

/// Query the Langfuse API for traces belonging to `session_id`.
/// Returns the raw JSON so the test can inspect observations.
async fn langfuse_traces(session_id: &str) -> serde_json::Value {
    let public_key = std::env::var("LANGFUSE_PUBLIC_KEY").unwrap();
    let secret_key = std::env::var("LANGFUSE_SECRET_KEY").unwrap();
    let host = std::env::var("LANGFUSE_BASE_URL")
        .unwrap_or_else(|_| "https://cloud.langfuse.com".to_string());

    let url = format!("{host}/api/public/traces?sessionId={session_id}&limit=5");
    let client = reqwest::Client::new();
    let resp = client
        .get(&url)
        .basic_auth(&public_key, Some(&secret_key))
        .send()
        .await
        .expect("Langfuse API request should succeed");

    resp.json().await.expect("Langfuse response should be JSON")
}

/// Query observations for a specific trace ID.
async fn langfuse_observations(trace_id: &str) -> serde_json::Value {
    let public_key = std::env::var("LANGFUSE_PUBLIC_KEY").unwrap();
    let secret_key = std::env::var("LANGFUSE_SECRET_KEY").unwrap();
    let host = std::env::var("LANGFUSE_BASE_URL")
        .unwrap_or_else(|_| "https://cloud.langfuse.com".to_string());

    let url = format!("{host}/api/public/observations?traceId={trace_id}&limit=20");
    let client = reqwest::Client::new();
    let resp = client
        .get(&url)
        .basic_auth(&public_key, Some(&secret_key))
        .send()
        .await
        .expect("Langfuse observations request should succeed");

    resp.json()
        .await
        .expect("Langfuse observations should be JSON")
}

/// Collect all StreamEvents from a harness turn into a Vec.
async fn collect_events(
    harness: &Harness,
    messages: Vec<Message>,
) -> (Vec<StreamEvent>, Option<String>) {
    let (tx, mut rx) = tokio::sync::mpsc::channel::<StreamEvent>(128);
    let h = harness.clone();
    let handle = tokio::spawn(async move { h.run_turn(&messages, tx).await });

    let mut events = Vec::new();
    while let Some(event) = rx.recv().await {
        events.push(event);
    }

    let result = handle.await;
    let error = match result {
        Ok(Ok(())) => None,
        Ok(Err(e)) => Some(e.to_string()),
        Err(e) => Some(format!("task panicked: {e}")),
    };

    (events, error)
}

#[tokio::test]
#[ignore = "requires real OPENCODE_GO_API_KEY and Langfuse credentials"]
async fn agent_reads_readme_via_tool_call() {
    // Load .env if present (for local dev with real credentials).
    let _ = dotenvy::dotenv();

    // --- Prerequisites ---
    let api_key =
        std::env::var("OPENCODE_GO_API_KEY").expect("OPENCODE_GO_API_KEY must be set for e2e test");
    assert!(!api_key.is_empty(), "OPENCODE_GO_API_KEY must not be empty");

    // --- Set up tracing (Langfuse) ---
    let _tracing_provider = init_langfuse_tracing();

    // --- Create a temp project with a known README ---
    let project = fresh_project();
    let data_dir = std::env::temp_dir().join(format!(
        "mewcode-e2e-data-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
    ));
    std::fs::create_dir_all(&data_dir).unwrap();

    // --- Build the harness with real tools ---
    let skills = Arc::new(SkillRegistry::load_defaults());
    let ctx = ProjectContext::new(project.clone());
    let store = MemoryStore::new(data_dir);
    let tools = Arc::new(default_registry(ctx, skills.clone(), Some(store)));
    let session_id = uuid::Uuid::new_v4();

    let harness = Harness::new(ModelId::Glm52, Mode::Build, skills, tools)
        .with_session(session_id)
        .with_memory(MemoryStore::new(
            std::env::temp_dir().join(format!("mewcode-e2e-mem-{session_id}")),
        ));

    // --- Send the user message ---
    let messages = vec![Message::user(vec![MessagePart::Text {
        text: "Read the file README.md in the current directory and tell me \
             what the secret marker is. Use the read_file tool."
            .to_string(),
    }])];

    let (events, error) = collect_events(&harness, messages).await;

    // --- Assert no error ---
    assert!(error.is_none(), "harness should not error: {error:?}");
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, StreamEvent::Error { .. })),
        "no Error events should be emitted"
    );

    // --- Assert event sequence ---
    let has_start = events
        .iter()
        .any(|e| matches!(e, StreamEvent::Start { .. }));
    let has_tool_input = events
        .iter()
        .any(|e| matches!(e, StreamEvent::ToolInputAvailable { tool_name, .. } if tool_name == "read_file"));
    let has_tool_output = events
        .iter()
        .any(|e| matches!(e, StreamEvent::ToolOutputAvailable { .. }));
    let has_text_delta = events
        .iter()
        .any(|e| matches!(e, StreamEvent::TextDelta { .. }));
    let has_finish = events
        .iter()
        .any(|e| matches!(e, StreamEvent::Finish { .. }));

    assert!(has_start, "should emit Start event");
    assert!(
        has_tool_input,
        "should emit ToolInputAvailable for read_file — \
         events: {events:?}"
    );
    assert!(
        has_tool_output,
        "should emit ToolOutputAvailable — events: {events:?}"
    );
    assert!(has_text_delta, "should emit at least one TextDelta");
    assert!(has_finish, "should emit Finish event");

    // --- Assert the reply references the README content ---
    let reply: String = events
        .iter()
        .filter_map(|e| match e {
            StreamEvent::TextDelta { delta } => Some(delta.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("");

    assert!(
        !reply.is_empty(),
        "reply should not be empty — events: {events:?}"
    );
    assert!(
        reply.contains(README_MARKER),
        "reply should contain the README marker '{README_MARKER}' — \
         reply: {reply}"
    );

    // --- Verify Langfuse trace ---
    // Shut down the tracer provider first to force-flush all spans to Langfuse.
    // Then wait for Langfuse to index them (batch processor + API indexing latency).
    if let Some(provider) = &_tracing_provider {
        let _ = provider.shutdown();
    }
    eprintln!("Tracer provider shut down, waiting for Langfuse to index...");

    // Retry loop: Langfuse batch export + indexing can take several seconds.
    let traces = {
        let mut last_result = None;
        for attempt in 1..=4 {
            tokio::time::sleep(Duration::from_secs(2)).await;
            eprintln!("Querying Langfuse traces (attempt {attempt}/4)...");
            let result = langfuse_traces(&session_id.to_string()).await;
            let count = result
                .get("data")
                .and_then(|d| d.as_array())
                .map(|a| a.len())
                .unwrap_or(0);
            eprintln!("  → {count} traces found");
            if count > 0 {
                last_result = Some(result);
                break;
            }
            last_result = Some(result);
        }
        last_result.expect("should have at least one Langfuse API response")
    };
    let trace_data = traces
        .get("data")
        .and_then(|d| d.as_array())
        .expect("Langfuse should return trace data array");

    assert!(
        !trace_data.is_empty(),
        "Langfuse should have at least one trace for session {session_id}"
    );

    // Find the trace and verify it has a chat-turn observation
    let trace = &trace_data[0];
    let trace_id = trace
        .get("id")
        .and_then(|v| v.as_str())
        .expect("trace should have an id");

    let observations = langfuse_observations(trace_id).await;
    let obs_data = observations
        .get("data")
        .and_then(|d| d.as_array())
        .expect("Langfuse should return observations array");

    let has_chat_turn = obs_data.iter().any(|o| {
        o.get("name").and_then(|v| v.as_str()) == Some("chat-turn")
            && o.get("type").and_then(|v| v.as_str()) == Some("GENERATION")
    });
    let has_read_file_tool = obs_data.iter().any(|o| {
        o.get("name").and_then(|v| v.as_str()) == Some("read_file")
            && o.get("type").and_then(|v| v.as_str()) == Some("TOOL")
    });

    assert!(
        has_chat_turn,
        "Langfuse trace should have a 'chat-turn' GENERATION observation"
    );
    assert!(
        has_read_file_tool,
        "Langfuse trace should have a 'read_file' TOOL observation — \
         observations: {obs_data:?}"
    );

    // Print a summary for manual verification
    eprintln!("\n=== E2E Test Summary ===");
    eprintln!("Events: {} total", events.len());
    eprintln!(
        "  Start: {}, ToolInput: {}, ToolOutput: {}, TextDelta: {}, Finish: {}",
        has_start, has_tool_input, has_tool_output, has_text_delta, has_finish
    );
    eprintln!("Reply length: {} chars", reply.len());
    eprintln!("Langfuse trace ID: {trace_id}");
    eprintln!("Observations: {}", obs_data.len());
    eprintln!("=========================\n");

    // --- Clean up ---
    let _ = std::fs::remove_dir_all(&project);
}
