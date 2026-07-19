//! Key handling for picker overlays in the session screen.
//!
//! Model and session pickers share cursor movement and viewport clamping, but
//! differ in what Enter does: model picks patch or seed a model, session picks
//! open/delete saved sessions.

use crossterm::event::{KeyCode, KeyEvent};
use tui_textarea::{CursorMove, TextArea};

use mewcode_protocol::ModelId;

use crate::net::SessionPatch;

use super::super::model::{Cmd, FileEntry, Overlay, PickerState, SessionState};
use super::key_to_input;

const FILE_MENTION_PREFIX: char = '@';
const MAX_FILTERED_FILES: usize = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct FileMatchScore {
    rank: usize,
    offset: usize,
    len: usize,
}

impl FileMatchScore {
    fn new(rank: usize, offset: usize, len: usize) -> Self {
        Self { rank, offset, len }
    }
}

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
        KeyCode::Up => {
            file_cursor_move(s, -1);
            Cmd::None
        }
        KeyCode::Down => {
            file_cursor_move(s, 1);
            Cmd::None
        }
        KeyCode::Enter => {
            pick_file(s);
            Cmd::None
        }
        _ => {
            s.input.input(key_to_input(key));
            refresh_file_picker(s)
        }
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
    if current_file_query(s).is_none() {
        s.overlay = Overlay::None;
        return Cmd::None;
    }
    s.overlay = Overlay::FilePicker;
    if s.file_picker.files.is_none() {
        return Cmd::FetchFiles;
    }
    let len = filtered_files(s).len();
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
        }
        Overlay::FilePicker => file_cursor_move(s, delta),
        _ => {}
    }
}

fn file_cursor_move(s: &mut SessionState, delta: i32) {
    let len = filtered_files(s).len();
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
pub(super) fn clamp_model_picker_scroll(s: &mut SessionState) {
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
pub(super) fn clamp_session_list_scroll(s: &mut SessionState) {
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

pub(super) fn clamp_file_picker_scroll(s: &mut SessionState) {
    let viewport = s
        .file_picker
        .picker
        .viewport
        .max(s.file_picker.picker.viewport_max) as usize;
    s.file_picker.picker.scroll = clamp_picker_scroll(
        s.file_picker.picker.scroll,
        s.file_picker.picker.cursor,
        filtered_files(s).len(),
        viewport,
    );
}

pub(crate) fn filtered_files(s: &SessionState) -> Vec<&FileEntry> {
    let Some(files) = s.file_picker.files.as_ref() else {
        return Vec::new();
    };
    let query = current_file_query(s).unwrap_or_default();
    let show_hidden = query.starts_with('.');
    let mut matches = files
        .iter()
        .filter(|file| show_hidden || !is_hidden_path(&file.path))
        .filter_map(|file| file_match_score(&file.path, &query).map(|score| (score, file)))
        .collect::<Vec<_>>();
    matches.sort_by(|(a_score, a_file), (b_score, b_file)| {
        a_score
            .cmp(b_score)
            .then_with(|| a_file.path.cmp(&b_file.path))
    });
    matches
        .into_iter()
        .map(|(_, file)| file)
        .take(MAX_FILTERED_FILES)
        .collect()
}

fn is_hidden_path(path: &str) -> bool {
    path.split('/').any(|part| part.starts_with('.'))
}

fn file_match_score(path: &str, query: &str) -> Option<FileMatchScore> {
    if query.is_empty() {
        return Some(FileMatchScore::new(
            0,
            path.matches('/').count(),
            path.len(),
        ));
    }
    let path = path.to_ascii_lowercase();
    let query = query.to_ascii_lowercase();
    let basename = path.rsplit('/').next().unwrap_or(&path);
    if basename.starts_with(&query) {
        return Some(FileMatchScore::new(0, basename.len(), path.len()));
    }
    if path.starts_with(&query) {
        return Some(FileMatchScore::new(1, path.len(), path.len()));
    }
    if let Some(idx) = basename.find(&query) {
        return Some(FileMatchScore::new(2, idx, path.len()));
    }
    if let Some(idx) = path.find(&query) {
        return Some(FileMatchScore::new(3, idx, path.len()));
    }
    if is_subsequence(&query, &path) {
        return Some(FileMatchScore::new(4, path.len(), path.len()));
    }
    None
}

fn is_subsequence(needle: &str, haystack: &str) -> bool {
    let mut chars = needle.chars();
    let Some(mut wanted) = chars.next() else {
        return true;
    };
    for c in haystack.chars() {
        if c == wanted {
            let Some(next) = chars.next() else {
                return true;
            };
            wanted = next;
        }
    }
    false
}

pub(super) fn current_file_query(s: &SessionState) -> Option<String> {
    let (row, col) = s.input.cursor();
    let line = s.input.lines().get(row)?;
    let prefix: String = line.chars().take(col).collect();
    let token = prefix
        .rsplit_once(char::is_whitespace)
        .map_or(prefix.as_str(), |(_, token)| token);
    token
        .strip_prefix(FILE_MENTION_PREFIX)
        .map(ToOwned::to_owned)
}

fn pick_file(s: &mut SessionState) {
    let Some(path) = filtered_files(s)
        .get(s.file_picker.picker.cursor)
        .map(|file| file.path.clone())
    else {
        return;
    };
    replace_current_file_token(s, &format!("{FILE_MENTION_PREFIX}{path}"));
    s.overlay = Overlay::None;
}

fn replace_current_file_token(s: &mut SessionState, replacement: &str) {
    let (row, col) = s.input.cursor();
    let mut lines = s.input.lines().to_vec();
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
    s.input = TextArea::new(lines);
    s.input.move_cursor(CursorMove::Jump(
        row as u16,
        (start + replacement.chars().count()) as u16,
    ));
}
