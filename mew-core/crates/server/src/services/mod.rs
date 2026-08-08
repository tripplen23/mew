pub mod chat;
pub mod compact;
pub mod github_bot;
pub mod runtime;

/// Client-facing message for the missing-session failure, shared by the chat
/// and compaction services so the wording cannot drift between them.
pub(crate) const SESSION_NOT_FOUND: &str = "session not found";
