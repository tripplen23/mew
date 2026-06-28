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

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};

use mewcode_client::runtime::model::{App, CanvasData, CanvasState, Msg, Screen};
use mewcode_client::runtime::update::update;
use mewcode_protocol::canvas::{Edge, EdgeKind, Graph, Layout, Node, NodeId, NodeKind, Point};

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

// ---------------------------------------------------------------------------
// T5: navigation (mouse + keyboard).
// ---------------------------------------------------------------------------
//
// T5's spec calls for unit tests on the pure helpers
// `hit_test` and `nearest_in_direction`. Per the project's
// "small public API" preference, those helpers stay
// `pub(crate)`; the tests below drive them through the
// public `update` entry point and assert on the resulting
// `CanvasState`. This is integration-level testing of
// the behaviour, not direct unit testing of the helpers.

/// Clicking inside a node's rect selects it.
#[test]
fn mouse_left_click_inside_node_selects_it() {
    let mut app = App::new();
    let mut s = CanvasState::loading();
    s.graph = three_node_graph();
    s.layout = empty_layout();
    s.loading = false;
    app.screen = Screen::Canvas(s);

    // Node "a" sits at the resolver's (0, 0). Its rect spans
    // (0, 0) to (NODE_W, NODE_H). Hit-test at the centre.
    let col = CanvasState::NODE_W as u16 / 2;
    let row = CanvasState::NODE_H as u16 / 2;
    let mouse = MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: col,
        row,
        modifiers: KeyModifiers::empty(),
    };
    update(&mut app, Msg::Mouse(mouse));

    match &app.screen {
        Screen::Canvas(c) => {
            assert_eq!(c.selected.as_ref().map(|n| n.as_str()), Some("a"));
        }
        other => panic!("expected Screen::Canvas, got {other:?}"),
    }
}

#[test]
fn mouse_left_click_outside_node_keeps_selection() {
    // The T5 spec only specifies click-to-select on hit;
    // it does not specify click-to-deselect on miss. Today
    // the behaviour is "keep the previous selection" — the
    // user keeps their selected node until they click on
    // a different one. M2's properties panel can change
    // this if "click empty to deselect" turns out to be
    // a better UX. The test pins the current behaviour
    // so a future change is intentional.
    let mut app = App::new();
    let mut s = CanvasState::loading();
    s.graph = three_node_graph();
    s.layout = empty_layout();
    s.loading = false;
    s.selected = Some(NodeId("a".into()));
    app.screen = Screen::Canvas(s);

    let mouse = MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: 200, // well outside any node rect
        row: 200,
        modifiers: KeyModifiers::empty(),
    };
    update(&mut app, Msg::Mouse(mouse));

    match &app.screen {
        Screen::Canvas(c) => {
            assert!(
                c.selected.is_some(),
                "click on empty canvas should keep the existing selection"
            );
            assert_eq!(c.selected.as_ref().map(|n| n.as_str()), Some("a"));
        }
        other => panic!("expected Screen::Canvas, got {other:?}"),
    }
}

/// Arrow keys move the selection. Down from "a" → "b" when
/// the graph has multi-row layout (a at (0,0), b at (24,6),
/// c at (48,12)). Without explicit layout positions, the
/// 3-node graph fits in a single row and there's nothing
/// below `a` to move to.
#[test]
fn arrow_keys_move_selection_down() {
    let mut app = App::new();
    let mut s = CanvasState::loading();
    s.graph = three_node_graph();
    // Multi-row layout: nodes are spread across two rows so
    // arrow-Down has a candidate below `a`.
    s.layout = Layout {
        version: 1,
        positions: [
            (NodeId("a".into()), Point { x: 0, y: 0 }),
            (NodeId("b".into()), Point { x: 24, y: 6 }),
            (NodeId("c".into()), Point { x: 48, y: 12 }),
        ]
        .into_iter()
        .collect(),
        theme: Default::default(),
    };
    s.loading = false;
    s.selected = Some(NodeId("a".into()));
    app.screen = Screen::Canvas(s);

    update(&mut app, Msg::Key(key(KeyCode::Down)));

    match &app.screen {
        Screen::Canvas(c) => {
            assert_eq!(c.selected.as_ref().map(|n| n.as_str()), Some("b"));
        }
        other => panic!("expected Screen::Canvas, got {other:?}"),
    }
}

/// Scroll wheel pans the viewport.
#[test]
fn scroll_wheel_pans_viewport() {
    let mut app = App::new();
    let mut s = CanvasState::loading();
    s.graph = three_node_graph();
    s.layout = empty_layout();
    s.loading = false;
    app.screen = Screen::Canvas(s);

    let mouse = MouseEvent {
        kind: MouseEventKind::ScrollDown,
        column: 0,
        row: 0,
        modifiers: KeyModifiers::empty(),
    };
    update(&mut app, Msg::Mouse(mouse));

    match &app.screen {
        Screen::Canvas(c) => {
            assert!(c.viewport.1 < 0, "ScrollDown should pan down (negative y)");
        }
        other => panic!("expected Screen::Canvas, got {other:?}"),
    }
}

/// Viewport persists across frames. After multiple scroll
/// events, the offset accumulates.
#[test]
fn scroll_wheel_accumulates_viewport() {
    let mut app = App::new();
    let mut s = CanvasState::loading();
    s.graph = three_node_graph();
    s.layout = empty_layout();
    s.loading = false;
    app.screen = Screen::Canvas(s);

    let mouse = MouseEvent {
        kind: MouseEventKind::ScrollDown,
        column: 0,
        row: 0,
        modifiers: KeyModifiers::empty(),
    };
    update(&mut app, Msg::Mouse(mouse));
    update(&mut app, Msg::Mouse(mouse));
    update(&mut app, Msg::Mouse(mouse));

    match &app.screen {
        Screen::Canvas(c) => {
            // 3 scroll events × -SCROLL_PAN (3) = -9
            assert_eq!(c.viewport.1, -9);
        }
        other => panic!("expected Screen::Canvas, got {other:?}"),
    }
}

/// A burst of mouse events (click + several drags + a scroll)
/// must not mutate `graph` or `layout`. The user reported
/// "everything gone after a click/drag" — this test pins the
/// invariant that the canvas data is preserved across the full
/// T5 mouse vocabulary. If this test ever fails, the `Msg::Mouse`
/// arm in `update/mod.rs` has been changed to mutate the wrong
/// field.
#[test]
fn click_and_drag_burst_preserves_graph_and_layout() {
    let mut app = App::new();
    let mut s = CanvasState::loading();
    s.graph = three_node_graph();
    s.layout = empty_layout();
    s.loading = false;
    s.selected = Some(NodeId("a".into()));
    app.screen = Screen::Canvas(s);

    // Snapshot the canvas data before the burst.
    let (graph_before, layout_before, viewport_before) = match &app.screen {
        Screen::Canvas(c) => (c.graph.clone(), c.layout.clone(), c.viewport),
        _ => unreachable!(),
    };

    // A realistic user burst: click inside node "a", then
    // drag a few times, then scroll. Each event is the kind
    // crossterm actually delivers in a real terminal.
    let events = [
        MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 1,
            row: 1,
            modifiers: KeyModifiers::empty(),
        },
        MouseEvent {
            kind: MouseEventKind::Drag(MouseButton::Left),
            column: 1,
            row: 1,
            modifiers: KeyModifiers::empty(),
        },
        MouseEvent {
            kind: MouseEventKind::Drag(MouseButton::Left),
            column: 2,
            row: 1,
            modifiers: KeyModifiers::empty(),
        },
        MouseEvent {
            kind: MouseEventKind::ScrollDown,
            column: 0,
            row: 0,
            modifiers: KeyModifiers::empty(),
        },
        MouseEvent {
            kind: MouseEventKind::ScrollRight,
            column: 0,
            row: 0,
            modifiers: KeyModifiers::empty(),
        },
    ];
    for mouse in events {
        update(&mut app, Msg::Mouse(mouse));
    }

    // The graph and layout must be byte-identical to the
    // pre-burst snapshot. The selection and viewport MAY
    // have changed (and the viewport should have — drags
    // and scrolls both pan it). Anything beyond that is a
    // state-loss bug.
    match &app.screen {
        Screen::Canvas(c) => {
            assert_eq!(
                c.graph, graph_before,
                "graph was mutated by mouse events"
            );
            assert_eq!(
                c.layout, layout_before,
                "layout was mutated by mouse events"
            );
            // The selection may or may not have changed
            // (a click on "a" when "a" was already selected
            // is a no-op; the test just checks it's still
            // a valid NodeId from the graph).
            if let Some(sel) = &c.selected {
                assert!(
                    c.graph.nodes.iter().any(|n| &n.id == sel),
                    "selection {:?} is not a node in the graph — \
                     this is the \"everything gone\" symptom",
                    sel
                );
            }
            // Viewport should have moved (2 drags + 1 scroll
            // down + 1 scroll right). Pin a coarse check —
            // any non-zero delta is fine; the goal is just
            // to make sure *some* pan happened and we didn't
            // accidentally swallow the events.
            assert!(
                c.viewport != viewport_before,
                "viewport did not change despite drags + scrolls"
            );
        }
        other => panic!("screen changed unexpectedly: {other:?}"),
    }
}

/// Drag pans the viewport by the *delta* from the down-click,
/// not by a fixed stride per event. T5.1 (post-PR) replaced
/// the fixed-stride drag with delta tracking so a 1-second
/// drag doesn't sweep the canvas off-screen.
#[test]
fn drag_pans_by_delta_not_fixed_stride() {
    let mut app = App::new();
    let mut s = CanvasState::loading();
    s.graph = three_node_graph();
    s.layout = empty_layout();
    s.loading = false;
    app.screen = Screen::Canvas(s);

    // Press at (10, 5).
    update(
        &mut app,
        Msg::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 10,
            row: 5,
            modifiers: KeyModifiers::empty(),
        }),
    );

    // Drag to (15, 8). Delta = (5, 3). The viewport should
    // shift by (5, 3) — the canvas appears to "follow" the
    // cursor.
    update(
        &mut app,
        Msg::Mouse(MouseEvent {
            kind: MouseEventKind::Drag(MouseButton::Left),
            column: 15,
            row: 8,
            modifiers: KeyModifiers::empty(),
        }),
    );

    match &app.screen {
        Screen::Canvas(c) => {
            assert_eq!(
                c.viewport,
                (5, 3),
                "drag should pan by the delta from the down-click"
            );
        }
        other => panic!("expected Screen::Canvas, got {other:?}"),
    }

    // Release. The drag origin is cleared.
    update(
        &mut app,
        Msg::Mouse(MouseEvent {
            kind: MouseEventKind::Up(MouseButton::Left),
            column: 15,
            row: 8,
            modifiers: KeyModifiers::empty(),
        }),
    );
    match &app.screen {
        Screen::Canvas(c) => assert_eq!(c.drag_origin, None),
        other => panic!("expected Screen::Canvas, got {other:?}"),
    }

    // A subsequent drag with no new press must not pan —
    // the origin is None, so the drag is ignored.
    let viewport_after_up = match &app.screen {
        Screen::Canvas(c) => c.viewport,
        _ => unreachable!(),
    };
    update(
        &mut app,
        Msg::Mouse(MouseEvent {
            kind: MouseEventKind::Drag(MouseButton::Left),
            column: 30,
            row: 20,
            modifiers: KeyModifiers::empty(),
        }),
    );
    match &app.screen {
        Screen::Canvas(c) => assert_eq!(
            c.viewport, viewport_after_up,
            "drag without a fresh press must not pan"
        ),
        other => panic!("expected Screen::Canvas, got {other:?}"),
    }
}

/// Arrow keys with no current selection start from origin.
#[test]
fn arrow_keys_from_no_selection_pick_closest_in_direction() {
    let mut app = App::new();
    let mut s = CanvasState::loading();
    s.graph = three_node_graph();
    // Multi-row layout — same as the test above, but with no
    // initial selection. Origin is (0, 0), and `b` is the
    // closest node down-and-right.
    s.layout = Layout {
        version: 1,
        positions: [
            (NodeId("a".into()), Point { x: 0, y: 0 }),
            (NodeId("b".into()), Point { x: 24, y: 6 }),
            (NodeId("c".into()), Point { x: 48, y: 12 }),
        ]
        .into_iter()
        .collect(),
        theme: Default::default(),
    };
    s.loading = false;
    // No selection — origin is (0, 0).
    app.screen = Screen::Canvas(s);

    update(&mut app, Msg::Key(key(KeyCode::Down)));

    match &app.screen {
        Screen::Canvas(c) => {
            // "b" is the closest node down-and-right from (0, 0).
            assert_eq!(c.selected.as_ref().map(|n| n.as_str()), Some("b"));
        }
        other => panic!("expected Screen::Canvas, got {other:?}"),
    }
}

/// Arrow keys with no candidate in the half-plane leave the
/// selection unchanged.
#[test]
fn arrow_keys_with_no_candidate_does_nothing() {
    let mut app = App::new();
    let mut s = CanvasState::loading();
    s.graph = three_node_graph();
    // All three nodes on the top row. Going Up from `a`
    // (which is the leftmost top-row node) has no candidate.
    s.layout = Layout {
        version: 1,
        positions: [
            (NodeId("a".into()), Point { x: 0, y: 0 }),
            (NodeId("b".into()), Point { x: 24, y: 0 }),
            (NodeId("c".into()), Point { x: 48, y: 0 }),
        ]
        .into_iter()
        .collect(),
        theme: Default::default(),
    };
    s.loading = false;
    s.selected = Some(NodeId("a".into()));
    app.screen = Screen::Canvas(s);

    update(&mut app, Msg::Key(key(KeyCode::Up)));

    match &app.screen {
        Screen::Canvas(c) => {
            // No node is "above" a, so selection stays put.
            assert_eq!(c.selected.as_ref().map(|n| n.as_str()), Some("a"));
        }
        other => panic!("expected Screen::Canvas, got {other:?}"),
    }
}
