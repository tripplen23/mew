//! Render-only tool display payloads.
//!
//! `ToolDisplay` is carried on a dedicated stream event
//! ([`StreamEvent::ToolDisplayAvailable`](crate::StreamEvent)) and stored on
//! [`ToolResult::display`](crate::ToolResult) for the client to render. It is
//! deliberately **separate from a tool's model-facing output**: the engine
//! never feeds it into the model's context (history mapping forwards text
//! parts only), so a diff can be shown to the human without spending model
//! tokens. This is the "display channel" side of the diff feature.

use serde::{Deserialize, Serialize};

/// A render-only payload attached to a tool call for the client UI.
///
/// Tagged by `kind` so new display shapes (e.g. a rendered table or image
/// preview) can be added without breaking existing consumers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ToolDisplay {
    /// A before/after code diff for a file-mutating tool.
    Diff(DiffDisplay),
}

/// The data needed to render a unified diff for a single file edit.
///
/// `old` and `new` are the raw before/after text; the client computes and
/// colors the actual hunks. For a targeted edit these are the replaced
/// fragment and its replacement; for a whole-file write they are the previous
/// file contents (empty for a new file) and the new contents.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub struct DiffDisplay {
    /// Path of the edited file, relative to the project root.
    pub path: String,
    /// 1-based line where the change starts, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_line: Option<u64>,
    /// Text before the edit (the removed side). Empty for a new file.
    pub old: String,
    /// Text after the edit (the added side).
    pub new: String,
    /// `true` if `old`/`new` were truncated because the file was very large;
    /// the client then notes the diff is partial.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub truncated: bool,
}

impl DiffDisplay {
    /// Cap (in bytes) on each side of a diff carried over the wire, so a huge
    /// overwrite cannot bloat the stream. The write still happens in full;
    /// only the *display* is bounded.
    // ponytail: naive byte-prefix cut (may split a UTF-8 char boundary — we
    // round down to a char boundary). Upgrade path: cap by line count instead.
    pub const MAX_SIDE_BYTES: usize = 64 * 1024;

    /// Build a diff display, truncating each side to [`Self::MAX_SIDE_BYTES`]
    /// on a char boundary and flagging when either side was cut.
    pub fn new(path: impl Into<String>, start_line: Option<u64>, old: &str, new: &str) -> Self {
        let (old_c, old_t) = truncate_side(old);
        let (new_c, new_t) = truncate_side(new);
        Self {
            path: path.into(),
            start_line,
            old: old_c,
            new: new_c,
            truncated: old_t || new_t,
        }
    }
}

/// Truncate one side to the byte cap on a UTF-8 char boundary; returns the
/// possibly-shortened string and whether it was cut.
fn truncate_side(s: &str) -> (String, bool) {
    if s.len() <= DiffDisplay::MAX_SIDE_BYTES {
        return (s.to_string(), false);
    }
    let mut end = DiffDisplay::MAX_SIDE_BYTES;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    (s[..end].to_string(), true)
}
