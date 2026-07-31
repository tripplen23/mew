//! File-picker derived state: filtering and scoring the current `@`-mention
//! query against the fetched file list. Shared by the view and the update
//! loop so both agree on what the picker shows.

use super::{FileEntry, SessionState};

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

impl SessionState {
    pub fn filtered_files(&self) -> Vec<&FileEntry> {
        let Some(files) = self.file_picker.files.as_ref() else {
            return Vec::new();
        };
        let query = self.current_file_query().unwrap_or_default();
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

    pub fn current_file_query(&self) -> Option<String> {
        let (row, col) = self.input.cursor();
        let line = self.input.lines().get(row)?;
        let prefix: String = line.chars().take(col).collect();
        let token = prefix
            .rsplit_once(char::is_whitespace)
            .map_or(prefix.as_str(), |(_, token)| token);
        token
            .strip_prefix(FILE_MENTION_PREFIX)
            .map(ToOwned::to_owned)
    }

    pub fn file_mention_token(path: &str, is_dir: bool) -> String {
        if is_dir {
            format!("{FILE_MENTION_PREFIX}{path}/")
        } else {
            format!("{FILE_MENTION_PREFIX}{path}")
        }
    }
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
