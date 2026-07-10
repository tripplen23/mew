use uuid::Uuid;

use crossterm::event::KeyEvent;

use crate::net::{ModelEntry, Session, SessionSummary};

/// Messages that drive the [`super::App`] through `update`.
#[derive(Debug)]
pub enum Msg {
    /// A key was pressed.
    Key(KeyEvent),
    /// Text pasted into the terminal.
    Paste(String),
    /// A periodic tick (for animations / elapsed time).
    Tick,
    /// A new session finished being created.
    SessionCreated(Result<Session, CreateError>),
    /// A streaming event arrived.
    Stream(StreamMsg),
    /// The model registry was fetched (or failed).
    ModelsFetched(Result<Vec<ModelEntry>, String>),
    /// The session list was fetched (or failed).
    SessionsFetched(Result<Vec<SessionSummary>, String>),
    /// A `PATCH /sessions/{id}` returned a refreshed session (or failed).
    /// `from_rename` is `true` when the request originated from
    /// `/session rename` so the update loop can clear the rename draft.
    SessionPatched(Result<Session, String>, bool),
    /// A single session was hydrated (or failed). Used after `/session switch`.
    SessionOpened(Result<Session, String>),
    /// A `DELETE /sessions/{id}` completed (or failed).
    SessionDeleted(Result<uuid::Uuid, String>),
}

/// Why a `POST /sessions` failed.
///
/// Distinguishes the empty-title server error from every other failure so
/// `update` can branch without re-deriving HTTP semantics. In the
/// chat-first flow the title is always derived from a non-empty first
/// message, so `EmptyTitle` is effectively unreachable — kept for
/// forward-compat.
#[derive(Debug)]
pub enum CreateError {
    /// The server rejected the request because the title was empty.
    EmptyTitle(String),
    /// Any other failure (transport, decode, non-4xx status).
    Other(String),
}

/// Streaming sub-messages, decoded from server SSE events.
#[derive(Debug)]
pub enum StreamMsg {
    /// Stream started; carries the assistant message id.
    Started(Uuid),
    /// A chunk of assistant text.
    Delta(String),
    /// The model is calling a tool.
    ToolInput {
        /// Stable id of the call.
        id: String,
        /// Tool name.
        name: String,
        /// JSON arguments.
        input: serde_json::Value,
    },
    /// A tool call produced output.
    ToolOutput {
        /// Id of the call this result is for.
        id: String,
        /// JSON output.
        output: serde_json::Value,
    },
    /// Stream finished successfully.
    Finished {
        /// Wall-clock duration in milliseconds.
        duration_ms: u64,
    },
    /// Stream failed.
    Failed(String),
}
