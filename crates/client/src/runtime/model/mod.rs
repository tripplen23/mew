//! The application model for the Elm-style runtime.
//!
//! > Idiom: make illegal states unrepresentable. The TUI has a single screen
//! > ([`Screen::Session`]) whose `session` field is `Option<Session>` — the
//! > chat screen is the only place to be, and the placeholder-before-first-
//! > message and the loaded-with-history states are spelled out by the type
//! > so neither can be forgotten.

mod cmd;
mod msg;
mod states;

pub use cmd::Cmd;
pub use msg::{CreateError, Msg, StreamMsg};
pub use states::{
    App, Overlay, Screen, SessionState, StreamingState, Toast, ToastKind, ToolCallView,
};
