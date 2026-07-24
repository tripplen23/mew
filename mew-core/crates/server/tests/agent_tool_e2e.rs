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
    let mut h = harness.clone();
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
    let store = MemoryStore::new(data_dir.clone());
    let tools = Arc::new(default_registry(
        ctx,
        skills.clone(),
        Some(store),
        Mode::Build,
    ));
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
    let _ = std::fs::remove_dir_all(&data_dir);
    let _ =
        std::fs::remove_dir_all(std::env::temp_dir().join(format!("mewcode-e2e-mem-{session_id}")));
}

/// E2E: agent uses `write_file` to create a file, then `read_file` to verify.
#[tokio::test]
#[ignore = "requires real OPENCODE_GO_API_KEY and Langfuse credentials"]
async fn agent_writes_file_via_tool_call() {
    let _ = dotenvy::dotenv();
    std::env::var("OPENCODE_GO_API_KEY").expect("OPENCODE_GO_API_KEY must be set");

    let _tracing_provider = init_langfuse_tracing();

    let project = fresh_project();
    let data_dir = std::env::temp_dir().join(format!(
        "mewcode-e2e-write-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
    ));
    std::fs::create_dir_all(&data_dir).unwrap();

    let skills = Arc::new(SkillRegistry::load_defaults());
    let ctx = ProjectContext::new(project.clone());
    let store = MemoryStore::new(data_dir.clone());
    let tools = Arc::new(default_registry(
        ctx,
        skills.clone(),
        Some(store),
        Mode::Build,
    ));
    let session_id = uuid::Uuid::new_v4();

    let harness = Harness::new(ModelId::Glm52, Mode::Build, skills, tools)
        .with_session(session_id)
        .with_memory(MemoryStore::new(
            std::env::temp_dir().join(format!("mewcode-e2e-mem-{session_id}")),
        ));

    let messages = vec![Message::user(vec![MessagePart::Text {
        text: "Create a file called `greeting.txt` with the content `Hello from mewcode!` using the write_file tool. Then read it back with read_file to confirm."
            .to_string(),
    }])];

    let (events, error) = collect_events(&harness, messages).await;
    assert!(error.is_none(), "harness should not error: {error:?}");

    let has_write = events.iter().any(|e| {
        matches!(
            e,
            StreamEvent::ToolInputAvailable { tool_name, .. } if tool_name == "write_file"
        )
    });
    assert!(has_write, "should emit ToolInputAvailable for write_file");

    let reply: String = events
        .iter()
        .filter_map(|e| match e {
            StreamEvent::TextDelta { delta } => Some(delta.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("");

    assert!(!reply.is_empty(), "reply should not be empty");

    // Verify the file was actually created.
    let written = std::fs::read_to_string(project.join("greeting.txt"));
    assert!(
        written.is_ok(),
        "greeting.txt should exist after write_file tool call"
    );
    assert_eq!(
        written.unwrap(),
        "Hello from mewcode!",
        "file content should match"
    );

    if let Some(provider) = &_tracing_provider {
        let _ = provider.shutdown();
    }
    let _ = std::fs::remove_dir_all(&project);
    let _ = std::fs::remove_dir_all(&data_dir);
    let _ =
        std::fs::remove_dir_all(std::env::temp_dir().join(format!("mewcode-e2e-mem-{session_id}")));
}

/// E2E: agent uses `bash` to run a command.
#[tokio::test]
#[ignore = "requires real OPENCODE_GO_API_KEY and Langfuse credentials"]
async fn agent_runs_bash_via_tool_call() {
    let _ = dotenvy::dotenv();
    std::env::var("OPENCODE_GO_API_KEY").expect("OPENCODE_GO_API_KEY must be set");

    let _tracing_provider = init_langfuse_tracing();

    let project = fresh_project();
    let data_dir = std::env::temp_dir().join(format!(
        "mewcode-e2e-bash-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
    ));
    std::fs::create_dir_all(&data_dir).unwrap();

    let skills = Arc::new(SkillRegistry::load_defaults());
    let ctx = ProjectContext::new(project.clone());
    let store = MemoryStore::new(data_dir.clone());
    let tools = Arc::new(default_registry(
        ctx,
        skills.clone(),
        Some(store),
        Mode::Build,
    ));
    let session_id = uuid::Uuid::new_v4();

    let harness = Harness::new(ModelId::Glm52, Mode::Build, skills, tools)
        .with_session(session_id)
        .with_memory(MemoryStore::new(
            std::env::temp_dir().join(format!("mewcode-e2e-mem-{session_id}")),
        ));

    let messages = vec![Message::user(vec![MessagePart::Text {
        text: "Run `echo mewcode_bash_marker_42` using the bash tool and tell me the output."
            .to_string(),
    }])];

    let (events, error) = collect_events(&harness, messages).await;
    assert!(error.is_none(), "harness should not error: {error:?}");

    let has_bash = events.iter().any(|e| {
        matches!(
            e,
            StreamEvent::ToolInputAvailable { tool_name, .. } if tool_name == "bash"
        )
    });
    assert!(has_bash, "should emit ToolInputAvailable for bash");

    let reply: String = events
        .iter()
        .filter_map(|e| match e {
            StreamEvent::TextDelta { delta } => Some(delta.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("");

    assert!(
        reply.contains("mewcode_bash_marker_42"),
        "reply should contain the bash output marker — reply: {reply}"
    );

    if let Some(provider) = &_tracing_provider {
        let _ = provider.shutdown();
    }
    let _ = std::fs::remove_dir_all(&project);
    let _ = std::fs::remove_dir_all(&data_dir);
    let _ =
        std::fs::remove_dir_all(std::env::temp_dir().join(format!("mewcode-e2e-mem-{session_id}")));
}

/// E2E: PLAN mode rejects write_file — the tool is not registered, so the
/// model gets a `ToolNotFound` error and no file is created.
#[tokio::test]
#[ignore = "requires real OPENCODE_GO_API_KEY and Langfuse credentials"]
async fn plan_mode_rejects_write_file() {
    let _ = dotenvy::dotenv();
    std::env::var("OPENCODE_GO_API_KEY").expect("OPENCODE_GO_API_KEY must be set");

    let _tracing_provider = init_langfuse_tracing();

    let project = fresh_project();
    let data_dir = std::env::temp_dir().join(format!(
        "mewcode-e2e-plan-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
    ));
    std::fs::create_dir_all(&data_dir).unwrap();

    let skills = Arc::new(SkillRegistry::load_defaults());
    let ctx = ProjectContext::new(project.clone());
    let store = MemoryStore::new(data_dir.clone());
    let tools = Arc::new(default_registry(
        ctx,
        skills.clone(),
        Some(store),
        Mode::Plan,
    ));
    let session_id = uuid::Uuid::new_v4();

    let harness = Harness::new(ModelId::Glm52, Mode::Plan, skills, tools)
        .with_session(session_id)
        .with_memory(MemoryStore::new(
            std::env::temp_dir().join(format!("mewcode-e2e-mem-{session_id}")),
        ));

    let messages = vec![Message::user(vec![MessagePart::Text {
        text: "Create a file called `should_not_exist.txt` with any content using the write_file tool."
            .to_string(),
    }])];

    let (_events, error) = collect_events(&harness, messages).await;
    assert!(error.is_none(), "harness should not error: {error:?}");

    // In PLAN mode, write_file is not registered. The model may try to call
    // it and get a ToolNotFound error, or it may recognize from the system
    // prompt that it can't write files and explain that to the user.
    // Either way, no file should be created.
    let file_exists = project.join("should_not_exist.txt").exists();
    assert!(
        !file_exists,
        "no file should be created in PLAN mode — the tool is not registered"
    );

    if let Some(provider) = &_tracing_provider {
        let _ = provider.shutdown();
    }
    let _ = std::fs::remove_dir_all(&project);
    let _ = std::fs::remove_dir_all(&data_dir);
    let _ =
        std::fs::remove_dir_all(std::env::temp_dir().join(format!("mewcode-e2e-mem-{session_id}")));
}

/// E2E: caching — a multi-turn chat where the second turn should benefit
/// from prompt caching. The test verifies that the `chat-turn` span
/// records `gen_ai.usage.cache_read.input_tokens > 0` on at least one
/// CompletionCall.
#[tokio::test]
#[ignore = "requires real OPENCODE_GO_API_KEY and Langfuse credentials"]
async fn prompt_caching_records_cache_read_tokens() {
    let _ = dotenvy::dotenv();
    std::env::var("OPENCODE_GO_API_KEY").expect("OPENCODE_GO_API_KEY must be set");

    let _tracing_provider = init_langfuse_tracing();

    let project = fresh_project();
    let data_dir = std::env::temp_dir().join(format!(
        "mewcode-e2e-cache-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
    ));
    std::fs::create_dir_all(&data_dir).unwrap();

    let skills = Arc::new(SkillRegistry::load_defaults());
    let ctx = ProjectContext::new(project.clone());
    let store = MemoryStore::new(data_dir.clone());
    let tools = Arc::new(default_registry(
        ctx,
        skills.clone(),
        Some(store),
        Mode::Build,
    ));
    let session_id = uuid::Uuid::new_v4();

    let harness = Harness::new(ModelId::Glm52, Mode::Build, skills, tools)
        .with_session(session_id)
        .with_memory(MemoryStore::new(
            std::env::temp_dir().join(format!("mewcode-e2e-mem-{session_id}")),
        ));

    // First turn: ask the agent to read a file.
    let messages_turn1 = vec![Message::user(vec![MessagePart::Text {
        text: "Read the file README.md using the read_file tool and tell me what the secret marker is."
            .to_string(),
    }])];

    let (events1, error1) = collect_events(&harness, messages_turn1).await;
    assert!(error1.is_none(), "turn 1 should not error: {error1:?}");

    let reply1: String = events1
        .iter()
        .filter_map(|e| match e {
            StreamEvent::TextDelta { delta } => Some(delta.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("");

    assert!(
        reply1.contains(README_MARKER),
        "turn 1 reply should contain the README marker"
    );

    // Second turn: follow-up that references the first read. The system
    // prompt + tool descriptors are identical, so the cache should hit.
    let messages_turn2 = vec![
        Message::user(vec![MessagePart::Text {
            text: "Read the file README.md using the read_file tool and tell me what the secret marker is."
                .to_string(),
        }]),
        Message::assistant(vec![MessagePart::Text { text: reply1.clone() }], "glm-5.2"),
        Message::user(vec![MessagePart::Text {
            text: "What was the secret marker you just read? Just repeat it."
                .to_string(),
        }]),
    ];

    let (events2, error2) = collect_events(&harness, messages_turn2).await;
    assert!(error2.is_none(), "turn 2 should not error: {error2:?}");

    let reply2: String = events2
        .iter()
        .filter_map(|e| match e {
            StreamEvent::TextDelta { delta } => Some(delta.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("");

    assert!(
        reply2.contains(README_MARKER),
        "turn 2 reply should still reference the marker"
    );

    // Flush spans and verify Langfuse trace has cache_read tokens.
    if let Some(provider) = &_tracing_provider {
        let _ = provider.shutdown();
    }

    // Query Langfuse for the trace and check usage metadata.
    tokio::time::sleep(Duration::from_secs(3)).await;
    let traces = langfuse_traces(&session_id.to_string()).await;
    let trace_data = traces
        .get("data")
        .and_then(|d| d.as_array())
        .expect("Langfuse should return trace data");

    assert!(
        !trace_data.is_empty(),
        "Langfuse should have traces for session {session_id}"
    );

    // Check at least one observation has cache-read token usage.
    // Langfuse's observations API returns the cache breakdown as
    // `usage.cacheReadInputTokens` (and `cacheCreationInputTokens`).
    // Generic `usage.input > 0` is not a cache signal — it fires for
    // every regular completion, so checking it would pass even if
    // caching is broken.
    let mut found_cache = false;
    for trace in trace_data {
        if let Some(trace_id) = trace.get("id").and_then(|v| v.as_str()) {
            let observations = langfuse_observations(trace_id).await;
            if let Some(obs) = observations.get("data").and_then(|d| d.as_array()) {
                for o in obs {
                    let cache_read = o
                        .get("usage")
                        .and_then(|u| u.get("cacheReadInputTokens"))
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0);
                    if cache_read > 0 {
                        found_cache = true;
                    }
                }
            }
        }
    }

    eprintln!("\n=== Caching E2E Summary ===");
    eprintln!("Turn 1 reply: {} chars", reply1.len());
    eprintln!("Turn 2 reply: {} chars", reply2.len());
    eprintln!("Found cacheReadInputTokens > 0: {found_cache}");
    eprintln!("==========================\n");

    assert!(
        found_cache,
        "expected at least one observation with usage.cacheReadInputTokens > 0; \
         prompt caching is not working (or the cache_read field is being \
         filtered out before it reaches Langfuse)"
    );

    let _ = std::fs::remove_dir_all(&project);
    let _ = std::fs::remove_dir_all(&data_dir);
    let _ =
        std::fs::remove_dir_all(std::env::temp_dir().join(format!("mewcode-e2e-mem-{session_id}")));
}
