use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use mewcode_protocol::ToolDisplay;
use serde_json::Value;

/// Display record tagged with the tool's input arguments.
///
/// Tools never see Rig's call id, so they stamp display data with `args`
/// and the stream layer matches it back by value.
#[derive(Debug, Clone)]
pub struct DisplayRecord {
    /// The tool's input arguments — identical to what the model sent.
    pub args: Value,
    /// The render payload.
    pub display: ToolDisplay,
}

/// Out-of-band sink for render-only display (diffs, previews). Kept off
/// the model path so display payloads never enter the context window.
pub type DisplaySink = Arc<Mutex<Vec<DisplayRecord>>>;

/// Project context. Every tool needs to know what directory to operate on.
#[derive(Debug, Clone)]
pub struct ProjectContext {
    /// Absolute path to the project root the tools operate on.
    pub root: PathBuf,
    /// Display sink. `Some` when streaming to a UI, `None` in headless paths.
    pub display: Option<DisplaySink>,
}

impl ProjectContext {
    /// Build a context rooted at the given directory, with no display sink.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            display: None,
        }
    }

    /// Attach a display sink so mutating tools can emit render-only data.
    pub fn with_display(mut self, sink: DisplaySink) -> Self {
        self.display = Some(sink);
        self
    }

    /// Record a display payload for `args` if a sink is attached. Cheap no-op
    /// otherwise, so tools can call it unconditionally.
    pub fn push_display(&self, args: Value, display: ToolDisplay) {
        if let Some(sink) = &self.display {
            if let Ok(mut records) = sink.lock() {
                records.push(DisplayRecord { args, display });
            }
        }
    }
}
