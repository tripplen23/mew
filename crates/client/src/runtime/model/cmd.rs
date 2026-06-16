use uuid::Uuid;

use crate::net::CreateSessionRequest;
use mewcode_protocol::event::ChatRequest;

/// Side effects the runtime should perform after an `update`.
#[derive(Debug)]
pub enum Cmd {
    /// Do nothing.
    None,
    /// Fetch the session list.
    LoadSessions,
    /// Create a new session.
    CreateSession(CreateSessionRequest),
    /// Open/hydrate a session by id.
    OpenSession(Uuid),
    /// Start a chat turn.
    StartChat(ChatRequest),
}
