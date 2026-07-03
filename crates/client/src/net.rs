//! Network client (server API).

use eventsource_stream::Eventsource;
use futures::{Stream, StreamExt};
use mewcode_protocol::event::ChatRequest;
use mewcode_protocol::routes::{CHAT, HEALTH, MODELS, SESSION_BY_ID, SESSIONS};
use mewcode_protocol::{Message, Mode, ModelId, ModelKind, StreamEvent};
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Errors raised at the network boundary.
///
/// This is the library-level error type for [`ApiClient`]'s session and chat
/// operations. The runtime maps it to a user-facing toast string, keeping
/// `anyhow` at the app boundary only.
#[derive(Debug, thiserror::Error)]
pub enum NetError {
    /// The HTTP request never produced a response (DNS, connect, timeout, ...).
    #[error("transport error: {0}")]
    Transport(#[from] reqwest::Error),
    /// The server responded with a non-success status code.
    #[error("server returned status {0}")]
    Status(reqwest::StatusCode),
    /// A response body (or SSE frame) failed to decode into the expected type.
    #[error("decode error: {0}")]
    Decode(#[from] serde_json::Error),
    /// The SSE stream broke mid-flight (dropped connection or malformed frame).
    #[error("stream error: {0}")]
    Stream(String),
}

/// HTTP client wrapper.
#[derive(Debug, Clone)]
pub struct ApiClient {
    base_url: String,
    inner: reqwest::Client,
}

/// Response payload of `GET /health`.
#[derive(Debug, Clone, Deserialize)]
pub struct HealthResponse {
    /// `true` when the server is up.
    pub ok: bool,
    /// Service name.
    pub service: String,
    /// Service version.
    pub version: String,
}

/// One entry in the model registry returned by `GET /models`.
#[derive(Debug, Clone, Deserialize)]
pub struct ModelEntry {
    /// Provider-side model id.
    pub id: String,
    /// Human-friendly display name.
    pub display_name: String,
    /// Which OpenCode Go endpoint serves the model.
    pub kind: ModelKind,
}

/// A lightweight view of a session, without message history. Mirrors the
/// server's `SessionSummary` wire shape.
#[derive(Debug, Clone, Deserialize)]
pub struct SessionSummary {
    /// Unique session identifier.
    pub id: uuid::Uuid,
    /// Human-readable title.
    pub title: String,
    /// Model selected for the session.
    pub model: ModelId,
    /// Interaction mode for the session.
    pub mode: Mode,
    /// When the session was created.
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// A full session including its message history. Mirrors the server's
/// `Session` wire shape.
#[derive(Debug, Clone, Deserialize)]
pub struct Session {
    /// Unique session identifier.
    pub id: uuid::Uuid,
    /// Human-readable title.
    pub title: String,
    /// Model selected for the session.
    pub model: ModelId,
    /// Interaction mode for the session.
    pub mode: Mode,
    /// When the session was created.
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// When the session was last updated.
    pub updated_at: chrono::DateTime<chrono::Utc>,
    /// Ordered message history.
    pub messages: Vec<Message>,
}

/// Request body for `POST /sessions`. Mirrors the server's
/// `CreateSessionRequest`: only `title` is required; `model` and `mode`
/// fall back to server defaults when omitted.
#[derive(Debug, Clone, Serialize)]
pub struct CreateSessionRequest {
    /// Human-readable title (required, non-empty after trimming server-side).
    pub title: String,
    /// Model to use; server default when `None`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<ModelId>,
    /// Interaction mode; server default when `None`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<Mode>,
}

/// Partial update for a session. Mirrors the server's `SessionPatch`: each
/// `Some` field is applied; `None` fields are left unchanged.
#[derive(Debug, Clone, Default, Serialize)]
pub struct SessionPatch {
    /// New title. `None` keeps the current title.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// New model. `None` keeps the current model.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<ModelId>,
    /// New mode. `None` keeps the current mode.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<Mode>,
}

impl ApiClient {
    /// Build a new client. `base_url` should not have a trailing slash.
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            inner: reqwest::Client::new(),
        }
    }

    /// `GET /health`
    pub async fn health(&self) -> reqwest::Result<HealthResponse> {
        self.inner
            .get(format!("{}{}", self.base_url, HEALTH))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await
    }

    /// `GET /models` — fetch the model registry.
    ///
    /// A per-request 10 s timeout bounds exactly this call, so a hung registry
    /// fetch fails fast (into the picker's fallback) instead of wedging the
    /// dialog open forever. Transport error, non-success status, decode
    /// failure, or timeout all surface as one [`NetError`].
    pub async fn models(&self) -> Result<Vec<ModelEntry>, NetError> {
        let resp = self
            .inner
            .get(format!("{}{}", self.base_url, MODELS))
            .timeout(Duration::from_secs(10))
            .send()
            .await?;
        let bytes = ensure_success(resp)?.bytes().await?;
        Ok(serde_json::from_slice(&bytes)?)
    }

    /// `GET /sessions` — list every session as a summary, newest-first.
    pub async fn list_sessions(&self) -> Result<Vec<SessionSummary>, NetError> {
        let resp = self
            .inner
            .get(format!("{}{}", self.base_url, SESSIONS))
            .send()
            .await?;
        let bytes = ensure_success(resp)?.bytes().await?;
        Ok(serde_json::from_slice(&bytes)?)
    }

    /// `POST /sessions` — create a session and return it.
    pub async fn create_session(&self, new: &CreateSessionRequest) -> Result<Session, NetError> {
        let resp = self
            .inner
            .post(format!("{}{}", self.base_url, SESSIONS))
            .json(new)
            .send()
            .await?;
        let bytes = ensure_success(resp)?.bytes().await?;
        Ok(serde_json::from_slice(&bytes)?)
    }

    /// `GET /sessions/{id}` — fetch one session with its messages hydrated.
    pub async fn get_session(&self, id: uuid::Uuid) -> Result<Session, NetError> {
        let path = SESSION_BY_ID.replace("{id}", &id.to_string());
        let resp = self
            .inner
            .get(format!("{}{}", self.base_url, path))
            .send()
            .await?;
        let bytes = ensure_success(resp)?.bytes().await?;
        Ok(serde_json::from_slice(&bytes)?)
    }

    /// `PATCH /sessions/{id}` — apply a partial update and return the
    /// refreshed session.
    pub async fn patch_session(
        &self,
        id: uuid::Uuid,
        patch: &SessionPatch,
    ) -> Result<Session, NetError> {
        let path = SESSION_BY_ID.replace("{id}", &id.to_string());
        let resp = self
            .inner
            .patch(format!("{}{}", self.base_url, path))
            .json(patch)
            .send()
            .await?;
        let bytes = ensure_success(resp)?.bytes().await?;
        Ok(serde_json::from_slice(&bytes)?)
    }

    /// `DELETE /sessions/{id}` — delete a session.
    pub async fn delete_session(&self, id: uuid::Uuid) -> Result<(), NetError> {
        let path = SESSION_BY_ID.replace("{id}", &id.to_string());
        let resp = self
            .inner
            .delete(format!("{}{}", self.base_url, path))
            .send()
            .await?;
        let _ = ensure_success(resp)?.bytes().await?;
        Ok(())
    }

    /// `POST /chat` — open the SSE chat stream.
    ///
    /// A failed open (transport error or non-success status) returns
    /// `Err(NetError)`. Once open, each SSE frame's `data` field is decoded
    /// into a [`StreamEvent`]; a mid-stream decode failure or dropped
    /// connection surfaces as a `Result::Err(NetError)` stream item rather
    /// than panicking.
    pub async fn chat_stream(
        &self,
        req: &ChatRequest,
    ) -> Result<impl Stream<Item = Result<StreamEvent, NetError>>, NetError> {
        let resp = self
            .inner
            .post(format!("{}{}", self.base_url, CHAT))
            .json(req)
            .send()
            .await?;
        let resp = ensure_success(resp)?;
        let stream = resp.bytes_stream().eventsource().map(|frame| match frame {
            Ok(event) => serde_json::from_str::<StreamEvent>(&event.data).map_err(NetError::from),
            Err(e) => Err(NetError::Stream(e.to_string())),
        });
        Ok(stream)
    }

    /// Resolve a model id string into the registry.
    pub fn model_id(&self, id: &str) -> Option<ModelId> {
        id.parse().ok()
    }

    /// Base URL the client is configured against.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }
}

/// Map a non-success HTTP response into a typed [`NetError::Status`], leaving
/// successful responses untouched. Unlike `reqwest::Response::error_for_status`,
/// this keeps the response body available for typed decoding.
fn ensure_success(resp: reqwest::Response) -> Result<reqwest::Response, NetError> {
    let status = resp.status();
    if status.is_success() {
        Ok(resp)
    } else {
        Err(NetError::Status(status))
    }
}
