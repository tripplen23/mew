use crate::net::{CreateSessionRequest, SessionPatch};
use mewcode_protocol::event::ChatRequest;

/// Text the user types to exit the TUI.
pub const QUIT_COMMAND: &str = "quit";

/// Side effects the runtime should perform after an `update`.
#[derive(Debug)]
pub enum Cmd {
    /// Do nothing.
    None,
    /// Create a new session. Used when the user sends their first message
    /// in the chat-first flow; the result is auto-routed into the session
    /// view via `Msg::SessionCreated`.
    CreateSession(CreateSessionRequest),
    /// Start a chat turn.
    StartChat(ChatRequest),
    /// Fetch the model registry for the picker overlay.
    FetchModels,
    /// Fetch the session list for the picker overlay.
    FetchSessions,
    /// Apply a partial update to a session via `PATCH /sessions/{id}`.
    /// Carries the whole `SessionPatch` so a single cmd covers rename,
    /// model change, and mode change.
    PatchSession {
        /// Id of the session to update.
        id: uuid::Uuid,
        /// The fields to change.
        patch: SessionPatch,
    },
    /// Switch the active session to `id`; hydrates it via `GET /sessions/{id}`.
    OpenSession(uuid::Uuid),
    /// Delete a session by id.
    DeleteSession(uuid::Uuid),
    /// Exit the TUI. Triggered when the user types [`QUIT_COMMAND`].
    Quit,
}
