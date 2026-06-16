//! The application model for the Elm-style runtime.
//!
//! > Idiom: make illegal states unrepresentable. Each [`Screen`] variant owns
//! > its own state, so the chat view ([`Screen::Session`]) can never exist
//! > without a hydrated [`crate::net::Session`] — the compiler guarantees it,
//! > and there are no `Option` fields to forget to populate.

mod cmd;
mod msg;
mod state;

pub use cmd::Cmd;
pub use msg::{Msg, StreamMsg};
pub use state::{
    App, HomeState, NewSessionField, NewSessionState, Overlay, Screen, SessionState,
    StreamingState, Toast, ToastKind, ToolCallView,
};
