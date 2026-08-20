//! Transcript rendering for the session screen.
//!
//! This module owns message-to-lines conversion plus scroll measurement. The
//! view writes the measured scroll bounds back into [`SessionState`] so key
//! handling can clamp PageUp/PageDown without doing layout work.

use ratatui::Frame;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Paragraph, Wrap};

use mewcode_protocol::{MessagePart, Role};

use std::rc::Rc;

use crate::runtime::model::{CachedBlock, CompactionView, SessionState, TranscriptCache, TurnItem};
use crate::runtime::view::entry::render_entry_lines;
use crate::runtime::view::markdown::render_markdown;
use crate::runtime::view::session::render_mentions;
use crate::runtime::view::spinner::spinner_frame;
use crate::runtime::view::theme::Theme;
use crate::runtime::view::tool_card::{
    render_diff, render_tool_call_header, render_tool_result_body, render_tool_result_header,
};
use mewcode_protocol::{ToolCall, ToolDisplay, ToolResult};

/// Wrapped height, in rows, of `lines` at `width`.
///
/// Safe to sum per block: ratatui wraps each [`Line`] independently (pinned
/// by `wrapped_line_counts_are_additive` in `tests/render_perf.rs`), which is
/// what lets the transcript render as a window instead of wrapping the whole
/// history each frame.
fn wrapped_height(lines: &[Line<'static>], width: u16) -> u16 {
    if width == 0 {
        return 0;
    }
    Paragraph::new(Text::from(lines.to_vec()))
        .wrap(Wrap { trim: false })
        .line_count(width)
        .min(u16::MAX as usize) as u16
}

/// Cached, height-measured block for one committed compaction entry.
fn compaction_block(
    cache: &mut TranscriptCache,
    index: usize,
    committed_len: usize,
    width: u16,
    view: &CompactionView,
    theme: Theme,
) -> CachedBlock {
    cache.compaction_lines(index, committed_len, width, || {
        let lines = render_compaction_section(view, theme, width);
        let height = wrapped_height(&lines, width);
        (lines, height)
    })
}

/// Which blocks the viewport touches, given each block's wrapped height.
///
/// Returns `(first, local_scroll, end)`: render `blocks[first..end]` with a
/// scroll offset of `local_scroll` rows. `None` means there is nothing to
/// draw. Pure integer arithmetic — this is the whole point of virtualization,
/// and it is kept free of `Frame`/`Paragraph` so it can be tested directly.
#[doc(hidden)]
pub fn window_bounds(heights: &[u16], scroll: u16, viewport: u16) -> Option<(usize, u16, usize)> {
    if viewport == 0 || heights.is_empty() {
        return None;
    }

    // First block whose rows reach past `scroll` — on equality the viewport
    // starts at the next block, so this one is fully above it.
    let mut skipped = 0u32;
    let mut first = None;
    for (index, height) in heights.iter().enumerate() {
        if skipped + *height as u32 > scroll as u32 {
            first = Some(index);
            break;
        }
        skipped += *height as u32;
    }
    let first = first?;
    debug_assert!(skipped <= scroll as u32, "loop exits before passing scroll");
    // In `[0, height-1]` by the invariant above, so this never truncates.
    let local_scroll = (scroll as u32 - skipped) as u16;

    // Take whole blocks until the viewport is covered; `budget` includes the
    // `local_scroll` rows that `taken` still counts, though they scroll away.
    let budget = local_scroll as u32 + viewport as u32;
    let mut taken = 0u32;
    let mut end = first;
    while end < heights.len() && taken < budget {
        taken += heights[end] as u32;
        end += 1;
    }
    Some((first, local_scroll, end))
}

/// Render the transcript panel and update its scroll bounds.
///
/// Only viewport-touching blocks are materialized. Scroll extent is integer
/// sums of cached heights, so cost is viewport-sized, not session-sized
/// (~30 ms → <1 ms on 800 messages; see `tests/render_perf.rs`).
pub(super) fn render_transcript(
    frame: &mut Frame,
    chunk: Rect,
    s: &mut SessionState,
    theme: Theme,
) {
    // Width 0 zeroes every height and clamps scroll, losing the position.
    if chunk.width == 0 || chunk.height == 0 {
        return;
    }
    let is_entry = s.session.is_none();
    let width = chunk.width;
    // Collect blocks — nothing materialized until the visible window is known.
    let mut blocks: Vec<CachedBlock> = Vec::with_capacity(
        s.session
            .as_ref()
            .map(|session| session.messages.len() + s.compaction.committed.len() + 1)
            .unwrap_or(2),
    );
    if s.session.is_some() {
        // Destructure so `transcript_cache` is &mut while `session`/`compaction`
        // stay &immut — disjoint fields the borrow checker otherwise splits.
        let SessionState {
            session,
            compaction,
            transcript_cache,
            ..
        } = &mut *s;
        if let Some(session) = session.as_mut() {
            transcript_cache.sync_session(session.id);
            let committed_len = compaction.committed.len();

            let mut msg_idx = 0;
            let mut comp_idx = 0;
            while msg_idx < session.messages.len() || comp_idx < committed_len {
                let next_comp = compaction.committed.get(comp_idx);
                let next_msg = session.messages.get(msg_idx);

                let comp_first = next_comp
                    .map(|c| c.after_message_count == msg_idx)
                    .unwrap_or(false);

                if comp_first {
                    if let Some(entry) = next_comp {
                        blocks.push(compaction_block(
                            transcript_cache,
                            comp_idx,
                            committed_len,
                            width,
                            &entry.view,
                            theme,
                        ));
                    }
                    comp_idx += 1;
                } else if let Some(msg) = next_msg {
                    blocks.push(transcript_cache.message_lines(msg.id, width, || {
                        let mut lines = render_message(msg, theme);
                        lines.push(Line::from(""));
                        let height = wrapped_height(&lines, width);
                        (lines, height)
                    }));
                    msg_idx += 1;
                } else {
                    break;
                }
            }
            for (offset, entry) in compaction.committed[comp_idx..].iter().enumerate() {
                blocks.push(compaction_block(
                    transcript_cache,
                    comp_idx + offset,
                    committed_len,
                    width,
                    &entry.view,
                    theme,
                ));
            }
        }
    } else {
        let lines = render_entry_lines(s, theme, chunk);
        let height = wrapped_height(&lines, width);
        blocks.push(CachedBlock {
            lines: Rc::new(lines),
            height,
        });
    }
    // Not cached: the in-flight turn changes every delta/spinner tick, so it
    // is rebuilt per frame — cost bounded by one turn, not session length.
    if let Some(st) = &s.streaming {
        let mut lines: Vec<Line<'static>> = Vec::new();
        lines.push(Line::from(Span::styled(
            format!("{} assistant", spinner_frame(st.started_at.elapsed())),
            Style::default()
                .fg(theme.mew_gold)
                .add_modifier(Modifier::BOLD),
        )));
        for item in &st.items {
            match item {
                TurnItem::Text(text) | TurnItem::Progress(text) => {
                    if !text.is_empty() {
                        lines.extend(render_markdown(text));
                    }
                }
                TurnItem::Tool(view) => {
                    let call = ToolCall {
                        id: view.id.clone(),
                        name: view.name.clone(),
                        input: view.input.clone(),
                    };
                    lines.push(render_tool_call_header(&call));
                    match &view.display {
                        Some(ToolDisplay::Diff(diff)) => lines.extend(render_diff(diff)),
                        Some(ToolDisplay::Todo(_)) => {}
                        None => {
                            if let Some(output) = &view.output {
                                let res = ToolResult {
                                    call_id: view.id.clone(),
                                    name: view.name.clone(),
                                    output: output.clone(),
                                    is_error: false,
                                    display: None,
                                };
                                lines.extend(render_tool_result_body(&res));
                            }
                        }
                    }
                }
                TurnItem::Compaction(view) => {
                    lines.extend(render_compaction_section(view, theme, width));
                }
            }
        }
        let height = wrapped_height(&lines, width);
        blocks.push(CachedBlock {
            lines: Rc::new(lines),
            height,
        });
    }

    // `u16` caps scroll at 65_535 rows (~8k messages); past that `follow`
    // pins bottom and streaming scrolls out of view. Upgrade: widen to `u32`.
    let total: u32 = blocks
        .iter()
        .fold(0u32, |acc, block| acc.saturating_add(block.height as u32));
    let total = total.min(u16::MAX as u32) as u16;

    s.viewport = chunk.height;
    s.max_scroll = total.saturating_sub(chunk.height);
    if is_entry {
        s.scroll = 0;
    } else if s.follow {
        s.scroll = s.max_scroll;
    } else {
        s.scroll = s.scroll.min(s.max_scroll);
    }

    let heights: Vec<u16> = blocks.iter().map(|block| block.height).collect();
    let Some((first, local_scroll, end)) = window_bounds(&heights, s.scroll, chunk.height) else {
        return;
    };

    // Materialize only the blocks the viewport actually touches.
    let rows: usize = heights[first..end].iter().map(|h| *h as usize).sum();
    let mut window: Vec<Line> = Vec::with_capacity(rows);
    for block in &blocks[first..end] {
        window.extend(block.lines.iter().cloned());
    }

    let mut para = Paragraph::new(Text::from(window))
        .style(Style::default().fg(theme.text).bg(theme.ink_bg))
        .wrap(Wrap { trim: false });
    if is_entry {
        para = para.alignment(Alignment::Center);
    }

    frame.render_widget(para.scroll((local_scroll, 0)), chunk);
}

fn render_message(msg: &mewcode_protocol::Message, theme: Theme) -> Vec<Line<'static>> {
    let (label, label_style) = match msg.role {
        Role::User => ("you", Style::default().fg(theme.hot_pink)),
        Role::Assistant => ("assistant", Style::default().fg(theme.mew_gold)),
        Role::Tool => ("tool", Style::default().fg(theme.lavender)),
    };
    let mut out = vec![Line::from(Span::styled(
        label.to_string(),
        label_style.add_modifier(Modifier::BOLD),
    ))];

    let mut last_tool_call: Option<&ToolCall> = None;
    for part in &msg.parts {
        match part {
            MessagePart::Text { text } => {
                last_tool_call = None;
                if msg.role == Role::User {
                    for line_text in text.split('\n') {
                        out.push(Line::from(render_mentions(line_text, theme)));
                    }
                } else {
                    out.extend(render_markdown(text));
                }
            }
            MessagePart::ToolCall(call) => {
                last_tool_call = Some(call);
                out.push(render_tool_call_header(call));
            }
            MessagePart::ToolResult(res) => {
                let paired = last_tool_call.map(|c| c.id == res.call_id).unwrap_or(false);
                last_tool_call = None;
                if !paired {
                    out.push(render_tool_result_header(res));
                }
                // Render-only data (a diff) replaces the JSON summary; other
                // tools keep the existing body.
                match &res.display {
                    Some(ToolDisplay::Diff(diff)) => out.extend(render_diff(diff)),
                    Some(ToolDisplay::Todo(_)) => {}
                    None => out.extend(render_tool_result_body(res)),
                }
            }
            MessagePart::FileMention { path } => {
                last_tool_call = None;
                let color = if path.ends_with('/') {
                    theme.psy_cyan
                } else {
                    theme.mew_gold
                };
                out.push(Line::from(Span::styled(
                    format!("@{path}"),
                    Style::default().fg(color),
                )));
            }
        }
    }
    out
}

fn render_compaction_section(
    view: &CompactionView,
    theme: Theme,
    width: u16,
) -> Vec<Line<'static>> {
    let mut out = Vec::new();
    let label = " Compaction ";
    let total_dashes = (width as usize).saturating_sub(label.len());
    let left_len = total_dashes / 2;
    let right_len = total_dashes - left_len;
    let header = format!("{}{}{}", "─".repeat(left_len), label, "─".repeat(right_len));
    out.push(Line::from(Span::styled(
        header,
        Style::default()
            .fg(theme.psy_cyan)
            .add_modifier(Modifier::BOLD),
    )));
    out.push(Line::from(""));

    let secs = view.thought_duration_ms as f64 / 1000.0;
    out.push(Line::from(Span::styled(
        format!("+ Thought: {secs:.1}s"),
        Style::default().fg(theme.mew_gold),
    )));
    out.push(Line::from(""));

    out.extend(render_markdown(&view.summary));
    out.push(Line::from(""));
    out
}
