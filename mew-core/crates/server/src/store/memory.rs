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

use super::{
    Backend, CompactionPatch, NewSession, Session, SessionPatch, SessionStore, SessionSummary,
    StoreError, compaction_patch, validate_compaction_checkpoint, validate_title,
};

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
    model: mewcode_protocol::ModelRef,
    /// Runtime transport captured from provider discovery.
    model_kind: Option<mewcode_protocol::ModelKind>,
    /// Runtime context limit captured from provider discovery.
    model_context_length: Option<u64>,
    /// Interaction mode for the session.
    mode: mewcode_protocol::Mode,
    /// When the session was created.
    created_at: chrono::DateTime<Utc>,
    /// When the session was last updated.
    updated_at: chrono::DateTime<Utc>,
    /// Optional compaction summary from the last manual or automatic compaction.
    compaction_summary: Option<String>,
    /// Message index already covered by `compaction_summary`.
    compacted_up_to: Option<usize>,
    /// Stable id of the message at `compacted_up_to - 1`.
    compacted_up_to_message_id: Option<Uuid>,
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
            model: self.model.clone(),
            model_kind: self.model_kind,
            model_context_length: self.model_context_length,
            mode: self.mode,
            created_at: self.created_at,
        }
    }

    /// Hydrate a stored row into a full [`Session`] with the given messages.
    fn to_session(&self, messages: Vec<Message>) -> Session {
        Session {
            id: self.id,
            title: self.title.clone(),
            model: self.model.clone(),
            model_kind: self.model_kind,
            model_context_length: self.model_context_length,
            mode: self.mode,
            created_at: self.created_at,
            updated_at: self.updated_at,
            messages,
            todos: Vec::new(),
            compaction_summary: self.compaction_summary.clone(),
            compacted_up_to: self.compacted_up_to,
            compacted_up_to_message_id: self.compacted_up_to_message_id,
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

        let messages = guard.messages.get(&id).cloned().unwrap_or_default();
        Ok(row.to_session(messages))
    }

    async fn create_session(&self, new: NewSession) -> Result<Session, StoreError> {
        let now = Utc::now();
        let row = SessionRow {
            id: Uuid::new_v4(),
            title: new.title,
            model: new.model,
            model_kind: new.model_kind,
            model_context_length: new.model_context_length,
            mode: new.mode,
            created_at: now,
            updated_at: now,
            compaction_summary: None,
            compacted_up_to: None,
            compacted_up_to_message_id: None,
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

    async fn patch_session(&self, id: Uuid, patch: SessionPatch) -> Result<Session, StoreError> {
        let compaction = compaction_patch(&patch)?;
        let mut guard = self.inner.write().await;
        if !guard.sessions.iter().any(|session| session.id == id) {
            return Err(StoreError::NotFound);
        }
        let messages = guard.messages.get(&id).cloned().unwrap_or_default();
        if let CompactionPatch::Set {
            up_to, message_id, ..
        } = &compaction
        {
            validate_compaction_checkpoint(&messages, *up_to, *message_id)?;
        }
        let row = guard
            .sessions
            .iter_mut()
            .find(|s| s.id == id)
            .ok_or(StoreError::NotFound)?;
        if let Some(title) = patch.title {
            row.title = validate_title(&title)?;
        }
        if let Some(model) = patch.model {
            row.model = model;
            row.model_kind = patch.model_kind;
            row.model_context_length = patch.model_context_length;
        } else {
            if patch.model_kind.is_some() {
                row.model_kind = patch.model_kind;
            }
            if patch.model_context_length.is_some() {
                row.model_context_length = patch.model_context_length;
            }
        }
        if let Some(mode) = patch.mode {
            row.mode = mode;
        }
        match compaction {
            CompactionPatch::Unchanged => {}
            CompactionPatch::Clear => {
                row.compaction_summary = None;
                row.compacted_up_to = None;
                row.compacted_up_to_message_id = None;
            }
            CompactionPatch::Set {
                summary,
                up_to,
                message_id,
            } => {
                row.compaction_summary = Some(summary);
                row.compacted_up_to = Some(up_to);
                row.compacted_up_to_message_id = Some(message_id);
            }
        }
        row.updated_at = Utc::now();
        let snapshot = row.clone();
        Ok(snapshot.to_session(messages))
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
