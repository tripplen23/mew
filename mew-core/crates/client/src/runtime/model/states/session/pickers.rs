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
}

/// State for the model picker overlay.
#[derive(Debug, Default)]
pub struct ModelPickerState {
    /// Cached model registry for the [`super::Overlay::ModelPicker`] overlay.
    pub models: Option<Vec<ModelEntry>>,
    pub picker: PickerState,
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
