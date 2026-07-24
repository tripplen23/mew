use std::time::Instant;

use tui_textarea::TextArea;
use uuid::Uuid;

use crate::net::{ModelEntry, Session, SessionSummary, SkillEntry};
use mewcode_protocol::event::{ChoiceCancelReason, ChoiceRequest, ChoiceResponse};
use mewcode_protocol::{Mode, ModelId};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileEntry {
    pub path: String,
    pub is_dir: bool,
}

/// One row in the slash-command picker.
#[derive(Debug, Clone, Copy)]
pub struct SlashCommand {
    /// What to seed the composer with, e.g. `"/model"`.
    pub command: &'static str,
    /// Single-line explanation shown next to the row.
    pub description: &'static str,
}

/// Catalog of slash commands surfaced in the picker.
pub const SLASH_COMMANDS: &[SlashCommand] = &[
    SlashCommand {
        command: "/model",
        description: "Switch model",
    },
    SlashCommand {
        command: "/session",
        description: "List sessions",
    },
    SlashCommand {
        command: "/session new",
        description: "Create a new session",
    },
    SlashCommand {
        command: "/session rename",
        description: "Rename current session",
    },
    SlashCommand {
        command: "/tools",
        description: "List tools",
    },
    SlashCommand {
        command: "/skills",
        description: "List skills",
    },
    SlashCommand {
        command: "/theme",
        description: "Pick theme",
    },
    SlashCommand {
        command: "/mode",
        description: "Switch mode",
    },
    SlashCommand {
        command: "/sound",
        description: "Toggle notification sound",
    },
    SlashCommand {
        command: "/compact",
        description: "Compact conversation context",
    },
    SlashCommand {
        command: "quit",
        description: "Exit the TUI",
    },
];

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
    /// Rename the active session; the input bar takes the new title.
    RenameSession,
    /// The slash-command picker shown when the composer starts with `/`.
    SlashPicker,
    /// The file picker shown when the current composer token starts with `@`.
    FilePicker,
    /// Theme picker overlay.
    Theme,
    /// Structured single-select choice prompt.
    Choice,
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

/// State backing [`super::Screen::Session`].
#[derive(Debug)]
pub struct SessionState {
    /// The hydrated session, including history.
    pub session: Option<Session>,
    /// The message composer.
    pub input: TextArea<'static>,
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
    /// FIFO queue of messages the user submitted while a turn was in flight
    pub message_queue: Vec<String>,
}

impl SessionState {
    /// A blank session screen: no session, no pending chat, no streaming.
    /// This is the entry state the TUI launches into.
    pub fn empty() -> Self {
        Self {
            session: None,
            input: TextArea::default(),
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
            sound_enabled: true,
            compaction: CompactionUiState::default(),
            pwd: None,
            session_tokens: 0,
            context_limit: 0,
            message_queue: Vec::new(),
        }
    }

    /// Open a session view for an already-hydrated [`Session`].
    pub fn new(session: Session) -> Self {
        Self {
            session: Some(session),
            ..Self::empty()
        }
    }
}

#[derive(Debug)]
pub struct ChoicePromptState {
    pub request: ChoiceRequest,
    pub picker: PickerState,
    pub started_at: Instant,
    pub response: Option<ChoiceResponse>,
}

impl ChoicePromptState {
    pub fn new(request: ChoiceRequest) -> Self {
        Self {
            request,
            picker: PickerState::default(),
            started_at: Instant::now(),
            response: None,
        }
    }

    pub fn cancel(&mut self, reason: ChoiceCancelReason) {
        self.response = Some(ChoiceResponse::Cancelled {
            request_id: self.request.request_id.clone(),
            reason,
        });
    }
}

#[derive(Debug, Default)]
pub struct FilePickerState {
    pub files: Option<Vec<FileEntry>>,
    pub picker: PickerState,
}

/// A multiline paste represented by a short marker in the composer.
#[derive(Debug, Clone)]
pub struct PastedText {
    /// Marker inserted into the visible composer.
    pub marker: String,
    /// Original pasted text submitted when the marker is present.
    pub text: String,
}

/// State for the model picker overlay.
#[derive(Debug, Default)]
pub struct ModelPickerState {
    /// Cached model registry for the [`Overlay::ModelPicker`] overlay.
    pub models: Option<Vec<ModelEntry>>,
    pub picker: PickerState,
}

/// State for the session list overlay.
#[derive(Debug, Default)]
pub struct SessionListState {
    /// Cached session summaries for the [`Overlay::SessionList`] overlay.
    pub summaries: Vec<SessionSummary>,
    pub picker: PickerState,
}

/// Cursor and viewport state shared by scrollable picker overlays.
#[derive(Debug, Default)]
pub struct PickerState {
    /// Highlighted row (0-based).
    pub cursor: usize,
    /// Vertical scroll offset.
    pub scroll: usize,
    /// Inner height of the overlay as last rendered.
    pub viewport: u16,
    /// Largest viewport the view has ever reported.
    pub viewport_max: u16,
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
    /// Render-only display (e.g. a code diff), once it arrives. Never sent to
    /// the model — purely for the transcript card.
    pub display: Option<mewcode_protocol::ToolDisplay>,
}

/// Compaction metadata displayed inline in the transcript.
#[derive(Debug, Clone)]
pub struct CompactionView {
    /// Tokens used before compaction fired.
    pub tokens_before: u64,
    /// Model context limit.
    pub context_limit: u64,
    /// LLM-generated summary text.
    pub summary: String,
    /// Wall-clock duration of the compaction call in ms.
    pub thought_duration_ms: u64,
}

/// A committed compaction entry stored in session history.
#[derive(Debug, Clone)]
pub struct CompactionEntry {
    /// Number of committed messages at the time of compaction.
    pub after_message_count: usize,
    /// Compaction metadata.
    pub view: CompactionView,
}

/// One ordered element of an in-flight assistant turn: a run of assistant text
/// or a tool call (with its eventual result/display). Kept in arrival order so
/// both the live view and the committed message match the runtime stream.
#[derive(Debug, Clone)]
pub enum TurnItem {
    /// A run of assistant text (consecutive deltas merged).
    Text(String),
    /// A tool call and its result/display as they arrive.
    Tool(ToolCallView),
    /// An inline compaction section.
    Compaction(CompactionView),
    /// Transient progress text rendered inline but never committed to history
    Progress(String),
}

/// State of an in-flight assistant turn.
#[derive(Debug)]
pub struct StreamingState {
    /// Id of the assistant message being produced.
    pub assistant_id: Uuid,
    /// Turn content in arrival order (text runs interleaved with tool calls).
    pub items: Vec<TurnItem>,
    /// When the turn started (for elapsed-time display / animations).
    pub started_at: Instant,
}

impl StreamingState {
    /// Begin tracking a new assistant turn.
    pub fn new(assistant_id: Uuid) -> Self {
        Self {
            assistant_id,
            items: Vec::new(),
            started_at: Instant::now(),
        }
    }

    /// Append a text delta, merging into a trailing text run so consecutive
    /// deltas stay one paragraph but text after a tool starts a new run.
    pub fn push_text(&mut self, delta: &str) {
        match self.items.last_mut() {
            Some(TurnItem::Text(t)) => t.push_str(delta),
            _ => self.items.push(TurnItem::Text(delta.to_string())),
        }
    }

    /// Record a new tool call in arrival order.
    pub fn push_tool_call(&mut self, view: ToolCallView) {
        self.items.push(TurnItem::Tool(view));
    }

    /// Record transient progress text rendered inline but never committed.
    pub fn push_progress(&mut self, text: &str) {
        self.items.push(TurnItem::Progress(text.to_string()));
    }

    /// Record a compaction event in arrival order.
    pub fn push_compaction(&mut self, view: CompactionView) {
        self.items.push(TurnItem::Compaction(view));
    }

    /// Append a chunk of a streaming compaction summary, merging consecutive
    /// deltas like [`Self::push_text`] does for chat replies. First delta
    /// creates a placeholder item; [`Self::finish_compaction`] fills metadata later.
    pub fn push_compaction_delta(&mut self, delta: &str) {
        match self.items.last_mut() {
            Some(TurnItem::Compaction(view)) => view.summary.push_str(delta),
            _ => self.items.push(TurnItem::Compaction(CompactionView {
                tokens_before: 0,
                context_limit: 0,
                summary: delta.to_string(),
                thought_duration_ms: 0,
            })),
        }
    }

    /// Set metadata on the trailing `Compaction` item after `Compacted` arrives.
    /// Keeps any summary text already streamed via [`Self::push_compaction_delta`].
    /// Falls back to creating a new item if no in-progress compaction exists.
    pub fn finish_compaction(
        &mut self,
        tokens_before: u64,
        context_limit: u64,
        summary: &str,
        thought_duration_ms: u64,
    ) {
        if let Some(TurnItem::Compaction(view)) = self.items.last_mut() {
            view.tokens_before = tokens_before;
            view.context_limit = context_limit;
            view.thought_duration_ms = thought_duration_ms;
            // The streamed summary should already match; prefer it only if
            // nothing was streamed (defensive, shouldn't normally happen).
            if view.summary.is_empty() {
                view.summary = summary.to_string();
            }
            return;
        }
        self.push_compaction(CompactionView {
            tokens_before,
            context_limit,
            summary: summary.to_string(),
            thought_duration_ms,
        });
    }

    /// Find the most recent tool call with `id` to attach its output/display.
    pub fn tool_mut(&mut self, id: &str) -> Option<&mut ToolCallView> {
        self.items.iter_mut().rev().find_map(|it| match it {
            TurnItem::Tool(v) if v.id == id => Some(v),
            _ => None,
        })
    }

    /// Concatenated assistant text across the whole turn.
    pub fn text(&self) -> String {
        let mut out = String::new();
        for it in &self.items {
            if let TurnItem::Text(t) | TurnItem::Progress(t) = it {
                out.push_str(t);
            }
        }
        out
    }

    /// `true` if no text or tool activity has been recorded yet.
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}
