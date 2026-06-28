//! Canvas-screen snapshot tests: render the 4-node C4 example
//! graph and pin the resulting buffer with `insta`. This locks
//! the canvas's visual contract ("with a hand-written
//! graph.json, launching the canvas renders the boxes and
//! their edges") against accidental regressions.

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use std::collections::HashMap;

use mewcode_client::runtime::model::{App, CanvasState, Screen};
use mewcode_client::runtime::view::render;
use mewcode_protocol::canvas::{Edge, EdgeKind, Graph, Layout, Node, NodeId, NodeKind, Point};

/// Build the same 4-node graph the screenshot demo uses (Client
/// → Server → Engine → OpenCode Go, with OpenCode Go offset to
/// the right so the layout is interesting).
fn four_node_graph() -> Graph {
    Graph {
        version: 1,
        nodes: vec![
            Node {
                id: NodeId("client".into()),
                kind: NodeKind::System,
                name: "Client (TUI)".into(),
                bind: None,
                contract: vec![],
                tech: None,
                desc: Some("Terminal UI".into()),
            },
            Node {
                id: NodeId("server".into()),
                kind: NodeKind::System,
                name: "Server (Axum)".into(),
                bind: None,
                contract: vec![],
                tech: None,
                desc: Some("HTTP service".into()),
            },
            Node {
                id: NodeId("engine".into()),
                kind: NodeKind::System,
                name: "Engine (Rig)".into(),
                bind: None,
                contract: vec![],
                tech: None,
                desc: Some("Agent harness".into()),
            },
            Node {
                id: NodeId("opencode".into()),
                kind: NodeKind::Container,
                name: "OpenCode Go".into(),
                bind: None,
                contract: vec![],
                tech: None,
                desc: Some("LLM router".into()),
            },
        ],
        edges: vec![
            Edge {
                from: NodeId("client".into()),
                to: NodeId("server".into()),
                kind: EdgeKind::Calls,
            },
            Edge {
                from: NodeId("server".into()),
                to: NodeId("engine".into()),
                kind: EdgeKind::Calls,
            },
            Edge {
                from: NodeId("engine".into()),
                to: NodeId("opencode".into()),
                kind: EdgeKind::Depends,
            },
        ],
    }
}

fn four_node_layout() -> Layout {
    // 6-cell row stride: 2 for the box + 1 for the edge gap
    // + 3 for breathing room. Matches the canonical
    // `.mewcode/canvas/layout.json` shipped with the project.
    let mut positions = HashMap::new();
    positions.insert(NodeId("client".into()), Point { x: 0, y: 0 });
    positions.insert(NodeId("server".into()), Point { x: 0, y: 6 });
    positions.insert(NodeId("engine".into()), Point { x: 0, y: 12 });
    positions.insert(NodeId("opencode".into()), Point { x: 28, y: 12 });
    Layout {
        version: 1,
        positions,
        theme: Default::default(),
    }
}

#[test]
fn four_node_canvas_loaded() {
    let mut app = App::new();
    app.screen = Screen::Canvas(CanvasState {
        graph: four_node_graph(),
        layout: four_node_layout(),
        selected: Some(NodeId("server".into())),
        viewport: (0, 0),
        loading: false,
        drag_origin: None,
    });

    // 80×24 is the canonical TUI size — wider than 60 so the
    // "opencode" node at x=28 fits in view.
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
    terminal.draw(|frame| render(frame, &mut app)).unwrap();
    insta::assert_snapshot!(terminal.backend().to_string());
}

#[test]
fn four_node_canvas_panned() {
    // Same graph, panned 10 cells right. The selected node is
    // still "server"; the "client" node scrolls off the left.
    let mut app = App::new();
    app.screen = Screen::Canvas(CanvasState {
        graph: four_node_graph(),
        layout: four_node_layout(),
        selected: Some(NodeId("engine".into())),
        viewport: (10, 0),
        loading: false,
        drag_origin: None,
    });

    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
    terminal.draw(|frame| render(frame, &mut app)).unwrap();
    insta::assert_snapshot!(terminal.backend().to_string());
}
