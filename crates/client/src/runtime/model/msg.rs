use uuid::Uuid;

use crossterm::event::KeyEvent;

use crate::net::{Session, SessionSummary};

/// Messages that drive the [`super::App`] through `update`.
#[derive(Debug)]
pub enum Msg {
    /// A key was pressed.
    Key(KeyEvent),
    /// A periodic tick (for animations / elapsed time).
    Tick,
    /// The session list finished loading.
    SessionsLoaded(Result<Vec<SessionSummary>, String>),
    /// A new session finished being created.
    SessionCreated(Result<Session, String>),
    /// A session finished being opened/hydrated.
    SessionOpened(Result<Session, String>),
    /// A streaming event arrived.
    Stream(StreamMsg),
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
