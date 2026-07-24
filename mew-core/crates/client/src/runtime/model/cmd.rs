use crate::net::{CreateSessionRequest, SessionPatch};
use mewcode_protocol::event::{ChatRequest, ChoiceResponseRequest};

/// Text the user types to exit the TUI.
pub const QUIT_COMMAND: &str = "quit";

/// Side effects the runtime should perform after an `update`.
#[derive(Debug)]
pub enum Cmd {
    /// Do nothing.
    None,
    /// Create a new session
    CreateSession(CreateSessionRequest),
    /// Start a chat turn.
    StartChat(ChatRequest),
    /// Submit a pending structured choice answer.
    SubmitChoice(ChoiceResponseRequest),
    /// Fetch the model registry for the picker overlay.
    FetchModels,
    /// Fetch the skill catalog for the skills overlay.
    FetchSkills,
    /// Fetch the session list for the picker overlay.
    FetchSessions,
    FetchFiles,
    /// Apply a partial update to a session via `PATCH /sessions/{id}`.
    PatchSession {
        /// Id of the session to update.
        id: uuid::Uuid,
        /// The fields to change.
        patch: SessionPatch,
        /// `true` if this PATCH came from `/session rename`.
        /// Allows clearing the rename draft + overlay even if the user Esc'd out.
        from_rename: bool,
    },
    /// Switch the active session to `id`; hydrates it via `GET /sessions/{id}`.
    OpenSession(uuid::Uuid),
    /// Delete a session by id.
    DeleteSession(uuid::Uuid),
    /// Exit the TUI. Triggered when the user types [`QUIT_COMMAND`].
    Quit,
    /// Play the completion notification sound.
    PlayNotificationSound,
    /// Trigger manual context compaction for the current session.
    Compact(uuid::Uuid),
    /// Run multiple commands.
    Batch(Vec<Cmd>),
}
