//! Read-only render of the architecture canvas: node cards at
//! resolved positions, edges as routed lines, plus a status row.
//!
//! Visual scope is intentionally minimal for the T4 milestone:
//! rounded cards, C4 color-coded title bars, and arrowhead
//! connectors are deferred to a follow-up PR. The render here
//! matches the T4 spec acceptance ("with a hand-written 3-node
//! `graph.json`, launching the canvas renders 3 boxes and their
//! edges") without the heavy flourish from `ui-aesthetic.md`.

use mewcode_protocol::canvas::{Node, NodeKind, Point};
use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Direction, Layout as TuLayout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

use super::super::model::CanvasState;

/// Color a node card by C4 kind. Hardcoded palette for T4; T4a
/// will route through `Theme`.
fn color_for_kind(kind: NodeKind) -> Color {
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
/// `state.layout.positions`, the viewport pan offset is applied
/// here, and the selected node gets a different border colour
/// for visual feedback. T5 added viewport + selection to
/// `CanvasState`; both are read by the renderer and mutated by
/// the update.
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
    if c.loading {
        frame.render_widget(
            Paragraph::new("loading canvas from server…")
                .style(Style::default().fg(Color::DarkGray)),
            chunks[1],
        );
    } else {
        render_canvas_area(frame, chunks[1], c);
    }
    render_status(frame, chunks[2], c);
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

fn render_canvas_area(frame: &mut Frame, area: Rect, c: &CanvasState) {
    if c.graph.nodes.is_empty() {
        // First-run / empty project: tell the user *why* the canvas
        // is blank and *where* to put a graph. The render is otherwise
        // silent, and a user landing on a blank canvas with no hint
        // will assume the screen is broken. The path follows the
        // server's default: `.mewcode/canvas/graph.json` relative to
        // the project root.
        let msg = Paragraph::new(vec![
            Line::from(""),
            Line::from(Span::styled(
                "No graph.json found.",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "Drop a graph into .mewcode/canvas/graph.json and press Esc + c to reload.",
                Style::default().fg(Color::DarkGray),
            )),
        ])
        .alignment(Alignment::Center);
        frame.render_widget(msg, area);
        return;
    }

    // `CanvasState::resolved_positions` is the single source of
    // truth for node positions; the view here just consumes it.
    // The same function is used by hit-testing in
    // `update/canvas.rs`, so the two stay in lockstep — a node
    // visible to the renderer is also clickable.
    let positions = c.resolved_positions();
    let viewport = c.viewport;

    for node in &c.graph.nodes {
        if let Some(&p) = positions.get(&node.id) {
            // Convert graph-coord top-left to view-coord
            // top-left by subtracting the viewport pan offset.
            let (vx, vy) = (p.x - viewport.0, p.y - viewport.1);
            // Skip nodes that have scrolled completely off-canvas.
            if vx + CanvasState::NODE_W <= 0 || vy + CanvasState::NODE_H <= 0 {
                continue;
            }
            if vx >= area.width as i32 || vy >= area.height as i32 {
                continue;
            }
            let rect = Rect {
                x: area.x.saturating_add(vx.max(0) as u16),
                y: area.y.saturating_add(vy.max(0) as u16),
                width: (CanvasState::NODE_W as u16)
                    .min(area.width.saturating_sub(vx.max(0) as u16)),
                height: (CanvasState::NODE_H as u16)
                    .min(area.height.saturating_sub(vy.max(0) as u16)),
            };
            if rect.width < 4 || rect.height < 2 {
                continue;
            }
            let is_selected = c.selected.as_ref().is_some_and(|s| s == &node.id);
            render_node(frame, rect, node, is_selected);
        }
    }

    for edge in &c.graph.edges {
        if let (Some(&a), Some(&b)) = (positions.get(&edge.from), positions.get(&edge.to)) {
            // Edges are drawn in viewport-shifted coords.
            let av = Point {
                x: a.x - viewport.0,
                y: a.y - viewport.1,
            };
            let bv = Point {
                x: b.x - viewport.0,
                y: b.y - viewport.1,
            };
            render_edge(frame, area, av, bv);
        }
    }
}

fn render_node(frame: &mut Frame, rect: Rect, node: &Node, is_selected: bool) {
    let color = color_for_kind(node.kind);
    // Selected nodes get a reverse-video title bar so the
    // selection is unambiguous even at 16-colour terminals.
    // The border stays the same colour as unselected so
    // kind-cues aren't lost when a node is highlighted.
    let title_style = if is_selected {
        Style::default()
            .fg(Color::Black)
            .bg(color)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(color).add_modifier(Modifier::BOLD)
    };
    // The block is single-row: just a title bar with a horizontal
    // rule underneath. T5's spec calls for minimal M1 visuals;
    // the M2 polish (rounded cards, kind badges, descriptions
    // inside the box) is deferred. A single-row label is the
    // simplest readable representation of "this is a node
    // called X" — a 4-row rectangle with three empty body
    // lines reads as a frame with no content, which is what
    // the user reported as "canvas looks bad".
    let rule = "─".repeat(rect.width.saturating_sub(2) as usize);
    let block = Block::default()
        .borders(Borders::TOP | Borders::BOTTOM)
        .border_style(Style::default().fg(color))
        .title(Span::styled(format!(" {} ", node.name), title_style));
    let body = Line::from(Span::styled(rule, Style::default().fg(color)));
    frame.render_widget(Paragraph::new(body).block(block), rect);
}

/// Minimal edge render: a vertical line at column `a.x` from
/// `a.y + NODE_H` (just below the source box) to `b.y + NODE_H`
/// (just below the target box), then a horizontal connector
/// at that row to `b.x` with an arrowhead at the target.
/// Drawing the edge below the boxes (rather than through the
/// top border) keeps both the box and the edge visually
/// distinct — the edge connects the *underside* of the source
/// to the *underside* of the target.
///
/// All `cell_mut` calls use `if let Some(...)` so a node placed
/// off-canvas (e.g. by a hand-written `graph.json` with a
/// huge `Point`) does not panic the render. The vertical and
/// horizontal loops each check `y` and `x` against the area
/// bounds before drawing.
fn render_edge(frame: &mut Frame, area: Rect, a: Point, b: Point) {
    let (left, right) = if a.x <= b.x { (a, b) } else { (b, a) };
    // Draw the edge below the boxes (offset by NODE_H). The
    // `+ 1` adds a 1-cell gap so the line doesn't kiss the
    // bottom border.
    let source_bottom = left.y + CanvasState::NODE_H + 1;
    let target_bottom = right.y + CanvasState::NODE_H + 1;
    let col = area.x.saturating_add(left.x.max(0) as u16);
    if col >= area.x.saturating_add(area.width) {
        return;
    }
    let y_top = area.y.saturating_add(source_bottom.max(0) as u16);
    let y_bot = area.y.saturating_add(target_bottom.max(0) as u16);
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

fn render_status(frame: &mut Frame, area: Rect, c: &CanvasState) {
    let text = if c.loading {
        "loading…".to_string()
    } else if c.selected.is_some() {
        // Show the selected node's id so the user has a
        // concrete reference when M2's properties panel lands.
        format!(
            "esc return · ↑↓←→ move · drag/scroll pan · selected: {}",
            c.selected.as_ref().unwrap().as_str()
        )
    } else {
        "esc return · ↑↓←→ move · drag/scroll pan · click to select".to_string()
    };
    frame.render_widget(
        Paragraph::new(Span::styled(text, Style::default().fg(Color::DarkGray))),
        area,
    );
}
