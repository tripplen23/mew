//! The application's state types, grouped by where they live.
//!
//! `App` and `Screen` are the top-level state machine; `Toast` is a
//! cross-screen UI primitive. The remaining structs are split per screen
//! (matching the layout in [`super::super::view`] and [`super::super::update`])
//! so the file you open matches the file you'd change.

use std::collections::HashMap;
use std::time::Instant;

use mewcode_protocol::canvas::{Graph, Layout, NodeId, Point};

mod home;
mod new_session;
mod session;

pub use home::HomeState;
pub use new_session::{ModelPicker, NewSessionField, NewSessionState};
pub use session::{Overlay, SessionState, StreamingState, ToolCallView};

/// The whole application state.
///
/// The current view is held solely as a single [`Screen`] value; there is no
/// screen-specific data outside its variant.
#[derive(Debug)]
pub struct App {
    /// The screen currently being shown, owning its own state.
    pub screen: Screen,
    /// Transient status message, if any.
    pub toast: Option<Toast>,
    /// Set once the user has asked to quit; the event loop checks this.
    pub should_quit: bool,
}

impl App {
    /// Build a fresh app sitting on a loading Home screen.
    pub fn new() -> Self {
        Self {
            screen: Screen::Home(HomeState::loading()),
            toast: None,
            should_quit: false,
        }
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

/// State backing [`super::Screen::Canvas`].
///
/// Holds the loaded graph + layout as-is from the server, plus a
/// per-screen selection / viewport / status. Positions are read
/// from `layout.positions` directly; missing positions are filled
/// by the view layer's `auto_layout` call.
#[derive(Debug, Default)]
pub struct CanvasState {
    /// Semantic graph (source of truth).
    pub graph: Graph,
    /// Presentation overlay (positions + theme).
    pub layout: Layout,
    /// Currently selected node id, if any.
    pub selected: Option<NodeId>,
    /// Pan offset applied to all graph-coord positions before
    /// they are drawn. Positive `x` shifts the canvas right;
    /// positive `y` shifts it down. The view treats a node at
    /// `p` as occupying the rect whose top-left is at
    /// `(p.x - viewport.0, p.y - viewport.1)`. Persisted in
    /// the model so a drag-scroll during one frame carries
    /// through to the next.
    pub viewport: (i32, i32),
    /// `true` while the canvas HTTP fetch is in flight; the view
    /// shows a spinner instead of boxes.
    pub loading: bool,
    /// Last position the user pressed the left mouse button
    /// on the canvas. `None` until the first press. Used by
    /// [`super::super::update::canvas::on_canvas_mouse`] to
    /// compute drag deltas — without this, every drag event
    /// would have to apply a fixed pan stride, which sweeps
    /// the canvas off-screen in milliseconds. Cleared on
    /// `Up(Left)` so the next press starts fresh.
    pub drag_origin: Option<(u16, u16)>,
}

impl CanvasState {
    /// A Canvas screen in its initial loading state, before the
    /// HTTP fetch returns.
    pub fn loading() -> Self {
        Self {
            graph: Graph::default(),
            layout: Layout::default(),
            selected: None,
            viewport: (0, 0),
            loading: true,
            drag_origin: None,
        }
    }

    /// Resolve every node's position, filling in any missing
    /// entries with a row-major grid that matches the engine's
    /// `auto_layout` behavior (same constants). Used by both
    /// the view (for rendering) and the update (for hit-testing
    /// and arrow-direction selection); a single source of
    /// truth keeps the two in lockstep.
    ///
    /// `COL_STEP` / `ROW_STEP` / `COLS_PER_ROW` mirror the
    /// engine's layout module in `mewcode_engine::canvas::layout`.
    /// The client doesn't depend on the engine crate today; if
    /// M2's drag-to-reposition lands, the two will be unified.
    pub fn resolved_positions(&self) -> std::collections::HashMap<NodeId, Point> {
        let mut sorted_ids: Vec<&NodeId> = self.graph.nodes.iter().map(|n| &n.id).collect();
        sorted_ids.sort_by(|a, b| a.0.cmp(&b.0));

        const COL_STEP: i32 = 24;
        const ROW_STEP: i32 = 6;
        const COLS_PER_ROW: usize = 4;
        let mut resolved: HashMap<NodeId, Point> = self.layout.positions.clone();
        for (i, id) in sorted_ids.into_iter().enumerate() {
            if resolved.contains_key(id) {
                continue;
            }
            let col = (i % COLS_PER_ROW) as i32;
            let row = (i / COLS_PER_ROW) as i32;
            resolved.insert(
                id.clone(),
                Point {
                    x: col * COL_STEP,
                    y: row * ROW_STEP,
                },
            );
        }
        resolved
    }

    /// Width and height of a single node card, in cell units.
    /// Public so the view and the hit-tester agree on the box
    /// size — drift between the two is a silent bug.
    ///
    /// `NODE_H = 2` matches the single-row-label render: top
    /// border (with title) and bottom border, one body line
    /// (the horizontal rule). M2 will revisit when the visual
    /// style moves from "label" to "card with content".
    pub const NODE_W: i32 = 20;
    pub const NODE_H: i32 = 2;
}

/// The set of screens the TUI can show. Data lives inside each variant so
/// illegal states (e.g. a session view with no session) are unrepresentable.
#[derive(Debug)]
pub enum Screen {
    /// Session list / launcher.
    Home(HomeState),
    /// New-session creation form.
    NewSession(NewSessionState),
    /// An open chat session.
    Session(SessionState),
    /// Architecture canvas: graph + layout read-only render.
    Canvas(CanvasState),
}

/// Severity of a [`Toast`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToastKind {
    /// Informational message.
    Info,
    /// Error message.
    Error,
}

/// A transient status message shown to the user.
// `Instant` is not `PartialEq`, so this only derives `Debug, Clone`.
#[derive(Debug, Clone)]
pub struct Toast {
    /// Message body.
    pub text: String,
    /// Whether this is an info or error toast.
    pub kind: ToastKind,
    /// When the toast was raised (for the fade-out animation).
    pub started_at: Instant,
}

impl Toast {
    /// Build an error toast.
    pub fn error(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            kind: ToastKind::Error,
            started_at: Instant::now(),
        }
    }

    /// Build an info toast.
    pub fn info(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            kind: ToastKind::Info,
            started_at: Instant::now(),
        }
    }
}
