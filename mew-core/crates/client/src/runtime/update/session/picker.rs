//! Key handling for picker overlays in the session screen.
//!
//! Model and session pickers share cursor movement and viewport clamping, but
//! differ in what Enter does: model picks patch or seed a model, session picks
//! open/delete saved sessions.

use crossterm::event::{KeyCode, KeyEvent};
use tui_textarea::{CursorMove, TextArea};

use mewcode_protocol::ModelId;

use crate::net::SessionPatch;

use crate::runtime::model::{Cmd, Overlay, PickerState, SessionState};
use crate::runtime::update::key_to_input;

/// Handle navigation and selection inside the model picker overlay.
pub(super) fn on_model_picker_key(s: &mut SessionState, key: KeyEvent) -> Cmd {
    match key.code {
        KeyCode::Up => cursor_move(s, -1),
        KeyCode::Down => cursor_move(s, 1),
        KeyCode::Enter => pick_model(s),
        _ => Cmd::None,
    }
}

/// Handle navigation, open, and delete inside the session list overlay.
pub(super) fn on_session_list_key(s: &mut SessionState, key: KeyEvent) -> Cmd {
    match key.code {
        KeyCode::Up => cursor_move(s, -1),
        KeyCode::Down => cursor_move(s, 1),
        KeyCode::Enter => s
            .session_list
            .summaries
            .get(s.session_list.picker.cursor)
            .map(|summary| Cmd::OpenSession(summary.id))
            .unwrap_or(Cmd::None),
        KeyCode::Char('d') | KeyCode::Char('D') => s
            .session_list
            .summaries
            .get(s.session_list.picker.cursor)
            .map(|summary| Cmd::DeleteSession(summary.id))
            .unwrap_or(Cmd::None),
        _ => Cmd::None,
    }
}

pub(super) fn on_file_picker_key(s: &mut SessionState, key: KeyEvent) -> Cmd {
    match key.code {
        KeyCode::Up => cursor_move(s, -1),
        KeyCode::Down => cursor_move(s, 1),
        KeyCode::Enter => {
            pick_file(s);
            Cmd::None
        }
        _ => {
            s.composer.input(key_to_input(key));
            refresh_file_picker(s)
        }
    }
}

pub(super) fn on_skills_picker_key(s: &mut SessionState, key: KeyEvent) -> Cmd {
    match key.code {
        KeyCode::Up => cursor_move(s, -1),
        KeyCode::Down => cursor_move(s, 1),
        KeyCode::PageUp => cursor_move(s, -(s.skills_picker.viewport.max(1) as i32)),
        KeyCode::PageDown => cursor_move(s, s.skills_picker.viewport.max(1) as i32),
        KeyCode::Enter => {
            pick_skill(s);
            Cmd::None
        }
        _ => Cmd::None,
    }
}

pub(super) fn open_file_picker(s: &mut SessionState) -> Cmd {
    s.overlay = Overlay::FilePicker;
    s.file_picker.picker.cursor = 0;
    if s.file_picker.files.is_none() {
        Cmd::FetchFiles
    } else {
        Cmd::None
    }
}

pub(super) fn refresh_file_picker(s: &mut SessionState) -> Cmd {
    if s.current_file_query().is_none() {
        s.overlay = Overlay::None;
        return Cmd::None;
    }
    s.overlay = Overlay::FilePicker;
    if s.file_picker.files.is_none() {
        return Cmd::FetchFiles;
    }
    let len = s.filtered_files().len();
    clamp_picker_cursor(&mut s.file_picker.picker, len);
    clamp_file_picker_scroll(s);
    Cmd::None
}

fn move_picker_cursor(picker: &mut PickerState, len: usize, delta: i32) {
    if len == 0 {
        return;
    }
    let max = (len - 1) as i32;
    picker.cursor = (picker.cursor as i32 + delta).clamp(0, max) as usize;
}

fn clamp_picker_cursor(picker: &mut PickerState, len: usize) {
    if picker.cursor >= len {
        picker.cursor = len.saturating_sub(1);
    }
}

fn pick_model(s: &mut SessionState) -> Cmd {
    let Some(entries) = s.model_picker.models.as_ref() else {
        return Cmd::None;
    };
    let Some(entry) = entries.get(s.model_picker.picker.cursor) else {
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
    s.creation.pending_model = Some(model);
    s.overlay = Overlay::None;
    Cmd::None
}

fn cursor_move(s: &mut SessionState, delta: i32) -> Cmd {
    match s.overlay {
        Overlay::ModelPicker => {
            let Some(models) = s.model_picker.models.as_ref() else {
                return Cmd::None;
            };
            move_picker_cursor(&mut s.model_picker.picker, models.len(), delta);
            let cursor_row = model_cursor_row(models, s.model_picker.picker.cursor);
            let header_row = model_header_row(models, s.model_picker.picker.cursor);
            let len = model_visual_len(models);
            s.model_picker.picker.scroll = clamp_picker_scroll(
                s.model_picker.picker.scroll,
                cursor_row,
                len,
                s.model_picker
                    .picker
                    .viewport
                    .max(s.model_picker.picker.viewport_max) as usize,
            );
            s.model_picker.picker.scroll = prefer_visible_header(
                s.model_picker.picker.scroll,
                header_row,
                cursor_row,
                s.model_picker
                    .picker
                    .viewport
                    .max(s.model_picker.picker.viewport_max) as usize,
            );
            Cmd::None
        }
        Overlay::SessionList => {
            move_picker_cursor(
                &mut s.session_list.picker,
                s.session_list.summaries.len(),
                delta,
            );
            s.session_list.picker.scroll = clamp_picker_scroll(
                s.session_list.picker.scroll,
                s.session_list.picker.cursor,
                s.session_list.summaries.len(),
                s.session_list
                    .picker
                    .viewport
                    .max(s.session_list.picker.viewport_max) as usize,
            );
            Cmd::None
        }
        Overlay::FilePicker => {
            file_cursor_move(s, delta);
            Cmd::None
        }
        Overlay::Skills => {
            let len = s.skills.as_ref().map(Vec::len).unwrap_or(0);
            move_picker_cursor(&mut s.skills_picker, len, delta);
            s.skills_picker.scroll = clamp_picker_scroll(
                s.skills_picker.scroll,
                s.skills_picker.cursor,
                len,
                s.skills_picker.viewport.max(1) as usize,
            );
            Cmd::None
        }
        _ => Cmd::None,
    }
}

fn file_cursor_move(s: &mut SessionState, delta: i32) {
    let len = s.filtered_files().len();
    move_picker_cursor(&mut s.file_picker.picker, len, delta);
    clamp_file_picker_scroll(s);
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
pub(crate) fn clamp_model_picker_scroll(s: &mut SessionState) {
    let (len, cursor) = s
        .model_picker
        .models
        .as_ref()
        .map(|models| {
            (
                model_visual_len(models),
                model_cursor_row(models, s.model_picker.picker.cursor),
            )
        })
        .unwrap_or((0, 0));
    let viewport = s
        .model_picker
        .picker
        .viewport
        .max(s.model_picker.picker.viewport_max) as usize;
    s.model_picker.picker.scroll =
        clamp_picker_scroll(s.model_picker.picker.scroll, cursor, len, viewport);
    if let Some(models) = s.model_picker.models.as_ref() {
        s.model_picker.picker.scroll = prefer_visible_header(
            s.model_picker.picker.scroll,
            model_header_row(models, s.model_picker.picker.cursor),
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
pub(crate) fn clamp_session_list_scroll(s: &mut SessionState) {
    let viewport = s
        .session_list
        .picker
        .viewport
        .max(s.session_list.picker.viewport_max) as usize;
    s.session_list.picker.scroll = clamp_picker_scroll(
        s.session_list.picker.scroll,
        s.session_list.picker.cursor,
        s.session_list.summaries.len(),
        viewport,
    );
}

pub(crate) fn clamp_file_picker_scroll(s: &mut SessionState) {
    let viewport = s
        .file_picker
        .picker
        .viewport
        .max(s.file_picker.picker.viewport_max) as usize;
    s.file_picker.picker.scroll = clamp_picker_scroll(
        s.file_picker.picker.scroll,
        s.file_picker.picker.cursor,
        s.filtered_files().len(),
        viewport,
    );
}

pub(crate) fn clamp_skills_picker_scroll(s: &mut SessionState) {
    let len = s.skills.as_ref().map(Vec::len).unwrap_or(0);
    clamp_picker_cursor(&mut s.skills_picker, len);
    s.skills_picker.scroll = clamp_picker_scroll(
        s.skills_picker.scroll,
        s.skills_picker.cursor,
        len,
        s.skills_picker.viewport.max(1) as usize,
    );
}

fn pick_file(s: &mut SessionState) {
    let Some((path, is_dir)) = s
        .filtered_files()
        .get(s.file_picker.picker.cursor)
        .map(|file| (file.path.clone(), file.is_dir))
    else {
        return;
    };
    let token = SessionState::file_mention_token(&path, is_dir);
    replace_current_file_token(s, &token);
    if is_dir {
        refresh_file_picker(s);
    } else {
        s.overlay = Overlay::None;
    }
}

fn pick_skill(s: &mut SessionState) {
    let Some(name) = s
        .skills
        .as_ref()
        .and_then(|skills| skills.get(s.skills_picker.cursor))
        .map(|skill| skill.name.clone())
    else {
        return;
    };
    s.pasted.clear();
    s.composer.insert_str(format!("/{name} "));
    s.overlay = Overlay::None;
}

fn replace_current_file_token(s: &mut SessionState, replacement: &str) {
    let (row, col) = s.composer.cursor();
    let mut lines = s.composer.lines().to_vec();
    let Some(line) = lines.get_mut(row) else {
        return;
    };
    let mut chars: Vec<char> = line.chars().collect();
    let start = chars[..col]
        .iter()
        .rposition(|c| c.is_whitespace())
        .map_or(0, |i| i + 1);
    chars.splice(start..col, replacement.chars());
    *line = chars.into_iter().collect();
    s.composer = TextArea::new(lines);
    s.composer.move_cursor(CursorMove::Jump(
        row as u16,
        (start + replacement.chars().count()) as u16,
    ));
}
