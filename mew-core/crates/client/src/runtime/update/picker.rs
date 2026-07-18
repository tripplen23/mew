//! Key handling for picker overlays in the session screen.
//!
//! Model and session pickers share cursor movement and viewport clamping, but
//! differ in what Enter does: model picks patch or seed a model, session picks
//! open/delete saved sessions.

use crossterm::event::{KeyCode, KeyEvent};

use mewcode_protocol::ModelId;

use crate::net::SessionPatch;

use super::super::model::{Cmd, Overlay, SessionState};

/// Handle navigation and selection inside the model picker overlay.
pub(super) fn on_model_picker_key(s: &mut SessionState, key: KeyEvent) -> Cmd {
    match key.code {
        KeyCode::Up => {
            cursor_move(s, -1);
            Cmd::None
        }
        KeyCode::Down => {
            cursor_move(s, 1);
            Cmd::None
        }
        KeyCode::Enter => pick_model(s),
        _ => Cmd::None,
    }
}

/// Handle navigation, open, and delete inside the session list overlay.
pub(super) fn on_session_list_key(s: &mut SessionState, key: KeyEvent) -> Cmd {
    match key.code {
        KeyCode::Up => {
            cursor_move(s, -1);
            Cmd::None
        }
        KeyCode::Down => {
            cursor_move(s, 1);
            Cmd::None
        }
        KeyCode::Enter => s
            .session_list
            .summaries
            .get(s.session_list.cursor)
            .map(|summary| Cmd::OpenSession(summary.id))
            .unwrap_or(Cmd::None),
        KeyCode::Char('d') | KeyCode::Char('D') => s
            .session_list
            .summaries
            .get(s.session_list.cursor)
            .map(|summary| Cmd::DeleteSession(summary.id))
            .unwrap_or(Cmd::None),
        _ => Cmd::None,
    }
}

fn pick_model(s: &mut SessionState) -> Cmd {
    let Some(entries) = s.model_picker.models.as_ref() else {
        return Cmd::None;
    };
    let Some(entry) = entries.get(s.model_picker.cursor) else {
        return Cmd::None;
    };
    let Ok(model) = entry.id.parse::<ModelId>() else {
        return Cmd::None;
    };
    if let Some(session) = s.session.as_ref() {
        return Cmd::PatchSession {
            id: session.id,
            patch: SessionPatch {
                model: Some(model),
                ..Default::default()
            },
            from_rename: false,
        };
    }
    s.pending_model = Some(model);
    s.overlay = Overlay::None;
    Cmd::None
}

fn cursor_move(s: &mut SessionState, delta: i32) {
    match s.overlay {
        Overlay::ModelPicker => {
            let Some(models) = s.model_picker.models.as_ref() else {
                return;
            };
            if models.is_empty() {
                return;
            }
            let max = (models.len() - 1) as i32;
            s.model_picker.cursor = (s.model_picker.cursor as i32 + delta).clamp(0, max) as usize;
            let cursor_row = model_cursor_row(models, s.model_picker.cursor);
            let header_row = model_header_row(models, s.model_picker.cursor);
            let len = model_visual_len(models);
            s.model_picker.scroll = clamp_picker_scroll(
                s.model_picker.scroll,
                cursor_row,
                len,
                s.model_picker.viewport.max(s.model_picker.viewport_max) as usize,
            );
            s.model_picker.scroll = prefer_visible_header(
                s.model_picker.scroll,
                header_row,
                cursor_row,
                s.model_picker.viewport.max(s.model_picker.viewport_max) as usize,
            );
        }
        Overlay::SessionList => {
            if s.session_list.summaries.is_empty() {
                return;
            }
            let max = (s.session_list.summaries.len() - 1) as i32;
            s.session_list.cursor = (s.session_list.cursor as i32 + delta).clamp(0, max) as usize;
            s.session_list.scroll = clamp_picker_scroll(
                s.session_list.scroll,
                s.session_list.cursor,
                s.session_list.summaries.len(),
                s.session_list.viewport.max(s.session_list.viewport_max) as usize,
            );
        }
        _ => {}
    }
}

fn clamp_picker_scroll(scroll: usize, cursor: usize, len: usize, visible_rows: usize) -> usize {
    if len == 0 || visible_rows == 0 {
        return 0;
    }
    let visible = visible_rows.min(len);
    if cursor < scroll {
        cursor
    } else if cursor >= scroll + visible {
        cursor + 1 - visible
    } else {
        scroll
    }
}

/// Re-clamp model picker scroll after async model data changes.
pub(super) fn clamp_model_picker_scroll(s: &mut SessionState) {
    let (len, cursor) = s
        .model_picker
        .models
        .as_ref()
        .map(|models| {
            (
                model_visual_len(models),
                model_cursor_row(models, s.model_picker.cursor),
            )
        })
        .unwrap_or((0, 0));
    let viewport = s.model_picker.viewport.max(s.model_picker.viewport_max) as usize;
    s.model_picker.scroll = clamp_picker_scroll(s.model_picker.scroll, cursor, len, viewport);
    if let Some(models) = s.model_picker.models.as_ref() {
        s.model_picker.scroll = prefer_visible_header(
            s.model_picker.scroll,
            model_header_row(models, s.model_picker.cursor),
            cursor,
            viewport,
        );
    }
}

fn prefer_visible_header(
    scroll: usize,
    header: usize,
    cursor: usize,
    visible_rows: usize,
) -> usize {
    if visible_rows == 0 || header >= scroll || cursor.saturating_sub(header) >= visible_rows {
        return scroll;
    }
    header
}

fn model_header_row(models: &[crate::net::ModelEntry], cursor: usize) -> usize {
    let mut row = 0;
    let mut prev = None;
    let mut header = 0;
    for (i, model) in models.iter().enumerate() {
        if prev != Some(model.provider) {
            header = row;
            row += 1;
            prev = Some(model.provider);
        }
        if i == cursor {
            return header;
        }
        row += 1;
    }
    header
}

fn model_cursor_row(models: &[crate::net::ModelEntry], cursor: usize) -> usize {
    let mut row = 0;
    let mut prev = None;
    for (i, model) in models.iter().enumerate() {
        if prev != Some(model.provider) {
            row += 1;
            prev = Some(model.provider);
        }
        if i == cursor {
            return row;
        }
        row += 1;
    }
    row.saturating_sub(1)
}

fn model_visual_len(models: &[crate::net::ModelEntry]) -> usize {
    let mut len = 0;
    let mut prev = None;
    for model in models {
        if prev != Some(model.provider) {
            len += 1;
            prev = Some(model.provider);
        }
        len += 1;
    }
    len
}

/// Re-clamp session list scroll after async list data changes.
pub(super) fn clamp_session_list_scroll(s: &mut SessionState) {
    let viewport = s.session_list.viewport.max(s.session_list.viewport_max) as usize;
    s.session_list.scroll = clamp_picker_scroll(
        s.session_list.scroll,
        s.session_list.cursor,
        s.session_list.summaries.len(),
        viewport,
    );
}
