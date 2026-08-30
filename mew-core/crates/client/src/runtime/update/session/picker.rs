//! Key handling for picker overlays in the session screen.
//!
//! Model and session pickers share cursor movement and viewport clamping, but
//! differ in what Enter does: model picks patch or seed a model, session picks
//! open/delete saved sessions.

use crossterm::event::{KeyCode, KeyEvent, MouseButton, MouseEvent, MouseEventKind};
use tui_textarea::{CursorMove, TextArea};

use ratatui::layout::Rect;

use crate::net::SessionPatch;

use crate::runtime::model::{
    CONNECT_PROVIDERS, Cmd, Overlay, PickerState, SLASH_COMMANDS, SessionState,
};
use crate::runtime::update::key_to_input;

/// Handle navigation and selection inside the model picker overlay.
pub(super) fn on_model_picker_key(s: &mut SessionState, key: KeyEvent) -> Cmd {
    match key.code {
        KeyCode::Up => cursor_move(s, -1),
        KeyCode::Down => cursor_move(s, 1),
        KeyCode::Enter => pick_model(s),
        _ => {
            let before = s.model_picker.query.lines().to_vec();
            s.model_picker.query.input(key_to_input(key));
            if s.model_picker.query.lines() != before {
                s.model_picker.picker.cursor = 0;
                s.model_picker.picker.scroll = 0;
            }
            Cmd::None
        }
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

/// Outcome of routing a mouse event through the active picker.
pub(super) enum PickerMouseResult {
    /// Pointer was outside the picker or the event was not actionable.
    Ignored,
    /// Picker consumed the event without activating a row.
    Consumed,
    /// A selectable row was clicked and should run the overlay's Enter action.
    Activate,
}

/// Mouse handling for the active scrollable picker overlay. Wheel scrolls a
/// row per tick; a left click selects and immediately activates that row.
/// Events outside the content rect remain available to the session screen.
pub(super) fn on_picker_mouse(s: &mut SessionState, event: MouseEvent) -> PickerMouseResult {
    let Some(rect) = picker_rect(s) else {
        return PickerMouseResult::Ignored;
    };
    if event.column < rect.x
        || event.column >= rect.x + rect.width
        || event.row < rect.y
        || event.row >= rect.y + rect.height
    {
        return PickerMouseResult::Ignored;
    }
    match event.kind {
        MouseEventKind::ScrollUp => {
            cursor_move(s, -1);
            PickerMouseResult::Consumed
        }
        MouseEventKind::ScrollDown => {
            cursor_move(s, 1);
            PickerMouseResult::Consumed
        }
        MouseEventKind::Down(MouseButton::Left) => {
            if click_picker_row(s, rect, event.row) {
                PickerMouseResult::Activate
            } else {
                PickerMouseResult::Consumed
            }
        }
        _ => PickerMouseResult::Ignored,
    }
}

// Content rect of the active scrollable picker overlay, if any.
fn picker_rect(s: &SessionState) -> Option<Rect> {
    match s.overlay {
        Overlay::ModelPicker => s.model_picker.picker.rect,
        Overlay::SessionList => s.session_list.picker.rect,
        Overlay::Skills => s.skills_picker.rect,
        Overlay::FilePicker => s.file_picker.picker.rect,
        Overlay::SlashPicker => s.slash_picker_geometry.map(|(rect, _)| rect),
        Overlay::ConnectProvider
            if s.connect_provider.step == crate::runtime::model::ConnectStep::PickProvider =>
        {
            s.connect_provider.picker.rect
        }
        _ => None,
    }
}

// Map a clicked screen row to an entry index. Returns true only when the row
// is selectable, so provider headers and empty rows never activate.
fn click_picker_row(s: &mut SessionState, rect: Rect, row: u16) -> bool {
    let local = (row.saturating_sub(rect.y)) as usize;
    match s.overlay {
        Overlay::SessionList => {
            let len = s.session_list.summaries.len();
            if local >= rect.height as usize || len == 0 {
                return false;
            }
            let index = s.session_list.picker.scroll + local;
            if index >= len {
                return false;
            }
            s.session_list.picker.cursor = index;
            clamp_session_list_scroll(s);
            true
        }
        Overlay::Skills => {
            let len = s.skills.as_ref().map(Vec::len).unwrap_or(0);
            let index = s.skills_picker.scroll + local;
            if index >= len {
                return false;
            }
            s.skills_picker.cursor = index;
            clamp_skills_picker_scroll(s);
            true
        }
        Overlay::FilePicker => {
            let len = s.filtered_files().len();
            let index = s.file_picker.picker.scroll + local;
            if index >= len {
                return false;
            }
            s.file_picker.picker.cursor = index;
            clamp_file_picker_scroll(s);
            true
        }
        // Model rows include provider headers; only model rows map to entries.
        Overlay::ModelPicker => {
            let models = s.model_picker.filtered_models();
            let visual_row = s.model_picker.picker.scroll + local;
            let Some(index) = model_entry_at_row(&models, visual_row) else {
                return false;
            };
            s.model_picker.picker.cursor = index;
            clamp_model_picker_scroll(s);
            true
        }
        Overlay::SlashPicker => {
            let Some((_, start)) = s.slash_picker_geometry else {
                return false;
            };
            let index = start + local;
            if index >= SLASH_COMMANDS.len() {
                return false;
            }
            s.slash_cursor = index;
            true
        }
        Overlay::ConnectProvider => {
            if local >= CONNECT_PROVIDERS.len() {
                return false;
            }
            s.connect_provider.picker.cursor = local;
            true
        }
        _ => false,
    }
}

// Entry index of the `row`-th visual row (headers count as rows).
fn model_entry_at_row(models: &[&crate::net::ModelEntry], row: usize) -> Option<usize> {
    let mut visual = 0;
    let mut prev: Option<mewcode_protocol::ProviderId> = None;
    for (i, model) in models.iter().enumerate() {
        if prev != Some(model.provider) {
            if visual >= row {
                return None;
            }
            visual += 1;
            prev = Some(model.provider);
        }
        if visual == row {
            return Some(i);
        }
        visual += 1;
    }
    None
}

fn clamp_picker_cursor(picker: &mut PickerState, len: usize) {
    if picker.cursor >= len {
        picker.cursor = len.saturating_sub(1);
    }
}

fn pick_model(s: &mut SessionState) -> Cmd {
    let Some((model, model_kind, context_length)) = s
        .model_picker
        .filtered_models()
        .get(s.model_picker.picker.cursor)
        .and_then(|entry| {
            entry
                .model_ref()
                .ok()
                .map(|model| (model, entry.kind, entry.context_length))
        })
    else {
        return Cmd::None;
    };
    if let Some(session) = s.session.as_ref() {
        return Cmd::PatchSession {
            id: session.id,
            patch: SessionPatch {
                model: Some(model),
                model_kind: Some(model_kind),
                model_context_length: context_length,
                ..Default::default()
            },
            from_rename: false,
        };
    }
    s.creation.pending_model = Some(model);
    s.creation.pending_model_kind = Some(model_kind);
    s.creation.pending_model_context_length = context_length;
    s.overlay = Overlay::None;
    Cmd::None
}

fn cursor_move(s: &mut SessionState, delta: i32) -> Cmd {
    match s.overlay {
        Overlay::ModelPicker => {
            let models = s.model_picker.filtered_models();
            let len = models.len();
            let max = len.saturating_sub(1) as i32;
            let cursor = if len == 0 {
                0
            } else {
                (s.model_picker.picker.cursor as i32 + delta).clamp(0, max) as usize
            };
            let cursor_row = model_cursor_row(&models, cursor);
            let header_row = model_header_row(&models, cursor);
            let visual_len = model_visual_len(&models);
            let viewport = s.model_picker.picker.viewport.max(1) as usize;
            let scroll = prefer_visible_header(
                clamp_picker_scroll(
                    s.model_picker.picker.scroll,
                    cursor_row,
                    visual_len,
                    viewport,
                ),
                header_row,
                cursor_row,
                viewport,
            );
            s.model_picker.picker.cursor = cursor;
            s.model_picker.picker.scroll = scroll;
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
        Overlay::SlashPicker => {
            if !SLASH_COMMANDS.is_empty() {
                let max = (SLASH_COMMANDS.len() - 1) as i32;
                s.slash_cursor = (s.slash_cursor as i32 + delta).clamp(0, max) as usize;
            }
            Cmd::None
        }
        Overlay::ConnectProvider => {
            move_picker_cursor(
                &mut s.connect_provider.picker,
                CONNECT_PROVIDERS.len(),
                delta,
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
    let scroll = if cursor < scroll {
        cursor
    } else if cursor >= scroll + visible {
        cursor + 1 - visible
    } else {
        scroll
    };
    scroll.min(len.saturating_sub(visible))
}

/// Re-clamp model picker scroll after async model data changes.
pub(crate) fn clamp_model_picker_scroll(s: &mut SessionState) {
    let models = s.model_picker.filtered_models();
    let cursor = model_cursor_row(&models, s.model_picker.picker.cursor);
    let len = model_visual_len(&models);
    let header = model_header_row(&models, s.model_picker.picker.cursor);
    let viewport = s.model_picker.picker.viewport.max(1) as usize;
    s.model_picker.picker.scroll = prefer_visible_header(
        clamp_picker_scroll(s.model_picker.picker.scroll, cursor, len, viewport),
        header,
        cursor,
        viewport,
    );
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

fn model_header_row(models: &[&crate::net::ModelEntry], cursor: usize) -> usize {
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

fn model_cursor_row(models: &[&crate::net::ModelEntry], cursor: usize) -> usize {
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

fn model_visual_len(models: &[&crate::net::ModelEntry]) -> usize {
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
