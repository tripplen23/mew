//! The application's state types, grouped by where they live.
//!
//! `App` and `Screen` are the top-level state machine; `Toast` is a
//! cross-screen UI primitive. The remaining structs are split per screen
//! (matching the layout in [`super::super::view`] and [`super::super::update`])
//! so the file you open matches the file you'd change.

use std::time::Instant;

mod session;

pub use session::{
    ModelPickerState, Overlay, PastedText, SLASH_COMMANDS, SessionListState, SessionState,
    SlashCommand, StreamingState, ToolCallView,
};

/// The whole application state.
///
/// The current view is held solely as a single [`Screen`] value; there is no
/// screen-specific data outside its variant. Quitting is signalled by the
/// `Cmd::Quit` variant returned from `update`; the model doesn't carry
/// any quit flag of its own.
#[derive(Debug)]
pub struct App {
    /// The screen currently being shown, owning its own state.
    pub screen: Screen,
    /// Transient status message, if any.
    pub toast: Option<Toast>,
}

impl App {
    /// Build a fresh app sitting on a blank session screen. The first
    /// message the user sends will create a new session on the server.
    pub fn new() -> Self {
        Self {
            screen: Screen::Session(SessionState::empty()),
            toast: None,
        }
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

/// The set of screens the TUI can show. Data lives inside each variant so
/// illegal states are unrepresentable.
#[derive(Debug)]
pub enum Screen {
    /// An open chat session — may or may not yet have a session on the server.
    Session(SessionState),
}

/// Severity of a [`Toast`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToastKind {
    /// Informational message.
    Info,
    /// Error message.
    Error,
}

/// A transient status message shown to the user.
// `Instant` is not `PartialEq`, so this only derives `Debug, Clone`.
#[derive(Debug, Clone)]
pub struct Toast {
    /// Message body.
    pub text: String,
    /// Whether this is an info or error toast.
    pub kind: ToastKind,
    /// When the toast was raised (for the fade-out animation).
    pub started_at: Instant,
}

impl Toast {
    /// Build an error toast.
    pub fn error(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            kind: ToastKind::Error,
            started_at: Instant::now(),
        }
    }

    /// Build an info toast.
    pub fn info(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            kind: ToastKind::Info,
            started_at: Instant::now(),
        }
    }
}
