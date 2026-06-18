//! [Langfuse](https://langfuse.com) tracing over its native ingestion HTTP API.
//!
//! Each completed chat turn is reported as one Langfuse **trace** (grouped by
//! the chat `sessionId`) containing a single **generation** observation: the
//! user's input, the assistant's output (or the error), the model, the mode,
//! and the wall-clock latency. We POST directly to Langfuse's
//! `/api/public/ingestion` endpoint rather than pulling in the OpenTelemetry
//! crate stack — `reqwest` (already a dependency) speaks HTTP Basic auth
//! natively, so this adds **zero new dependencies**.
//!
//! Tracing is opt-in: [`LangfuseTracer::from_env`] returns `None` (disabled)
//! unless both `LANGFUSE_PUBLIC_KEY` and `LANGFUSE_SECRET_KEY` are set, so the
//! engine behaves exactly as before when Langfuse is not configured.
//!
//! > Idiom: observability is a side channel. Reporting runs fire-and-forget on
//! > its own task and swallows its own errors, so a slow or down Langfuse can
//! > never add latency to — or fail — a user's turn.

use std::{env, time::Duration};

use chrono::{DateTime, Utc};
use serde_json::{Value, json};
use uuid::Uuid;

/// Default Langfuse Cloud host.
const DEFAULT_HOST: &str = "https://cloud.langfuse.com";

/// Env var names. Public so the binary can document/validate them in one place.
pub const ENV_PUBLIC_KEY: &str = "LANGFUSE_PUBLIC_KEY";
/// Secret-key env var.
pub const ENV_SECRET_KEY: &str = "LANGFUSE_SECRET_KEY";
/// Optional host override env var.
pub const ENV_HOST: &str = "LANGFUSE_HOST";
/// Alias for [`ENV_HOST`] (some setups use `LANGFUSE_BASE_URL`).
pub const ENV_BASE_URL: &str = "LANGFUSE_BASE_URL";

/// A configured Langfuse client. Build once with [`LangfuseTracer::from_env`]
/// and share it (clone the `Arc`) across turns; the inner `reqwest::Client`
/// holds the connection pool.
#[derive(Debug, Clone)]
pub struct LangfuseTracer {
    http: reqwest::Client,
    base_url: String,
    public_key: String,
    secret_key: String,
}

/// One finished turn to report. `outcome` is the assistant reply on success or
/// the error message on failure.
#[derive(Debug, Clone)]
pub struct TurnReport {
    /// Chat session id, used as the Langfuse `sessionId` for grouping.
    pub session_id: Option<String>,
    /// Provider-side model id sent upstream.
    pub model: String,
    /// `Build` or `Plan`.
    pub mode: String,
    /// The user text the turn answered.
    pub input: String,
    /// `Ok(reply)` on success, `Err(message)` on failure.
    pub outcome: Result<String, String>,
    /// When the turn started.
    pub start: DateTime<Utc>,
    /// When the turn finished.
    pub end: DateTime<Utc>,
}

impl LangfuseTracer {
    /// Build from the environment, or `None` when Langfuse is not configured.
    ///
    /// Reads [`ENV_PUBLIC_KEY`], [`ENV_SECRET_KEY`], and [`ENV_HOST`]
    /// (defaulting to Langfuse Cloud EU). Returns `None` — and
    /// builds no HTTP client — unless both keys are present and non-blank, so
    /// an unconfigured engine pays nothing.
    pub fn from_env() -> Option<Self> {
        let public_key = non_blank(env::var(ENV_PUBLIC_KEY).ok())?;
        let secret_key = non_blank(env::var(ENV_SECRET_KEY).ok())?;
        // Accept either LANGFUSE_HOST (the name in Langfuse's own docs) or the
        // LANGFUSE_BASE_URL alias, defaulting to Langfuse Cloud EU.
        let host = non_blank(env::var(ENV_HOST).ok())
            .or_else(|| non_blank(env::var(ENV_BASE_URL).ok()))
            .unwrap_or_else(|| DEFAULT_HOST.to_string());
        Some(Self {
            http: reqwest::Client::builder()
                .timeout(Duration::from_secs(10))
                .build()
                .expect("failed to build Langfuse reqwest client"),
            base_url: host.trim_end_matches('/').to_string(),
            public_key,
            secret_key,
        })
    }

    /// Report one finished turn to Langfuse. Never fails the caller: a
    /// transport error or non-success status is logged at WARN and swallowed,
    /// so observability cannot break a turn. Intended to be `tokio::spawn`ed.
    /// Uses [`reqwest`](https://docs.rs/reqwest/latest/reqwest/) for the POST
    /// to Langfuse's ingestion API.
    pub async fn report_turn(&self, report: TurnReport) {
        let batch = build_batch(&report);
        let url = format!("{}/api/public/ingestion", self.base_url);
        tracing::debug!(session = ?report.session_id, "reporting turn to langfuse");
        let resp = self
            .http
            .post(&url)
            .basic_auth(&self.public_key, Some(&self.secret_key))
            .json(&batch)
            .send()
            .await;
        match resp {
            Ok(r) => {
                let status = r.status();
                let body = r.text().await.unwrap_or_default();
                if !status.is_success() {
                    tracing::warn!(
                        %status,
                        body_len = body.len(),
                        "langfuse ingestion rejected the batch"
                    );
                } else if has_event_errors(&body) {
                    // A 207 can still carry per-event validation errors.
                    tracing::warn!(
                        %status,
                        body_len = body.len(),
                        "langfuse ingestion reported per-event errors"
                    );
                } else {
                    tracing::debug!(%status, "langfuse ingestion accepted the turn");
                }
            }
            Err(e) => tracing::warn!(error = %e, "langfuse ingestion request failed"),
        }
    }
}

/// `true` when an ingestion response body carries a non-empty `errors` array.
fn has_event_errors(body: &str) -> bool {
    serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|v| {
            v.get("errors")
                .and_then(Value::as_array)
                .map(|a| !a.is_empty())
        })
        .unwrap_or(false)
}

impl LangfuseTracer {
    /// Verify the configured credentials by calling Langfuse's authenticated
    /// `GET /api/public/projects` endpoint. Returns the project name on success
    /// or a human-readable error. Used as a startup self-check so a bad key or
    /// host is reported immediately instead of silently dropping every turn.
    pub async fn health_check(&self) -> Result<String, String> {
        let url = format!("{}/api/public/projects", self.base_url);
        let resp = self
            .http
            .get(&url)
            .basic_auth(&self.public_key, Some(&self.secret_key))
            .send()
            .await
            .map_err(|e| format!("request to {url} failed: {e}"))?;
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        if status.is_success() {
            Ok(project_name(&body))
        } else {
            Err(format!("{status}: {body}"))
        }
    }
}

/// Best-effort extraction of the first project name from a `/projects`
/// response body, falling back to `"unknown"` when the shape is unexpected.
pub fn project_name(body: &str) -> String {
    serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|v| v["data"][0]["name"].as_str().map(str::to_string))
        .unwrap_or_else(|| "unknown".to_string())
}

/// `Some(s)` only when `s` is present and not all-whitespace.
fn non_blank(v: Option<String>) -> Option<String> {
    v.map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

/// Build the Langfuse ingestion batch for one turn: a `trace-create` event plus
/// a `generation-create` observation nested under it (linked by `traceId`).
///
/// A failed turn is recorded with `level: "ERROR"` and the error text
/// as both the output and the status message.
pub fn build_batch(r: &TurnReport) -> Value {
    let trace_id = Uuid::new_v4().to_string();
    let gen_id = Uuid::new_v4().to_string();
    let start = r.start.to_rfc3339();
    let end = r.end.to_rfc3339();

    let (output, level, status_message) = match &r.outcome {
        Ok(reply) => (json!(reply), "DEFAULT", Value::Null),
        Err(msg) => (json!(msg), "ERROR", json!(msg)),
    };

    json!({
        "batch": [
            {
                "id": Uuid::new_v4().to_string(),
                "type": "trace-create",
                "timestamp": start,
                "body": {
                    "id": trace_id,
                    "timestamp": start,
                    "name": "chat-turn",
                    "sessionId": r.session_id,
                    "input": r.input,
                    "output": output,
                    "metadata": { "mode": r.mode },
                }
            },
            {
                "id": Uuid::new_v4().to_string(),
                "type": "generation-create",
                "timestamp": start,
                "body": {
                    "id": gen_id,
                    "traceId": trace_id,
                    "name": "completion",
                    "startTime": start,
                    "endTime": end,
                    "model": r.model,
                    "input": r.input,
                    "output": output,
                    "level": level,
                    "statusMessage": status_message,
                    "metadata": { "mode": r.mode },
                }
            }
        ]
    })
}
