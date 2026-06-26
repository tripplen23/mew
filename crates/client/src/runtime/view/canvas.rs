//! Read-only render of the architecture canvas: node cards at
//! resolved positions, edges as routed lines, plus a status row.
//!
//! Visual scope is intentionally minimal for the T4 milestone:
//! rounded cards, C4 color-coded title bars, and arrowhead
//! connectors are deferred to a follow-up PR. The render here
//! matches the T4 spec acceptance ("with a hand-written 3-node
//! `graph.json`, launching the canvas renders 3 boxes and their
//! edges") without the heavy flourish from `ui-aesthetic.md`.

use std::collections::HashMap;

use mewcode_protocol::canvas::{Graph, Layout, Node, NodeId, Point};
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout as TuLayout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

use super::super::model::CanvasState;

/// Width and height of a single node card, in cell units. Big
/// enough to read a 1-2 word label; the layout engine's
/// `COL_STEP` is tuned to match.
const NODE_W: u16 = 20;
const NODE_H: u16 = 4;

/// Color a node card by C4 kind. Hardcoded palette for T4; T4a
/// will route through `Theme`.
fn color_for_kind(kind: mewcode_protocol::canvas::NodeKind) -> Color {
    use mewcode_protocol::canvas::NodeKind;
    match kind {
        NodeKind::System => Color::Blue,
        NodeKind::Container => Color::Cyan,
        NodeKind::Component => Color::Green,
    }
}

/// Draw the canvas screen: title bar, node cards, edge lines, and
/// a status row at the bottom.
///
/// Takes `&CanvasState` because the render is read-only — the
/// resolved-position grid is rebuilt each frame from
/// `state.layout.positions` and the engine's auto-layout rules.
/// T5 (canvas navigation) will switch this back to `&mut` once
/// hit-testing persists selection state and viewport pan/zoom
/// back into `CanvasState`.
pub(super) fn render_canvas(frame: &mut Frame, area: Rect, c: &CanvasState) {
    let chunks = TuLayout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .split(area);

    render_title(frame, chunks[0], c);
    render_canvas_area(frame, chunks[1], &c.graph, &c.layout, c.loading);
    render_status(frame, chunks[2], c.loading);
}

fn render_title(frame: &mut Frame, area: Rect, c: &CanvasState) {
    let title = if c.loading {
        " canvas — loading "
    } else {
        " canvas "
    };
    let detail = if c.loading {
        Line::from("")
    } else {
        Line::from(Span::styled(
            format!(
                " {} nodes, {} edges ",
                c.graph.nodes.len(),
                c.graph.edges.len()
            ),
            Style::default().fg(Color::DarkGray),
        ))
    };
    let block = Block::default()
        .borders(Borders::TOP | Borders::BOTTOM)
        .title(title);
    frame.render_widget(Paragraph::new(detail).block(block), area);
}

fn render_canvas_area(
    frame: &mut Frame,
    area: Rect,
    graph: &Graph,
    layout: &Layout,
    loading: bool,
) {
    if loading {
        frame.render_widget(
            Paragraph::new("loading canvas from server…")
                .style(Style::default().fg(Color::DarkGray)),
            area,
        );
        return;
    }

    // Apply auto-layout in-place for missing positions. This is
    // the only place the client touches `auto_layout`; the
    // server returns the raw layout.positions, and the view
    // resolves any gaps so hit-testing (T5) has stable
    // positions. The in-house grid matches the engine's
    // behavior (same constants); a future PR can route
    // through `mewcode_engine::canvas::layout::auto_layout`
    // directly once the client gains a dep on the engine.
    let positions = ensure_resolved(graph, &layout.positions);

    for node in &graph.nodes {
        if let Some(&p) = positions.get(&node.id) {
            let rect = Rect {
                x: area.x.saturating_add(p.x.max(0) as u16),
                y: area.y.saturating_add(p.y.max(0) as u16),
                width: NODE_W.min(area.width.saturating_sub(p.x.max(0) as u16)),
                height: NODE_H.min(area.height.saturating_sub(p.y.max(0) as u16)),
            };
            if rect.width < 4 || rect.height < 2 {
                continue;
            }
            render_node(frame, rect, node);
        }
    }

    for edge in &graph.edges {
        if let (Some(&a), Some(&b)) = (positions.get(&edge.from), positions.get(&edge.to)) {
            render_edge(frame, area, a, b);
        }
    }
}

fn render_node(frame: &mut Frame, rect: Rect, node: &Node) {
    let color = color_for_kind(node.kind);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(color))
        .title(Span::styled(
            format!(" {} ", node.name),
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        ));
    frame.render_widget(Paragraph::new("").block(block), rect);
}

/// Minimal edge render: a vertical line at column `a.x` from
/// `a.y` to `b.y`, then a horizontal connector at `b.y` to
/// `b.x` with an arrowhead at the target.
///
/// All `cell_mut` calls use `if let Some(...)` so a node placed
/// off-canvas (e.g. by a hand-written `graph.json` with a
/// huge `Point`) does not panic the render. The vertical and
/// horizontal loops each check `y` and `x` against the area
/// bounds before drawing.
fn render_edge(frame: &mut Frame, area: Rect, a: Point, b: Point) {
    let (left, right) = if a.x <= b.x { (a, b) } else { (b, a) };
    let col = area.x.saturating_add(left.x.max(0) as u16);
    if col >= area.x.saturating_add(area.width) {
        return;
    }
    let y_top = area.y.saturating_add(left.y.max(0) as u16);
    let y_bot = area.y.saturating_add(right.y.max(0) as u16);
    let area_bottom = area.y.saturating_add(area.height);
    let area_right = area.x.saturating_add(area.width).saturating_sub(1);
    for y in y_top.min(y_bot)..=y_top.max(y_bot) {
        if y < area_bottom {
            if let Some(cell) = frame.buffer_mut().cell_mut((col, y)) {
                cell.set_symbol("│");
                cell.set_style(Style::default().fg(Color::DarkGray));
            }
        }
    }
    // The horizontal connector only draws inside the canvas
    // height. `y_bot` may exceed the area when the target
    // node's y is off-screen — clamp to the area in that
    // case so we still draw the line at the bottom edge.
    let y_draw = y_bot.min(area_bottom.saturating_sub(1));
    if y_draw < area.y {
        return;
    }
    let x_end = area.x.saturating_add(right.x.max(0) as u16);
    for x in col..=x_end.min(area_right) {
        if let Some(cell) = frame.buffer_mut().cell_mut((x, y_draw)) {
            let sym = if x == x_end { "▶" } else { "─" };
            cell.set_symbol(sym);
            cell.set_style(Style::default().fg(Color::DarkGray));
        }
    }
}

fn render_status(frame: &mut Frame, area: Rect, loading: bool) {
    let text = if loading {
        "loading…"
    } else {
        "press [esc] to return · click to select (T5)"
    };
    frame.render_widget(
        Paragraph::new(Span::styled(text, Style::default().fg(Color::DarkGray))),
        area,
    );
}

/// Resolved positions: every node has a `Point`. Missing entries
/// are filled by a row-major grid that matches the engine's
/// `auto_layout` behavior (same constants). Keeping a local
/// copy here means the client doesn't need to depend on
/// `mewcode_engine` for view-only code; T5's hit-testing
/// layer can switch to the engine's function once we add
/// the dep.
fn ensure_resolved(graph: &Graph, existing: &HashMap<NodeId, Point>) -> HashMap<NodeId, Point> {
    let mut sorted_ids: Vec<&NodeId> = graph.nodes.iter().map(|n| &n.id).collect();
    sorted_ids.sort_by(|a, b| a.0.cmp(&b.0));

    const COL_STEP: i32 = 24;
    const ROW_STEP: i32 = 6;
    const COLS_PER_ROW: usize = 4;
    let mut resolved: HashMap<NodeId, Point> = existing.clone();
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
