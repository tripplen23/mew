use std::time::Instant;

use tui_textarea::TextArea;
use uuid::Uuid;

use crate::net::{Session, SessionSummary};
use mewcode_protocol::Mode;

/// The whole application state.
///
/// The current view is held solely as a single [`Screen`] value; there is no
/// screen-specific data outside its variant.
#[derive(Debug)]
pub struct App {
    /// The screen currently being shown, owning its own state.
    pub screen: Screen,
    /// Transient status message, if any.
    pub toast: Option<Toast>,
    /// Set once the user has asked to quit; the event loop checks this.
    pub should_quit: bool,
}

impl App {
    /// Build a fresh app sitting on a loading Home screen.
    pub fn new() -> Self {
        Self {
            screen: Screen::Home(HomeState::loading()),
            toast: None,
            should_quit: false,
        }
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

/// The set of screens the TUI can show. Data lives inside each variant so
/// illegal states (e.g. a session view with no session) are unrepresentable.
#[derive(Debug)]
pub enum Screen {
    /// Session list / launcher.
    Home(HomeState),
    /// New-session creation form.
    NewSession(NewSessionState),
    /// An open chat session.
    Session(SessionState),
}

/// State backing [`Screen::Home`].
#[derive(Debug)]
pub struct HomeState {
    /// Sessions shown in the list.
    pub sessions: Vec<SessionSummary>,
    /// Index of the highlighted row.
    pub selected: usize,
    /// `true` while the session list is being fetched.
    pub loading: bool,
}

impl HomeState {
    /// A Home screen in its initial loading state, before sessions arrive.
    pub fn loading() -> Self {
        Self {
            sessions: Vec::new(),
            selected: 0,
            loading: true,
        }
    }
}

/// Which field of the new-session form currently has focus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NewSessionField {
    /// The title text input.
    Title,
    /// The model picker.
    Model,
    /// The mode (Build/Plan) toggle.
    Mode,
}

impl NewSessionField {
    /// The next field in the focus cycle: Title → Model → Mode → Title.
    pub fn next(self) -> Self {
        match self {
            NewSessionField::Title => NewSessionField::Model,
            NewSessionField::Model => NewSessionField::Mode,
            NewSessionField::Mode => NewSessionField::Title,
        }
    }
}

/// State backing [`Screen::NewSession`].
#[derive(Debug)]
pub struct NewSessionState {
    /// The session title editor.
    pub title: TextArea<'static>,
    /// Index into `mewcode_protocol::ModelId::ALL` for the selected model.
    pub model_idx: usize,
    /// Selected interaction mode.
    pub mode: Mode,
    /// Which field currently has focus.
    pub field: NewSessionField,
}

impl Default for NewSessionState {
    fn default() -> Self {
        Self {
            title: TextArea::default(),
            model_idx: 0,
            mode: Mode::default(),
            field: NewSessionField::Title,
        }
    }
}

/// An overlay panel layered over the session view.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum Overlay {
    /// No overlay.
    #[default]
    None,
    /// The tools list overlay.
    Tools,
    /// The skills list overlay.
    Skills,
}

/// State backing [`Screen::Session`].
#[derive(Debug)]
pub struct SessionState {
    /// The hydrated session, including history. Cannot be omitted.
    pub session: Session,
    /// The message composer.
    pub input: TextArea<'static>,
    /// Vertical scroll offset of the transcript.
    pub scroll: u16,
    /// `Some` while an assistant turn is in flight.
    pub streaming: Option<StreamingState>,
    /// Which overlay (if any) is showing.
    pub overlay: Overlay,
}

impl SessionState {
    /// Open a session view for an already-hydrated [`Session`].
    pub fn new(session: Session) -> Self {
        Self {
            session,
            input: TextArea::default(),
            scroll: 0,
            streaming: None,
            overlay: Overlay::None,
        }
    }
}

/// A lightweight view of a tool call accumulated during streaming.
#[derive(Debug, Clone)]
pub struct ToolCallView {
    /// Stable id of the call.
    pub id: String,
    /// Tool name.
    pub name: String,
    /// JSON arguments.
    pub input: serde_json::Value,
    /// JSON output, once the call finishes.
    pub output: Option<serde_json::Value>,
}

/// State of an in-flight assistant turn.
#[derive(Debug)]
pub struct StreamingState {
    /// Id of the assistant message being produced.
    pub assistant_id: Uuid,
    /// Accumulated assistant text so far.
    pub buffer: String,
    /// Tool calls seen during this turn.
    pub tool_calls: Vec<ToolCallView>,
    /// When the turn started (for elapsed-time display / animations).
    pub started_at: Instant,
}

impl StreamingState {
    /// Begin tracking a new assistant turn.
    pub fn new(assistant_id: Uuid) -> Self {
        Self {
            assistant_id,
            buffer: String::new(),
            tool_calls: Vec::new(),
            started_at: Instant::now(),
        }
    }
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
