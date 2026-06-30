use crate::net::CreateSessionRequest;
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
    /// Exit the TUI. Triggered when the user types [`QUIT_COMMAND`].
    Quit,
}
