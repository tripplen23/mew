//! Overlay rendering on the session screen: dispatch by [`Overlay`] variant
//! to the shared overlay row-builders (`content`), plus cursor parking inside
//! overlays that host a text field (the connect-provider API key input).

use ratatui::Frame;
use ratatui::layout::{Position, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use crate::runtime::model::{ConnectStep, Overlay, SessionState};
use crate::runtime::view::panel::{
    centered_rect, panel_content_rect, panel_list_content_rect, render_panel,
    render_scrolled_panel, render_scrolled_text_panel, wrapped_line_count,
};
use crate::runtime::view::session::overlay::content::{
    choice_lines, connect_provider_key_text, connect_provider_lines, file_picker_lines,
    rename_session_lines, render_slash_picker, skills_lines, theme_lines, tools_lines,
};

mod content;

pub use content::{model_picker_lines, session_list_lines};

/// True while an overlay hosts a text field (the connect-provider API key
/// input), which changes how the composer is rendered and where the cursor
/// parks.
pub(super) fn active_overlay_text_input(s: &SessionState) -> bool {
    s.overlay == Overlay::ModelPicker
        || matches!(
            s.overlay,
            Overlay::ConnectProvider if s.connect_provider.step == ConnectStep::EnterKey
        )
}

pub(super) fn render_active_overlay(frame: &mut Frame, area: Rect, s: &mut SessionState) {
    match s.overlay {
        Overlay::None => {}
        Overlay::Tools => {
            let mode = s.session.as_ref().map(|sess| sess.mode).unwrap_or_default();
            render_panel(frame, area, "Tools", tools_lines(mode))
        }
        Overlay::Skills => {
            let rect = centered_rect(area, 60, 60);
            s.skills_picker.rect = Some(panel_list_content_rect(rect));
            let inner_w = rect.width.saturating_sub(2) as usize;
            let body = skills_lines(s, inner_w);
            render_scrolled_panel(
                frame,
                area,
                "Skills",
                "Enter insert, Esc close",
                60,
                60,
                Vec::new(),
                body,
                s.skills.as_ref().map(Vec::len).unwrap_or(0),
                s.skills_picker.scroll,
                s.skills_picker.cursor,
                None,
                &mut s.skills_picker.viewport,
            );
        }
        Overlay::Theme => render_panel(frame, area, "Theme", theme_lines()),
        Overlay::Choice => {
            // Follow-scroll: keep the cursor's wrapped row inside the panel
            // when the body is taller than a small terminal.
            let rect = centered_rect(area, 60, 60);
            let inner_w = rect.width.saturating_sub(2);
            let inner_h = rect.height.saturating_sub(2) as usize;
            let (body, cursor_line) = choice_lines(s);
            let mut total = 0;
            let mut cursor_start = 0;
            for (i, line) in body.iter().enumerate() {
                if i == cursor_line {
                    cursor_start = total;
                }
                total += wrapped_line_count(line, inner_w);
            }
            let mut scroll = s
                .pending_choice
                .as_ref()
                .map(|c| c.picker.scroll)
                .unwrap_or(0);
            if cursor_start < scroll {
                scroll = cursor_start;
            } else if inner_h > 0 && cursor_start >= scroll + inner_h {
                scroll = cursor_start + 1 - inner_h;
            }
            scroll = scroll.min(total.saturating_sub(inner_h));
            if let Some(choice) = s.pending_choice.as_mut() {
                choice.picker.scroll = scroll;
            }
            render_scrolled_text_panel(frame, area, "Choose", body, scroll as u16);
        }
        Overlay::ConnectProvider => {
            let rect = centered_rect(area, 60, 60);
            if s.connect_provider.step == ConnectStep::PickProvider {
                let inner = panel_content_rect(rect);
                s.connect_provider.picker.rect = Some(Rect {
                    x: inner.x,
                    y: inner.y.saturating_add(1),
                    width: inner.width,
                    height: crate::runtime::model::CONNECT_PROVIDERS.len() as u16,
                });
            } else {
                s.connect_provider.picker.rect = None;
            }
            let body = connect_provider_lines(s);
            let title = match s.connect_provider.step {
                ConnectStep::PickProvider => "Connect Provider",
                ConnectStep::EnterKey => "Enter API Key",
                ConnectStep::Validating => "Validating...",
                ConnectStep::Done => "Connected!",
            };
            render_panel(frame, area, title, body);
            if active_overlay_text_input(s) {
                park_cursor_in_overlay_text_input(frame, area, s);
            }
        }
        Overlay::ModelPicker => {
            let rect = centered_rect(area, 60, 60);
            let mut list_rect = panel_list_content_rect(rect);
            list_rect.y = list_rect.y.saturating_add(1);
            list_rect.height = list_rect.height.saturating_sub(1);
            s.model_picker.picker.rect = Some(list_rect);
            let inner_w = rect.width.saturating_sub(2) as usize;
            s.model_picker.picker.viewport = list_rect.height;
            crate::runtime::update::clamp_model_picker_scroll(s);
            let body = model_picker_lines(s, inner_w);
            let filtered = s.model_picker.filtered_models().len();
            let total = s.model_picker.models.as_ref().map(Vec::len).unwrap_or(0);
            let query_text = s.model_picker.query.lines().join("");
            let query = model_search_layout(
                &query_text,
                s.model_picker.query.cursor().1,
                inner_w.saturating_sub(" Search: ".width()),
            )
            .0;
            let prefix = vec![Line::from(vec![
                Span::styled(" Search: ", Style::default().fg(Color::DarkGray)),
                Span::raw(query),
            ])];
            let footer = if query_text.is_empty() {
                None
            } else if filtered == 0 {
                Some(format!(" 0/{total} "))
            } else {
                Some(format!(
                    " {}/{} · {filtered}/{total} ",
                    s.model_picker.picker.cursor + 1,
                    filtered,
                ))
            };
            render_scrolled_panel(
                frame,
                area,
                "Model",
                "type to search, Esc close",
                60,
                60,
                prefix,
                body,
                filtered,
                s.model_picker.picker.scroll,
                s.model_picker.picker.cursor,
                footer,
                &mut s.model_picker.picker.viewport,
            );
            if s.model_picker.picker.viewport > s.model_picker.picker.viewport_max {
                s.model_picker.picker.viewport_max = s.model_picker.picker.viewport;
            }
            park_model_search_cursor(frame, rect, s);
        }
        Overlay::SessionList => {
            let rect = centered_rect(area, 60, 60);
            s.session_list.picker.rect = Some(panel_list_content_rect(rect));
            let inner_w = rect.width.saturating_sub(2) as usize;
            let body = session_list_lines(s, inner_w);
            render_scrolled_panel(
                frame,
                area,
                "Sessions",
                "Esc close, d delete",
                60,
                60,
                Vec::new(),
                body,
                s.session_list.summaries.len(),
                s.session_list.picker.scroll,
                s.session_list.picker.cursor,
                None,
                &mut s.session_list.picker.viewport,
            );
            if s.session_list.picker.viewport > s.session_list.picker.viewport_max {
                s.session_list.picker.viewport_max = s.session_list.picker.viewport;
            }
        }
        Overlay::FilePicker => {
            const FILE_PICKER_WIDTH_PERCENT: u16 = 70;
            const FILE_PICKER_HEIGHT_PERCENT: u16 = 50;
            let rect = centered_rect(area, FILE_PICKER_WIDTH_PERCENT, FILE_PICKER_HEIGHT_PERCENT);
            s.file_picker.picker.rect = Some(panel_list_content_rect(rect));
            let inner_w = rect.width.saturating_sub(2) as usize;
            let body = file_picker_lines(s, inner_w);
            render_scrolled_panel(
                frame,
                area,
                "Files",
                "Enter insert, Esc close",
                FILE_PICKER_WIDTH_PERCENT,
                FILE_PICKER_HEIGHT_PERCENT,
                Vec::new(),
                body,
                s.filtered_files().len(),
                s.file_picker.picker.scroll,
                s.file_picker.picker.cursor,
                None,
                &mut s.file_picker.picker.viewport,
            );
            if s.file_picker.picker.viewport > s.file_picker.picker.viewport_max {
                s.file_picker.picker.viewport_max = s.file_picker.picker.viewport;
            }
        }
        Overlay::RenameSession => {
            render_panel(frame, area, "Rename session", rename_session_lines(s))
        }
        Overlay::SlashPicker => render_slash_picker(frame, area, s),
    }
}

fn park_cursor_in_overlay_text_input(frame: &mut Frame, area: Rect, s: &SessionState) {
    let rect = centered_rect(area, 60, 60);
    let inner_width = rect.width.saturating_sub(2) as usize;
    if inner_width == 0 {
        return;
    }

    let (left_pad, top_pad) = (2, 4);
    let text = connect_provider_key_text(s);
    let offset = left_pad + UnicodeWidthStr::width(text.as_str());
    let x = rect.x + 1 + (offset % inner_width) as u16;
    let y = rect.y + 1 + top_pad + (offset / inner_width) as u16;
    frame.set_cursor_position(Position::new(
        x.min(rect.x + rect.width.saturating_sub(2)),
        y.min(rect.y + rect.height.saturating_sub(2)),
    ));
}

fn park_model_search_cursor(frame: &mut Frame, rect: Rect, s: &SessionState) {
    let query = s.model_picker.query.lines().join("");
    let available = (rect.width.saturating_sub(2) as usize).saturating_sub(" Search: ".width());
    let (_, cursor_width) = model_search_layout(&query, s.model_picker.query.cursor().1, available);
    let x = rect
        .x
        .saturating_add(1)
        .saturating_add(" Search: ".width() as u16)
        .saturating_add(cursor_width as u16)
        .min(rect.x.saturating_add(rect.width.saturating_sub(2)));
    frame.set_cursor_position(Position::new(x, rect.y.saturating_add(1)));
}

fn model_search_layout(query: &str, cursor: usize, width: usize) -> (String, usize) {
    let graphemes: Vec<&str> = query.graphemes(true).collect();
    let cursor_byte = query
        .char_indices()
        .nth(cursor)
        .map_or(query.len(), |(byte, _)| byte);
    let cursor = graphemes
        .iter()
        .scan(0, |byte, grapheme| {
            let start = *byte;
            *byte += grapheme.len();
            Some(start)
        })
        .take_while(|start| *start < cursor_byte)
        .count();
    let mut start = cursor;
    let mut before_width = 0;
    while start > 0 {
        let grapheme_width = graphemes[start - 1].width();
        if before_width + grapheme_width >= width {
            break;
        }
        before_width += grapheme_width;
        start -= 1;
    }
    let mut visible = String::new();
    let mut used = 0;
    for grapheme in &graphemes[start..] {
        let grapheme_width = grapheme.width();
        if used + grapheme_width > width {
            break;
        }
        visible.push_str(grapheme);
        used += grapheme_width;
    }
    (visible, before_width)
}
