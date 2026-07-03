//! Storage abstraction for sessions.
//!
//! Defines the [`SessionStore`] trait and the shared DTOs used by every
//! backend (in-memory or filesystem). Backends return [`StoreError`] at their
//! boundary; no backend-specific error type appears in the trait signatures.

use std::path::PathBuf;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use mewcode_protocol::{Message, Mode, ModelId};
use serde::{Deserialize, Serialize};

use crate::AppError;

pub mod fs;
pub mod memory;

/// Which storage backend is active.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backend {
    /// In-memory store (non-persistent).
    Memory,
    /// Filesystem-backed store (persistent).
    Filesystem,
}

impl Backend {
    /// Render the wire label for this backend (`"memory"`).
    pub fn as_str(&self) -> &'static str {
        match self {
            Backend::Memory => "memory",
            Backend::Filesystem => "filesystem",
        }
    }
}

/// Errors produced at the storage boundary.
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    /// The requested entity does not exist.
    #[error("not found")]
    NotFound,
    /// Input was invalid (e.g. an unparsable `ModelId` or `Mode`, or a
    /// corrupt `meta.json`).
    #[error("invalid: {0}")]
    Invalid(String),
    /// A filesystem I/O error at the storage boundary.
    #[error(transparent)]
    Io(#[from] std::io::Error),
    /// A (de)serialization error reading or writing stored JSON.
    #[error(transparent)]
    Serde(#[from] serde_json::Error),
}

impl From<StoreError> for AppError {
    fn from(e: StoreError) -> Self {
        match e {
            StoreError::NotFound => AppError::NotFound,
            StoreError::Invalid(s) => AppError::BadRequest(s),
            StoreError::Io(e) => AppError::Internal(e.to_string()),
            StoreError::Serde(e) => AppError::Internal(e.to_string()),
        }
    }
}

/// A full session including its message history.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
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
    pub created_at: DateTime<Utc>,
    /// When the session was last updated.
    pub updated_at: DateTime<Utc>,
    /// Ordered message history.
    pub messages: Vec<Message>,
}

/// A lightweight view of a session, without message history.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
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
    pub created_at: DateTime<Utc>,
}

/// Input for creating a new session.
///
/// Values are already resolved: unparsable inputs are rejected upstream and
/// surfaced as [`StoreError::Invalid`].
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct NewSession {
    /// Human-readable title.
    pub title: String,
    /// Model selected for the session.
    pub model: ModelId,
    /// Interaction mode for the session.
    pub mode: Mode,
}

/// Partial update for a session. Each `Some` field is applied to the
/// stored session; `None` fields are left unchanged.
#[derive(Debug, Clone, Default, Serialize, Deserialize, utoipa::ToSchema)]
pub struct SessionPatch {
    /// New title. `None` keeps the current title.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// New model. `None` keeps the current model.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<ModelId>,
    /// New mode. `None` keeps the current mode.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<Mode>,
}

/// Storage abstraction over session persistence.
///
/// Implementations must be object-safe so they can be held behind
/// `Arc<dyn SessionStore>`; [`macro@async_trait`] is used because native
/// async-fn-in-trait is not yet dyn-compatible with a clean `Send` bound.
#[async_trait]
pub trait SessionStore: Send + Sync {
    /// Which backend this store represents. Synchronous.
    fn backend(&self) -> Backend;

    /// The resolved data directory, when the backend is filesystem-backed.
    ///
    /// Returns `None` for non-persistent backends (the in-memory store).
    fn data_dir_path(&self) -> Option<PathBuf> {
        None
    }

    /// List all sessions as summaries.
    async fn list_sessions(&self) -> Result<Vec<SessionSummary>, StoreError>;

    /// Fetch a full session by id.
    async fn get_session(&self, id: uuid::Uuid) -> Result<Session, StoreError>;

    /// Create a new session and return it.
    async fn create_session(&self, new: NewSession) -> Result<Session, StoreError>;

    /// Apply a partial update to a session. `None` fields are left unchanged.
    /// Returns the refreshed session, or [`StoreError::NotFound`] if the id
    /// does not exist.
    async fn patch_session(
        &self,
        id: uuid::Uuid,
        patch: SessionPatch,
    ) -> Result<Session, StoreError>;

    /// Delete a session by id.
    async fn delete_session(&self, id: uuid::Uuid) -> Result<(), StoreError>;

    /// Append a message to a session's history.
    async fn append_message(&self, id: uuid::Uuid, message: Message) -> Result<(), StoreError>;
}
