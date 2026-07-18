use std::time::Instant;

use tui_textarea::TextArea;
use uuid::Uuid;

use crate::net::{ModelEntry, Session, SessionSummary, SkillEntry};
use mewcode_protocol::ModelId;

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
    /// Theme picker overlay.
    Theme,
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
    /// First message of a not-yet-created session, kept so it can be sent
    /// as the user message the moment `SessionCreated` arrives.
    pub pending_chat: Option<String>,
    /// Model picked before the first session exists.
    pub pending_model: Option<ModelId>,
    /// `true` while a `POST /sessions` is in flight for `pending_chat`.
    pub creating: bool,
    /// When `creating` was set true; used by the view to drive the
    /// "starting session…" spinner. `None` while not creating so a stale
    /// instant is never shown.
    pub creation_started_at: Option<Instant>,
    /// Vertical scroll offset of the transcript, in wrapped lines from the top.
    pub scroll: u16,
    /// When `true`, the transcript stays pinned to its latest line.
    pub follow: bool,
    /// Largest valid `scroll` for the last rendered frame (content lines minus
    /// viewport height). Written by the view, read by the key handler so it can
    /// clamp scrolling and know when the bottom has been reached.
    pub max_scroll: u16,
    /// Transcript viewport height from the last rendered frame, used as the
    /// PageUp/PageDown step.
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
    /// Highlighted row in the model picker (0-based).
    pub cursor: usize,
    /// Vertical scroll offset (in rows) of the model picker.
    pub scroll: usize,
    /// Inner height of the model-picker overlay as last rendered.
    pub viewport: u16,
    /// Largest model-picker viewport the view has ever reported.
    pub viewport_max: u16,
}

/// State for the session list overlay.
#[derive(Debug, Default)]
pub struct SessionListState {
    /// Cached session summaries for the [`Overlay::SessionList`] overlay.
    pub summaries: Vec<SessionSummary>,
    /// Highlighted row in the session list (0-based).
    pub cursor: usize,
    /// Vertical scroll offset of the session list.
    pub scroll: usize,
    /// Inner height of the session-list overlay as last rendered.
    pub viewport: u16,
    /// Largest session-list viewport the view has ever reported.
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
