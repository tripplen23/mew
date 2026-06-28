//! Update arm for [`Screen::Canvas`](super::super::model::Screen::Canvas).
//!
//! T5 adds navigation: click to select, drag to pan, scroll to pan,
//! arrow keys to move selection. All pure — no I/O. The hit-test
//! and nearest-in-direction helpers are pub(super) so the
//! canvas tests in `tests/canvas_screen.rs` can exercise them
//! without going through the full `update` entry point.

use std::collections::HashMap;

use crossterm::event::{KeyCode, KeyEvent, MouseButton, MouseEvent, MouseEventKind};
use mewcode_protocol::canvas::{NodeId, Point};

use super::super::model::{CanvasData, CanvasState, Cmd, HomeState, Screen, Toast};

/// Pan stride (cells) for a single scroll-wheel tick.
const SCROLL_PAN: i32 = 3;

/// Cardinal direction for arrow-key selection. Pure data, no
/// crossterm dependency, so the unit tests don't need an event
/// source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Direction {
    Up,
    Down,
    Left,
    Right,
}

impl Direction {
    /// Map a `KeyCode::Up/Down/Left/Right` to a `Direction`.
    /// Returns `None` for any other key — callers use this to
    /// decide whether to act.
    fn from_key(code: KeyCode) -> Option<Self> {
        match code {
            KeyCode::Up => Some(Direction::Up),
            KeyCode::Down => Some(Direction::Down),
            KeyCode::Left => Some(Direction::Left),
            KeyCode::Right => Some(Direction::Right),
            _ => None,
        }
    }
}

/// Handle a key event when the user is on the Canvas screen.
///
/// T5 spec:
/// - `Esc` returns the user to Home and refetches the session list.
/// - Arrow keys move selection to the nearest node in that
///   direction (purely visual; no I/O).
/// - Other keys are no-ops (T6 will add delete/rename, M2 will
///   add drag-mode keys).
pub(super) fn on_canvas_key(screen: &mut Screen, key: KeyEvent) -> Cmd {
    match key.code {
        KeyCode::Esc => {
            *screen = Screen::Home(HomeState::loading());
            Cmd::LoadSessions
        }
        code => {
            // Arrow keys mutate the selection in place.
            if let Some(dir) = Direction::from_key(code) {
                if let Screen::Canvas(c) = screen {
                    move_selection(c, dir);
                }
            }
            Cmd::None
        }
    }
}

/// Handle a mouse event when the user is on the Canvas screen.
///
/// T5 spec:
/// - `Down(Left)` inside a node's rect → select that node.
/// - `Down(Left)` on empty canvas → record the click position
///   as the drag origin (the next `Drag` event will pan by
///   the delta from here). No pan happens on the press itself.
/// - `Drag(Left)` → pan the viewport by the delta between the
///   drag origin and the current cursor position. The user
///   sees the canvas "stick to" the cursor in the direction
///   they pulled.
/// - `Up(Left)` → clear the drag origin so the next press
///   starts a new pan (or a new select, if they click on a
///   node this time).
/// - `ScrollUp`/`ScrollDown` → pan vertically by `SCROLL_PAN`.
/// - `ScrollLeft`/`ScrollRight` → pan horizontally by `SCROLL_PAN`.
pub(super) fn on_canvas_mouse(c: &mut CanvasState, mouse: MouseEvent) -> Cmd {
    match mouse.kind {
        MouseEventKind::Down(MouseButton::Left) => {
            if let Some(id) = hit_test(c, mouse.column, mouse.row) {
                c.selected = Some(id);
            }
            // Always record the origin so a subsequent drag
            // has a reference point. The hit-test is independent
            // — a press on a node both selects it and starts a
            // pan-with-origin (a real "drag to move the node"
            // is M2; for now, dragging from a node pans the
            // canvas, which is the documented T5 behaviour).
            c.drag_origin = Some((mouse.column, mouse.row));
            Cmd::None
        }
        MouseEventKind::Drag(MouseButton::Left) => {
            // Pan by the delta from the drag origin. This
            // matches the natural "grab and pull" feeling:
            // moving the cursor right shifts the canvas
            // right (the user sees content that was off-screen
            // to the left).
            if let Some((ox, oy)) = c.drag_origin {
                let dx = mouse.column as i32 - ox as i32;
                let dy = mouse.row as i32 - oy as i32;
                c.viewport.0 = c.viewport.0.saturating_add(dx);
                c.viewport.1 = c.viewport.1.saturating_add(dy);
                c.drag_origin = Some((mouse.column, mouse.row));
            }
            Cmd::None
        }
        MouseEventKind::Up(MouseButton::Left) => {
            c.drag_origin = None;
            Cmd::None
        }
        MouseEventKind::ScrollUp => {
            c.viewport.1 = c.viewport.1.saturating_add(SCROLL_PAN);
            Cmd::None
        }
        MouseEventKind::ScrollDown => {
            c.viewport.1 = c.viewport.1.saturating_sub(SCROLL_PAN);
            Cmd::None
        }
        MouseEventKind::ScrollLeft => {
            c.viewport.0 = c.viewport.0.saturating_add(SCROLL_PAN);
            Cmd::None
        }
        MouseEventKind::ScrollRight => {
            c.viewport.0 = c.viewport.0.saturating_sub(SCROLL_PAN);
            Cmd::None
        }
        _ => Cmd::None,
    }
}

/// Hit-test a click at view-coord `(col, row)`. Returns the id
/// of the topmost (last-drawn) node whose rect contains the
/// point, or `None` if the click is on empty canvas.
///
/// Pure: takes `&CanvasState`, returns `Option<NodeId>`. The
/// view computes the same resolved positions (via
/// `CanvasState::resolved_positions`) and the same viewport
/// offset, so the two stay in lockstep.
pub(crate) fn hit_test(c: &CanvasState, col: u16, row: u16) -> Option<NodeId> {
    let positions = c.resolved_positions();
    // Iterate in graph order; later nodes win on overlap (topmost
    // wins in z-order is a common convention).
    for node in &c.graph.nodes {
        if let Some(&p) = positions.get(&node.id) {
            // Convert graph-coord top-left to view-coord top-left.
            let x0 = p.x.saturating_sub(c.viewport.0);
            let y0 = p.y.saturating_sub(c.viewport.1);
            let x1 = x0 + CanvasState::NODE_W;
            let y1 = y0 + CanvasState::NODE_H;
            if (x0 as u16) <= col && col < (x1 as u16) && (y0 as u16) <= row && row < (y1 as u16) {
                return Some(node.id.clone());
            }
        }
    }
    None
}

/// Move the selection to the nearest node in `dir` from the
/// current selection (or from `(0, 0)` if no selection). Pure;
/// mutates `c.selected` in place.
///
/// "Nearest" = primary-axis distance minimized first, secondary-
/// axis distance used as a tiebreaker. A node that's straight
/// up wins over one that's up-and-to-the-side, even if the
/// diagonal node is closer in Euclidean distance.
pub(crate) fn move_selection(c: &mut CanvasState, dir: Direction) {
    let positions = c.resolved_positions();
    let origin = c
        .selected
        .as_ref()
        .and_then(|id| positions.get(id).copied())
        .unwrap_or(Point { x: 0, y: 0 });
    let next = nearest_in_direction(&origin, dir, &positions, c.selected.as_ref());
    if let Some(id) = next {
        c.selected = Some(id);
    }
}

/// Pure nearest-in-direction: find the node in `positions` that
/// is the closest to `origin` in `dir`, with the constraint that
/// the node lies in the half-plane defined by `dir` (e.g. for
/// `Up`, the node must be above `origin`). The current
/// selection (if any) is excluded so arrow keys always move.
pub(crate) fn nearest_in_direction(
    origin: &Point,
    dir: Direction,
    positions: &HashMap<NodeId, Point>,
    exclude: Option<&NodeId>,
) -> Option<NodeId> {
    let mut best: Option<(NodeId, i64)> = None;
    for (id, p) in positions {
        if exclude.is_some_and(|e| e == id) {
            continue;
        }
        let dx = (p.x - origin.x) as i64;
        let dy = (p.y - origin.y) as i64;
        // Reject nodes that are not in the correct half-plane.
        let in_half_plane = match dir {
            Direction::Up => dy < 0,
            Direction::Down => dy > 0,
            Direction::Left => dx < 0,
            Direction::Right => dx > 0,
        };
        if !in_half_plane {
            continue;
        }
        // Primary-axis distance is `|dy|` (or `|dx|`), secondary
        // is `|dx|` (or `|dy|`). Squared distance from origin is
        // a strict-but-noisy tiebreaker; we use a weighted score
        // that prefers pure primary-axis alignment.
        let score = match dir {
            Direction::Up | Direction::Down => dy.abs() * 1000 + dx.abs(),
            Direction::Left | Direction::Right => dx.abs() * 1000 + dy.abs(),
        };
        if let Some((_, best_score)) = &best {
            if score < *best_score {
                best = Some((id.clone(), score));
            }
        } else {
            best = Some((id.clone(), score));
        }
    }
    best.map(|(id, _)| id)
}

/// Apply a finished `Msg::CanvasLoaded` to the model. Populates
/// the graph + layout on success, surfaces a toast on failure,
/// and clears the loading flag in either case. Selection is
/// cleared because the previous selection's node may have been
/// removed; the user re-selects on the new graph.
pub(super) fn apply_canvas_loaded(
    c: &mut CanvasState,
    toast: &mut Option<Toast>,
    result: Result<CanvasData, String>,
) {
    c.loading = false;
    match result {
        Ok(data) => {
            c.graph = data.graph;
            c.layout = data.layout;
            c.selected = None;
        }
        Err(e) => {
            *toast = Some(Toast::error(format!("canvas load failed: {e}")));
        }
    }
}
