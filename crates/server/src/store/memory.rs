//! In-memory [`SessionStore`] implementation.
//!
//! This is the ephemeral / test path. It keeps sessions in a
//! `Vec` (newest-first on read) and messages in a side `HashMap` keyed by
//! session id, mirroring the hydration shape of the future `FsStore` so both
//! backends behave identically.

use std::collections::HashMap;

use async_trait::async_trait;
use chrono::Utc;
use mewcode_protocol::Message;
use tokio::sync::RwLock;
use uuid::Uuid;

use super::{Backend, NewSession, Session, SessionStore, SessionSummary, StoreError};

/// In-memory session store, guarded by a single async `RwLock`.
#[derive(Debug, Default)]
pub struct MemoryStore {
    /// All mutable state, behind one lock.
    inner: RwLock<MemState>,
}

/// The locked interior of a [`MemoryStore`].
#[derive(Debug, Default)]
struct MemState {
    /// Sessions in newest-first order (most recent insert at the front).
    sessions: Vec<SessionRow>,
    /// Message history keyed by session id, mirroring the Pg side table.
    messages: HashMap<Uuid, Vec<Message>>,
}

/// A stored session without its message history (the side map holds messages).
#[derive(Debug, Clone)]
struct SessionRow {
    /// Unique session identifier.
    id: Uuid,
    /// Human-readable title.
    title: String,
    /// Model selected for the session.
    model: mewcode_protocol::ModelId,
    /// Interaction mode for the session.
    mode: mewcode_protocol::Mode,
    /// When the session was created.
    created_at: chrono::DateTime<Utc>,
    /// When the session was last updated.
    updated_at: chrono::DateTime<Utc>,
}

impl MemoryStore {
    /// Create an empty store.
    pub fn new() -> Self {
        Self::default()
    }
}

impl SessionRow {
    /// Project a stored row into a wire [`SessionSummary`].
    fn to_summary(&self) -> SessionSummary {
        SessionSummary {
            id: self.id,
            title: self.title.clone(),
            model: self.model,
            mode: self.mode,
            created_at: self.created_at,
        }
    }

    /// Hydrate a stored row into a full [`Session`] with the given messages.
    fn to_session(&self, messages: Vec<Message>) -> Session {
        Session {
            id: self.id,
            title: self.title.clone(),
            model: self.model,
            mode: self.mode,
            created_at: self.created_at,
            updated_at: self.updated_at,
            messages,
        }
    }
}

#[async_trait]
impl SessionStore for MemoryStore {
    fn backend(&self) -> Backend {
        Backend::Memory
    }

    async fn list_sessions(&self) -> Result<Vec<SessionSummary>, StoreError> {
        let guard = self.inner.read().await;
        Ok(guard.sessions.iter().map(SessionRow::to_summary).collect())
    }

    async fn get_session(&self, id: Uuid) -> Result<Session, StoreError> {
        let guard = self.inner.read().await;
        let row = guard
            .sessions
            .iter()
            .find(|s| s.id == id)
            .ok_or(StoreError::NotFound)?;

        // Hydrate messages ordered by `created_at` ascending, mirroring the
        // Pg `(session_id, created_at)` index ordering.
        let mut messages = guard.messages.get(&id).cloned().unwrap_or_default();
        messages.sort_by_key(|m| m.created_at);
        Ok(row.to_session(messages))
    }

    async fn create_session(&self, new: NewSession) -> Result<Session, StoreError> {
        let now = Utc::now();
        let row = SessionRow {
            id: Uuid::new_v4(),
            title: new.title,
            model: new.model,
            mode: new.mode,
            created_at: now,
            updated_at: now,
        };
        let session = row.to_session(Vec::new());

        let mut guard = self.inner.write().await;
        guard.messages.insert(row.id, Vec::new());
        // Newest-first: most recent session lives at the front.
        guard.sessions.insert(0, row);
        Ok(session)
    }

    async fn delete_session(&self, id: Uuid) -> Result<(), StoreError> {
        let mut guard = self.inner.write().await;
        let before = guard.sessions.len();
        guard.sessions.retain(|s| s.id != id);
        if guard.sessions.len() == before {
            return Err(StoreError::NotFound);
        }
        guard.messages.remove(&id);
        Ok(())
    }

    async fn append_message(&self, id: Uuid, message: Message) -> Result<(), StoreError> {
        let mut guard = self.inner.write().await;
        let state = &mut *guard;
        let row = state
            .sessions
            .iter_mut()
            .find(|s| s.id == id)
            .ok_or(StoreError::NotFound)?;
        row.updated_at = Utc::now();
        state.messages.entry(id).or_default().push(message);
        Ok(())
    }
}
