use uuid::Uuid;

use crossterm::event::KeyEvent;

use super::FileEntry;
use crate::net::{ModelEntry, Session, SessionSummary, SkillEntry};
use mewcode_protocol::event::ChoiceRequest;

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
    /// A structured choice request arrived outside a stream.
    ChoiceRequested(ChoiceRequest),
    /// A pending structured choice answer was submitted.
    ChoiceSubmitted(Result<(), String>),
    /// The model registry was fetched (or failed).
    ModelsFetched(Result<Vec<ModelEntry>, String>),
    /// The skill catalog was fetched (or failed).
    SkillsFetched(Result<Vec<SkillEntry>, String>),
    /// The session list was fetched (or failed).
    SessionsFetched(Result<Vec<SessionSummary>, String>),
    FilesFetched(Result<Vec<FileEntry>, String>),
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
    Started { id: Uuid, pwd: Option<String> },
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
    /// Render-only display data (e.g. a diff) for a tool call.
    ToolDisplay {
        /// Id of the call this display is for.
        id: String,
        /// The render payload.
        display: mewcode_protocol::ToolDisplay,
    },
    /// Stream finished successfully.
    Finished {
        /// Wall-clock duration in milliseconds.
        duration_ms: u64,
        /// Current session token total.
        session_tokens: Option<u64>,
        /// Model context limit.
        context_limit: Option<u64>,
    },
    /// Stream failed.
    Failed(String),
    /// Runtime asks the client to render a structured choice prompt.
    ChoiceRequest(ChoiceRequest),
    /// Manual compaction has started.
    CompactionStarted,
    /// Compaction progress update.
    CompactionProgress {
        /// Current phase.
        phase: mewcode_protocol::event::CompactionPhase,
        /// Human-readable status message.
        message: String,
    },
    /// A chunk of the compaction summary, streamed as the LLM generates it.
    CompactionSummaryDelta(String),
    /// History was compacted during this turn.
    Compacted {
        tokens_before: u64,
        context_limit: u64,
        summary: String,
        thought_duration_ms: u64,
    },
}
