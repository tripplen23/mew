use std::time::Instant;

use uuid::Uuid;

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
    /// Render-only display (e.g. a code diff); never sent to the model.
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

    /// Append `delta`, merging into the trailing text run: consecutive deltas
    /// stay one paragraph, while a delta after a tool starts a new run.
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
            // Prefer the streamed summary; fall back when nothing streamed.
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
