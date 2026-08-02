//! Session screen state: hydrated session, composer, overlays, streaming,
//! compaction, and the transcript render cache. Split by concern: slash
//! commands, transcript cache, connect dialog, picker states, streaming, and
//! choice prompts each live in a sibling module; file-picker queries live in
//! the `file_picker` submodule.

use std::time::Instant;

use tui_textarea::TextArea;

use mewcode_protocol::{Mode, ModelId};

use crate::net::{Session, SkillEntry};

mod choice;
mod connect;
mod file_picker;
mod pickers;
mod slash;
mod streaming;
mod transcript_cache;

pub use choice::ChoicePromptState;
pub use connect::{ConnectProviderState, ConnectStep};
pub use pickers::{FileEntry, FilePickerState, ModelPickerState, PickerState, SessionListState};
pub use slash::{SLASH_COMMANDS, SlashCommand, SlashCommandKind, slash_command_by_token};
pub use streaming::{CompactionEntry, CompactionView, StreamingState, ToolCallView, TurnItem};
pub use transcript_cache::{CachedBlock, TranscriptCache};

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
    /// The model picker: lists flattened `GET /providers` model entries.
    ModelPicker,
    /// The session list: lists every saved session
    SessionList,
    /// Rename the active session; the composer bar takes the new title.
    RenameSession,
    /// The slash-command picker shown when the composer starts with `/`.
    SlashPicker,
    /// The file picker shown when the current composer token starts with `@`.
    FilePicker,
    /// Theme picker overlay.
    Theme,
    /// Structured single-select choice prompt.
    Choice,
    /// Provider connect dialog (pick provider → enter key → validate → done).
    ConnectProvider,
}

/// State while a session is being created from the first typed message.
///
/// `session` is `None` until the user sends their first message, at which
/// point the runtime `POST /sessions` to create one and lifts the result
/// into `SessionState::session`. `pending_chat` is what triggered the
/// create; it becomes the first user message once the session lands.
/// `creating` is true while that POST is in flight so the input can be
/// disabled and a spinner can be shown.
#[derive(Debug, Default)]
pub struct CreationState {
    /// First message of a not-yet-created session.
    pub pending_chat: Option<String>,
    /// Model picked before the first session exists.
    pub pending_model: Option<ModelId>,
    /// Mode picked before the first session exists.
    pub pending_mode: Option<Mode>,
    /// `true` while a `POST /sessions` is in flight for `pending_chat`.
    pub creating: bool,
    /// When `creating` was set true.
    pub creation_started_at: Option<Instant>,
}

/// State for one manual `/compact` round-trip, plus the entries it (and
/// automatic compaction) have committed to the transcript so far.
#[derive(Debug, Default)]
pub struct CompactionUiState {
    /// `true` while a `POST /sessions/{id}/compact` is in flight.
    pub active: bool,
    /// When `active` was set true (for spinner animation).
    pub started_at: Option<Instant>,
    /// Committed compaction entries (manual or automatic).
    pub committed: Vec<CompactionEntry>,
}

/// State backing [`super::super::Screen::Session`].
#[derive(Debug)]
pub struct SessionState {
    /// The hydrated session, including history.
    pub session: Option<Session>,
    /// The message composer.
    pub composer: TextArea<'static>,
    /// Full pasted bodies hidden behind short composer markers.
    pub pasted: Vec<PastedText>,
    /// Session-creation-in-progress state (`pending_chat`, `creating`, ...).
    pub creation: CreationState,
    /// Vertical scroll offset of the transcript, in wrapped lines from the top.
    pub scroll: u16,
    /// When `true`, the transcript stays pinned to its latest line.
    pub follow: bool,
    /// Largest valid `scroll` for the last rendered frame
    pub max_scroll: u16,
    /// Transcript viewport height from the last rendered frame
    pub viewport: u16,
    /// `Some` while an assistant turn is in flight.
    pub streaming: Option<StreamingState>,
    /// Which overlay (if any) is showing.
    pub overlay: Overlay,
    /// Model picker overlay state.
    pub model_picker: ModelPickerState,
    /// Session list overlay state.
    pub session_list: SessionListState,
    /// Cached skill catalog for the [`Overlay::Skills`] overlay.
    pub skills: Option<Vec<SkillEntry>>,
    /// Highlighted row in the slash-command picker (0-based).
    pub slash_cursor: usize,
    /// File picker with @ command
    pub file_picker: FilePickerState,
    /// Pending structured choice prompt, if any.
    pub pending_choice: Option<ChoicePromptState>,
    /// Provider connect dialog state.
    pub connect_provider: ConnectProviderState,
    /// Whether the notification sound plays after each assistant turn.
    pub sound_enabled: bool,
    /// Manual-`/compact`-in-progress state plus committed compaction entries.
    pub compaction: CompactionUiState,
    /// Server working directory, received from the Start event.
    pub pwd: Option<String>,
    /// Current session token total, received from the Finish event.
    pub session_tokens: u64,
    /// Model context limit, received from the Finish event.
    pub context_limit: u64,
    /// Running session cost in USD, accumulated from Finish events.
    pub session_cost_usd: f64,
    /// FIFO queue of messages the user submitted while a turn was in flight
    pub message_queue: Vec<String>,
    /// Rendered-lines cache for committed transcript history. See
    /// [`TranscriptCache`] for why this exists and its invalidation rules.
    pub transcript_cache: TranscriptCache,
}

impl SessionState {
    /// A blank session screen: no session, no pending chat, no streaming.
    /// This is the entry state the TUI launches into.
    pub fn empty() -> Self {
        Self {
            session: None,
            composer: TextArea::default(),
            pasted: Vec::new(),
            creation: CreationState::default(),
            scroll: 0,
            follow: true,
            max_scroll: 0,
            viewport: 0,
            streaming: None,
            overlay: Overlay::None,
            model_picker: ModelPickerState::default(),
            session_list: SessionListState::default(),
            skills: None,
            slash_cursor: 0,
            file_picker: FilePickerState::default(),
            pending_choice: None,
            connect_provider: ConnectProviderState::default(),
            sound_enabled: true,
            compaction: CompactionUiState::default(),
            pwd: None,
            session_tokens: 0,
            context_limit: 0,
            session_cost_usd: 0.0,
            message_queue: Vec::new(),
            transcript_cache: TranscriptCache::default(),
        }
    }

    /// Open a session view for an already-hydrated [`Session`].
    pub fn new(session: Session) -> Self {
        Self {
            session: Some(session),
            ..Self::empty()
        }
    }

    /// The composer's visible text, joined as it renders.
    pub fn composer_text(&self) -> String {
        self.composer.lines().join("\n")
    }

    /// Clear the composer and any pending pasted-text markers.
    pub fn clear_composer(&mut self) {
        self.composer = TextArea::default();
        self.pasted.clear();
    }
}

/// A multiline paste represented by a short marker in the composer.
#[derive(Debug, Clone)]
pub struct PastedText {
    /// Marker inserted into the visible composer.
    pub marker: String,
    /// Original pasted text submitted when the marker is present.
    pub text: String,
}

/// Shared prefix of pasted-text markers.
pub const PASTED_MARKER_PREFIX: &str = "[Pasted ~";
