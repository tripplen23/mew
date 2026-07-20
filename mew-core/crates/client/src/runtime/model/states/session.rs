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

/// State backing [`super::Screen::Session`].
///
/// The TUI always opens here. `session` is `None` until the user sends
/// their first message, at which point the runtime `POST /sessions` to
/// create one and lifts the result into this field. The `pending_chat`
/// text is what triggered the create; it becomes the first user message
/// once the session lands. `creating` is true while that POST is in
/// flight so the input can be disabled and a spinner can be shown.
#[derive(Debug)]
pub struct SessionState {
    /// The hydrated session, including history.
    pub session: Option<Session>,
    /// The message composer.
    pub input: TextArea<'static>,
    /// Full pasted bodies hidden behind short composer markers.
    pub pasted: Vec<PastedText>,
    /// First message of a not-yet-created session.
    pub pending_chat: Option<String>,
    /// Model picked before the first session exists.
    pub pending_model: Option<ModelId>,
    /// Mode picked before the first session exists.
    pub pending_mode: Option<Mode>,
    /// `true` while a `POST /sessions` is in flight for `pending_chat`.
    pub creating: bool,
    /// When `creating` was set true
    pub creation_started_at: Option<Instant>,
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
}

impl SessionState {
    /// A blank session screen: no session, no pending chat, no streaming.
    /// This is the entry state the TUI launches into.
    pub fn empty() -> Self {
        Self {
            session: None,
            input: TextArea::default(),
            pasted: Vec::new(),
            pending_chat: None,
            pending_model: None,
            pending_mode: None,
            creating: false,
            creation_started_at: None,
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

/// One ordered element of an in-flight assistant turn: a run of assistant text
/// or a tool call (with its eventual result/display). Kept in arrival order so
/// both the live view and the committed message match the runtime stream.
#[derive(Debug, Clone)]
pub enum TurnItem {
    /// A run of assistant text (consecutive deltas merged).
    Text(String),
    /// A tool call and its result/display as they arrive.
    Tool(ToolCallView),
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
            if let TurnItem::Text(t) = it {
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
