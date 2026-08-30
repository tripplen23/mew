use ratatui::layout::Rect;
use tui_textarea::TextArea;

use crate::net::{ModelEntry, SessionSummary};

/// One file listed by the file picker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileEntry {
    pub path: String,
    pub is_dir: bool,
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
    /// Absolute rect of the picker's inner content rows from the last render,
    /// so `update` can route mouse clicks and wheel scrolls.
    pub rect: Option<Rect>,
}

/// State for the model picker overlay.
#[derive(Debug, Default)]
pub struct ModelPickerState {
    /// Cached model registry for the [`super::Overlay::ModelPicker`] overlay.
    pub models: Option<Vec<ModelEntry>>,
    /// Local model search input.
    pub query: TextArea<'static>,
    /// Latest fetch generation; older asynchronous responses are ignored.
    pub generation: u64,
    pub picker: PickerState,
}

impl ModelPickerState {
    /// Models matching the display name, exact upstream id, or provider.
    pub fn filtered_models(&self) -> Vec<&ModelEntry> {
        let query = self.query.lines().join("").to_lowercase();
        self.models
            .as_deref()
            .unwrap_or_default()
            .iter()
            .filter(|model| {
                query.is_empty()
                    || model.display_name.to_lowercase().contains(&query)
                    || model.id.to_lowercase().contains(&query)
                    || model.provider.to_string().to_lowercase().contains(&query)
            })
            .collect()
    }
}

/// State for the session list overlay.
#[derive(Debug, Default)]
pub struct SessionListState {
    /// Cached session summaries for the [`super::Overlay::SessionList`] overlay.
    pub summaries: Vec<SessionSummary>,
    pub picker: PickerState,
}

/// State for the `@` file picker.
#[derive(Debug, Default)]
pub struct FilePickerState {
    pub files: Option<Vec<FileEntry>>,
    pub picker: PickerState,
}
