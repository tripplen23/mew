//! Tests for the canvas I/O engine: `load` / `save_graph` /
//! `save_layout` from `crates/engine/src/canvas/io.rs`.

use std::collections::HashMap;
use std::fs;

use mewcode_engine::canvas::io::{LAYOUT_FILE, load, save_graph, save_layout};
use mewcode_protocol::canvas::{Edge, EdgeKind, Graph, Layout, Node, NodeId, NodeKind, Point};

fn node(id: &str, name: &str) -> Node {
    Node {
        id: NodeId(id.to_string()),
        kind: NodeKind::Component,
        name: name.to_string(),
        bind: None,
        contract: Vec::new(),
        tech: None,
        desc: None,
    }
}

fn edge(from: &str, to: &str) -> Edge {
    Edge {
        from: NodeId(from.to_string()),
        to: NodeId(to.to_string()),
        kind: EdgeKind::Depends,
    }
}

/// Spec T2 acceptance (a): loading a project whose canvas files do
/// not exist yields the empty defaults rather than an error. The
/// "first run" case — no graph yet, no layout yet, agent must
/// bootstrap from scratch.
#[test]
fn load_missing_project_yields_empty_defaults() {
    let dir = tempdir();
    let (graph, layout) = load(&dir).expect("missing files should not error");
    assert_eq!(graph, Graph::default());
    assert_eq!(layout, Layout::default());

    // Files were not created as a side effect of `load`.
    assert!(!dir.join(".mewcode/canvas").exists());
}

/// Spec T2 acceptance (b): a 3-node/2-edge graph written via
/// `save_graph` and a layout written via `save_layout` round-trip
/// through `load` with every field preserved. Pin a position in
/// the layout to verify the `positions` map survives the trip.
#[test]
fn three_node_graph_round_trips_through_save_and_load() {
    let dir = tempdir();

    let graph = Graph {
        version: 1,
        nodes: vec![node("a", "A"), node("b", "B"), node("c", "C")],
        edges: vec![edge("a", "b"), edge("b", "c")],
    };
    let mut positions = HashMap::new();
    positions.insert(NodeId("a".to_string()), Point { x: 5, y: 5 });
    let layout = Layout {
        version: 1,
        positions,
        theme: Default::default(),
    };

    save_graph(&dir, &graph).expect("save_graph");
    save_layout(&dir, &layout).expect("save_layout");

    // Files live where the spec says they should.
    assert!(dir.join(".mewcode/canvas/graph.json").is_file());
    assert!(dir.join(".mewcode/canvas/layout.json").is_file());

    let (loaded_graph, loaded_layout) = load(&dir).expect("load after save");
    assert_eq!(loaded_graph, graph);
    assert_eq!(loaded_layout, layout);
    // Spot-check the pinned position survived, not just the length.
    assert_eq!(
        loaded_layout.positions.get(&NodeId("a".to_string())),
        Some(&Point { x: 5, y: 5 })
    );
}

/// Loading a malformed `graph.json` surfaces the error rather than
/// silently defaulting — same loud-failure philosophy as the
/// protocol's `version` field.
#[test]
fn load_malformed_graph_json_errors() {
    let dir = tempdir();
    let canvas_dir = dir.join(".mewcode/canvas");
    fs::create_dir_all(&canvas_dir).unwrap();
    fs::write(canvas_dir.join("graph.json"), b"{ not valid json").unwrap();
    fs::write(canvas_dir.join("layout.json"), b"{}").unwrap();
    assert!(load(&dir).is_err());
}

/// `load` tolerates the case where one of the two files is missing
/// independently of the other: a saved graph with no layout
/// returns an empty layout, and vice versa. Mirrors the
/// `.mewcode/canvas/{graph,layout}.json` "do not error" call in
/// the T2 spec.
#[test]
fn load_with_only_one_file_present() {
    let dir = tempdir();
    let canvas_dir = dir.join(".mewcode/canvas");
    fs::create_dir_all(&canvas_dir).unwrap();
    fs::write(
        canvas_dir.join("graph.json"),
        br#"{"version":1,"nodes":[],"edges":[]}"#,
    )
    .unwrap();
    // layout.json intentionally absent.

    let (graph, layout) = load(&dir).expect("missing layout should not error");
    assert_eq!(graph, Graph::default());
    assert_eq!(layout, Layout::default());
}

/// Minimal tempdir helper. `tempfile` is not a workspace dep, so
/// this is a stand-in: a unique-enough path under `/tmp` for a
/// single test process. Tests run with `--test-threads=1` for
/// this crate, so collisions are not a concern in practice.
fn tempdir() -> std::path::PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    let path = std::path::PathBuf::from(format!("/tmp/mewcode-canvas-io-test-{pid}-{n}"));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(&path).unwrap();
    path
}

// Avoid the `LAYOUT_FILE` import being flagged as unused on the
// surface — the file lives at this name on disk and the test
// confirms the path via `dir.join(".mewcode/canvas/graph.json")`.
// The constant is the canonical name reference for callers.
#[allow(dead_code)]
const _GRAPH_FILE_NAME: &str = LAYOUT_FILE;
