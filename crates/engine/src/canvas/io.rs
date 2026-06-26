//! Disk I/O for the architecture canvas: read `graph.json` and
//! `layout.json` from `.mewcode/canvas/`, write them back. Pure
//! serialization — no auto-layout here; `layout::auto_layout` is
//! called by the consumer after `load` returns a `Layout`.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use mewcode_protocol::canvas::{Graph, Layout};

/// Directory under the project root that holds canvas files.
pub const CANVAS_DIR: &str = ".mewcode/canvas";
/// File name for the semantic graph (the source of truth).
pub const GRAPH_FILE: &str = "graph.json";
/// File name for the presentation overlay.
pub const LAYOUT_FILE: &str = "layout.json";

/// Read the canvas graph and layout from disk under `project_root`.
/// Missing files yield an empty graph and an empty layout (the
/// "first run" case) rather than an error. Existing files are
/// strictly deserialized: a `version` mismatch or invalid JSON
/// surfaces as `Err`. No auto-layout is performed here — the
/// caller resolves missing positions with `layout::auto_layout`
/// after deciding whether to mutate the graph first.
pub fn load(project_root: impl AsRef<Path>) -> io::Result<(Graph, Layout)> {
    let graph = read_graph_or_default(project_root.as_ref())?;
    let layout = read_layout_or_default(project_root.as_ref())?;
    Ok((graph, layout))
}

/// Write the graph to `.mewcode/canvas/graph.json` as pretty JSON.
/// Creates the canvas directory if it does not exist.
pub fn save_graph(project_root: impl AsRef<Path>, graph: &Graph) -> io::Result<()> {
    let path = canvas_path(project_root.as_ref(), GRAPH_FILE);
    write_pretty(&path, graph)
}

/// Write the layout to `.mewcode/canvas/layout.json` as pretty JSON.
/// Creates the canvas directory if it does not exist.
pub fn save_layout(project_root: impl AsRef<Path>, layout: &Layout) -> io::Result<()> {
    let path = canvas_path(project_root.as_ref(), LAYOUT_FILE);
    write_pretty(&path, layout)
}

fn read_graph_or_default(project_root: &Path) -> io::Result<Graph> {
    let path = canvas_path(project_root, GRAPH_FILE);
    match fs::read(&path) {
        Ok(bytes) => Ok(serde_json::from_slice(&bytes)?),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Graph::default()),
        Err(e) => Err(e),
    }
}

fn read_layout_or_default(project_root: &Path) -> io::Result<Layout> {
    let path = canvas_path(project_root, LAYOUT_FILE);
    match fs::read(&path) {
        Ok(bytes) => Ok(serde_json::from_slice(&bytes)?),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Layout::default()),
        Err(e) => Err(e),
    }
}

fn canvas_path(project_root: &Path, file: &str) -> PathBuf {
    project_root.join(CANVAS_DIR).join(file)
}

fn write_pretty<T: serde::Serialize>(path: &Path, value: &T) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(value)
        .map_err(|e| io::Error::other(format!("serialize {path:?}: {e}")))?;
    fs::write(path, json)
}
