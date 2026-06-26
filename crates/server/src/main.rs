use std::time::Duration;

use anyhow::Context;
use mewcode_engine::memory::MemoryStore;
use mewcode_server::store::fs::{FsStore, resolve_data_dir};
use mewcode_server::{AppState, config::ServerConfig};
use opentelemetry::KeyValue;
use opentelemetry::trace::TracerProvider as _;
use opentelemetry_langfuse::ExporterBuilder;
use opentelemetry_sdk::Resource;
use opentelemetry_sdk::runtime::Tokio;
use opentelemetry_sdk::trace::BatchConfigBuilder;
use opentelemetry_sdk::trace::SdkTracerProvider;
use opentelemetry_sdk::trace::span_processor_with_async_runtime::BatchSpanProcessor;
use tokio::net::TcpListener;
use tracing_subscriber::{EnvFilter, fmt, prelude::*};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _ = dotenvy::dotenv();

    let config = ServerConfig::load()?;
    let telemetry = init_tracing(&config.log);

    let addr: std::net::SocketAddr = format!("{}:{}", config.host, config.port)
        .parse()
        .expect("MEWCODE_HOST/MEWCODE_PORT must form a valid SocketAddr");

    // A create/write failure here aborts startup; we deliberately do not fall
    // back to an in-memory store.
    let data_dir = resolve_data_dir().context("failed to resolve mewcode data directory")?;
    let store = std::sync::Arc::new(
        FsStore::new(data_dir.clone())
            .with_context(|| format!("failed to open session store at {}", data_dir.display()))?,
    );
    tracing::info!(data_dir = %data_dir.display(), "session store ready");

    let memory = MemoryStore::new(data_dir);
    let state = AppState::new(config.clone(), store, memory);

    let listener = TcpListener::bind(addr).await?;
    tracing::info!(%addr, "mewcode server listening");
    let app = mewcode_server::build_app(state);
    let result = axum::serve(listener, app).await;

    if let Some(provider) = telemetry {
        if let Err(e) = provider.shutdown() {
            tracing::warn!(error = %e, "failed to flush OpenTelemetry traces");
        }
    }

    result?;
    Ok(())
}

fn init_tracing(log_filter: &str) -> Option<SdkTracerProvider> {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(log_filter));
    let provider = build_langfuse_provider();
    let otel_layer = provider.as_ref().map(|provider| {
        let tracer = provider.tracer("mewcode-server");
        // Suppress Rig's per-turn `chat` spans and provider `completions`
        // spans — they create noisy duplicate observations in Langfuse.
        // Rig's `invoke_agent` (generation) and `execute_tool` spans pass
        // through and carry the standard `gen_ai.*` semantic-convention
        // fields. Our `chat-turn` span adds the `langfuse.*` fields that
        // Rig does not emit.
        tracing_opentelemetry::layer()
            .with_tracer(tracer)
            .with_filter(EnvFilter::new(
                "info,rig::agent_chat=off,rig::completions=off",
            ))
    });

    tracing_subscriber::registry()
        .with(filter)
        .with(otel_layer)
        .with(fmt::layer().with_target(true))
        .init();

    if provider.is_some() {
        tracing::info!("Langfuse OpenTelemetry tracing enabled");
    } else {
        tracing::info!(
            "Langfuse tracing disabled (set LANGFUSE_PUBLIC_KEY and LANGFUSE_SECRET_KEY to enable)"
        );
    }

    provider
}

fn build_langfuse_provider() -> Option<SdkTracerProvider> {
    let public_key = non_blank(std::env::var("LANGFUSE_PUBLIC_KEY").ok())?;
    let secret_key = non_blank(std::env::var("LANGFUSE_SECRET_KEY").ok())?;
    let host = non_blank(std::env::var("LANGFUSE_BASE_URL").ok())
        .unwrap_or_else(|| "https://cloud.langfuse.com".to_string());

    let exporter = match ExporterBuilder::new()
        .with_host(&host)
        .with_basic_auth(&public_key, &secret_key)
        .with_timeout(Duration::from_secs(10))
        .with_header("x-langfuse-ingestion-version", "4")
        .build()
    {
        Ok(exporter) => exporter,
        Err(e) => {
            eprintln!("failed to configure Langfuse exporter: {e}");
            return None;
        }
    };

    Some(
        SdkTracerProvider::builder()
            .with_resource(
                Resource::builder()
                    .with_attributes([KeyValue::new("service.name", "mewcode-server")])
                    .build(),
            )
            .with_span_processor(
                BatchSpanProcessor::builder(exporter, Tokio)
                    // Tuned for low-latency Langfuse ingestion: the v4 header
                    // (above) routes to Fast Preview, and these knobs shrink
                    // the worst-case flush window from ~35s (defaults) to ~2s.
                    // Values match PHASES.md §"Phase 17 — Trace ingestion latency".
                    .with_batch_config(
                        BatchConfigBuilder::default()
                            .with_scheduled_delay(Duration::from_secs(2))
                            .with_max_export_timeout(Duration::from_secs(10))
                            .with_max_export_batch_size(256)
                            .with_max_queue_size(4096)
                            .build(),
                    )
                    .build(),
            )
            .build(),
    )
}

fn non_blank(value: Option<String>) -> Option<String> {
    value
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}
