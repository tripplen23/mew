//! T4 canvas screen: model + key binding + load result handling.
//!
//! Pure tests, no real terminal. Pinned:
//! - Home `'c'` pushes `Screen::Canvas(loading)` and returns
//!   `Cmd::LoadCanvas`.
//! - `Msg::CanvasLoaded(Ok(...))` populates graph + layout and
//!   clears `loading`.
//! - `Msg::CanvasLoaded(Err(...))` clears `loading` and raises
//!   a toast; graph + layout are left untouched.
//! - `Msg::CanvasLoaded` is ignored when the user is no longer
//!   on the canvas (a stale load result must not mutate state).

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use mewcode_client::runtime::model::{App, CanvasData, CanvasState, Msg, Screen};
use mewcode_client::runtime::update::update;
use mewcode_protocol::canvas::{Edge, EdgeKind, Graph, Layout, Node, NodeId, NodeKind};

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::empty())
}

fn three_node_graph() -> Graph {
    Graph {
        version: 1,
        nodes: vec![
            Node {
                id: NodeId("a".into()),
                kind: NodeKind::Component,
                name: "A".into(),
                bind: None,
                contract: vec![],
                tech: None,
                desc: None,
            },
            Node {
                id: NodeId("b".into()),
                kind: NodeKind::Container,
                name: "B".into(),
                bind: None,
                contract: vec![],
                tech: None,
                desc: None,
            },
            Node {
                id: NodeId("c".into()),
                kind: NodeKind::System,
                name: "C".into(),
                bind: None,
                contract: vec![],
                tech: None,
                desc: None,
            },
        ],
        edges: vec![
            Edge {
                from: NodeId("a".into()),
                to: NodeId("b".into()),
                kind: EdgeKind::Depends,
            },
            Edge {
                from: NodeId("b".into()),
                to: NodeId("c".into()),
                kind: EdgeKind::Calls,
            },
        ],
    }
}

fn empty_layout() -> Layout {
    Layout {
        version: 1,
        positions: Default::default(),
        theme: Default::default(),
    }
}

#[test]
fn pressing_c_on_home_enters_canvas_loading() {
    let mut app = App::new();
    // Sanity: starts on Home.
    assert!(matches!(app.screen, Screen::Home(_)));

    let cmd = update(&mut app, Msg::Key(key(KeyCode::Char('c'))));

    assert!(matches!(
        cmd,
        mewcode_client::runtime::model::Cmd::LoadCanvas
    ));
    match &app.screen {
        Screen::Canvas(c) => {
            assert!(c.loading, "fresh canvas should be in loading state");
            assert!(c.graph.nodes.is_empty());
            assert!(c.layout.positions.is_empty());
        }
        other => panic!("expected Screen::Canvas, got {other:?}"),
    }
}

#[test]
fn canvas_loaded_ok_populates_state() {
    let mut app = App::new();
    app.screen = Screen::Canvas(CanvasState::loading());

    let data = CanvasData {
        graph: three_node_graph(),
        layout: empty_layout(),
    };
    update(&mut app, Msg::CanvasLoaded(Ok(data)));

    match &app.screen {
        Screen::Canvas(c) => {
            assert!(!c.loading);
            assert_eq!(c.graph.nodes.len(), 3);
            assert_eq!(c.graph.edges.len(), 2);
            assert!(c.selected.is_none());
        }
        other => panic!("expected Screen::Canvas, got {other:?}"),
    }
    assert!(app.toast.is_none());
}

#[test]
fn canvas_loaded_err_raises_toast_and_keeps_state() {
    let mut app = App::new();
    app.screen = Screen::Canvas(CanvasState::loading());
    let prior_toast = app.toast.clone();
    assert!(prior_toast.is_none());

    update(
        &mut app,
        Msg::CanvasLoaded(Err("server unreachable".into())),
    );

    match &app.screen {
        Screen::Canvas(c) => {
            assert!(!c.loading, "loading flag must clear even on error");
            assert!(c.graph.nodes.is_empty(), "graph untouched on error");
        }
        other => panic!("expected Screen::Canvas, got {other:?}"),
    }
    let toast = app.toast.expect("error toast should be set");
    assert!(toast.text.contains("canvas load failed"));
    assert!(toast.text.contains("server unreachable"));
}

#[test]
fn canvas_loaded_ignored_when_user_left_screen() {
    // User opens canvas, hits `q` to quit (T4 doesn't have a
    // pop-canvas key, but the screen may have been swapped
    // out by some other flow). The stale `CanvasLoaded` must
    // not silently mutate the current screen.
    let mut app = App::new();
    // Sanity: App starts on Home.
    assert!(matches!(app.screen, Screen::Home(_)));
    let was_home_before = matches!(app.screen, Screen::Home(_));
    let toast_was_none_before = app.toast.is_none();

    let data = CanvasData {
        graph: three_node_graph(),
        layout: empty_layout(),
    };
    update(&mut app, Msg::CanvasLoaded(Ok(data)));

    // The current screen is still Home.
    assert!(matches!(app.screen, Screen::Home(_)));
    // And the discriminant didn't change (the only thing we
    // can compare without a `Clone` impl on `Screen`).
    assert_eq!(was_home_before, matches!(app.screen, Screen::Home(_)));
    // No toast raised (the load was for a screen that no
    // longer exists).
    assert_eq!(app.toast.is_none(), toast_was_none_before);
}

/// `Esc` on the canvas screen must take the user back to Home
/// and re-fire the session list load. Without this, a stuck
/// `Cmd::LoadCanvas` would trap the user on a black screen —
/// the CodeRabbit review caught this exact regression.
#[test]
fn esc_on_canvas_returns_to_home_and_refetches_sessions() {
    let mut app = App::new();
    // Put the user on the canvas.
    app.screen = Screen::Canvas(CanvasState::loading());

    let cmd = update(&mut app, Msg::Key(key(KeyCode::Esc)));

    // Screen is now Home, in its initial loading state.
    match &app.screen {
        Screen::Home(h) => {
            assert!(h.loading, "Home should re-enter loading state on Esc");
            assert!(h.sessions.is_empty());
        }
        other => panic!("expected Screen::Home after Esc, got {other:?}"),
    }
    // And the side effect is a session list refetch.
    assert!(matches!(
        cmd,
        mewcode_client::runtime::model::Cmd::LoadSessions
    ));
}
