//! Generic overlay-panel drawing: a centred, bordered panel with an
//! optional list body and viewport reporting. Session overlays compose
//! these primitives with their own row-builders.

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Clear, Paragraph, Wrap};

/// Draw a centred, bordered panel with a `Clear` underneath it.
pub(super) fn render_panel(frame: &mut Frame, area: Rect, title: &str, body: Vec<Line<'static>>) {
    let rect = centered_rect(area, 60, 60);
    frame.render_widget(Clear, rect);
    let block = Block::bordered()
        .title(format!(" {title}  (Esc to close) "))
        .border_style(Style::default().fg(Color::Cyan));
    frame.render_widget(
        Paragraph::new(Text::from(body))
            .wrap(Wrap { trim: false })
            .block(block),
        rect,
    );
}

/// Wrapped row count of one logical line at `width`. Ratatui wraps each
/// [`Line`] independently, so per-line counts sum to the body's height.
pub(super) fn wrapped_line_count(line: &Line<'static>, width: u16) -> usize {
    if width == 0 {
        return 0;
    }
    Paragraph::new(Text::from(vec![line.clone()]))
        .wrap(Wrap { trim: false })
        .line_count(width)
}

/// Like [`render_panel`] but vertically scrolled by `scroll` wrapped rows,
/// for bodies taller than the panel (the choice prompt on small terminals).
pub(super) fn render_scrolled_text_panel(
    frame: &mut Frame,
    area: Rect,
    title: &str,
    body: Vec<Line<'static>>,
    scroll: u16,
) {
    let rect = centered_rect(area, 60, 60);
    frame.render_widget(Clear, rect);
    let block = Block::bordered()
        .title(format!(" {title}  (Esc to close) "))
        .border_style(Style::default().fg(Color::Cyan));
    frame.render_widget(
        Paragraph::new(Text::from(body))
            .wrap(Wrap { trim: false })
            .scroll((scroll, 0))
            .block(block),
        rect,
    );
}

/// Centred rect, matching the size used by [`render_panel`].
/// Exposed so callers that build their own body lines (e.g. to truncate
/// to the inner width) can render into the same rect.
pub(super) fn centered_rect(area: Rect, percent_x: u16, percent_y: u16) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(vertical[1])[1]
}

/// The rect of the list-body rows inside a centred, bordered panel — the
/// `border` cells on all four sides removed. Rows land in this area at
/// render time, so `update` can map a mouse row back to a picker entry.
pub(super) fn panel_content_rect(rect: Rect) -> Rect {
    Rect {
        x: rect.x.saturating_add(1),
        y: rect.y.saturating_add(1),
        width: rect.width.saturating_sub(2),
        height: rect.height.saturating_sub(2),
    }
}

/// Like [`panel_content_rect`] but without the footer row that
/// [`render_scrolled_panel`] reserves for its `cursor/total` indicator.
pub(super) fn panel_list_content_rect(rect: Rect) -> Rect {
    let mut r = panel_content_rect(rect);
    r.height = r.height.saturating_sub(1);
    r
}

/// Scroll-start for a cursor in a `viewport`-row window: returns the
/// smallest `scroll` that keeps `cursor` visible. Test surface only.
#[doc(hidden)]
pub fn scroll_start_for_cursor(cursor: usize, viewport: usize, total_rows: usize) -> usize {
    if viewport == 0 || total_rows <= viewport {
        return 0;
    }

    cursor
        .saturating_add(1)
        .saturating_sub(viewport)
        .min(total_rows.saturating_sub(viewport))
}

/// Like [`render_panel`] but for a list that may exceed the panel
/// height. Truncates `body` to the visible window, pads to the footer
/// row, and renders.
///
/// `viewport_out` = the list's visible row count (inner height minus fixed
/// prefix rows and the footer). The update loop clamps the scroll from this.
#[allow(clippy::too_many_arguments)]
pub(super) fn render_scrolled_panel(
    frame: &mut Frame,
    area: Rect,
    title: &str,
    hint: &str,
    percent_x: u16,
    percent_y: u16,
    prefix: Vec<Line<'static>>,
    body: Vec<Line<'static>>,
    total_rows: usize,
    _scroll: usize,
    cursor: usize,
    footer: Option<String>,
    viewport_out: &mut u16,
) {
    // Same rect callers used to truncate `body` to the inner width, so the
    // drawn panel and the pre-truncated lines always match.
    let rect = centered_rect(area, percent_x, percent_y);
    frame.render_widget(Clear, rect);
    let inner_height = rect.height.saturating_sub(2);
    let visible = inner_height
        .saturating_sub(1)
        .saturating_sub(prefix.len() as u16) as usize;

    let prefix_len = prefix.len();
    let mut lines = prefix;
    lines.extend(body.into_iter().take(visible));
    while lines.len() < prefix_len + visible {
        lines.push(Line::from(""));
    }
    let footer_text = footer.unwrap_or_else(|| {
        if total_rows == 0 {
            " — ".to_string()
        } else {
            format!(" {}/{} ", cursor + 1, total_rows)
        }
    });
    lines.push(Line::from(Span::styled(
        footer_text,
        Style::default().fg(Color::DarkGray),
    )));

    let block = Block::bordered()
        .title(format!(" {title}  ({hint}) "))
        .border_style(Style::default().fg(Color::Cyan));
    // Rows are pre-truncated to inner width; wrapping would re-wrap a
    // truncated tail onto a second line.
    frame.render_widget(Paragraph::new(Text::from(lines)).block(block), rect);

    // Report the *list* viewport, not the raw inner height — the footer
    // row is not part of the list.
    *viewport_out = visible as u16;
}
