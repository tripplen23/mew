//! Regression tests for the session file-picker derived state:
//! `filtered_files`, `current_file_query`, and `file_mention_token`.
//!
//! `file_match_score`'s ranking tiers are asserted through the ordering
//! contract of `filtered_files` (the only public consumer of the score).
//!
//! No-behavior-change contract: these tests pin the file-picker behavior
//! (filtering, query, mention-token, and ranking) against the
//! `SessionState` API, so refactors of the state/picker integration must
//! keep it green without changing user-visible file-picker behavior.
//!
//! Verification plan: `current_file_query` tests assert the parsed query
//! under cursor; the `@`-token tests assert the active file-mention
//! boundary; `filtered_files` ordering tests assert rank tiers descend
//! correctly.

use tui_textarea::{CursorMove, TextArea};

use mewcode_client::runtime::model::{FileEntry, SessionState};

// --- fixtures -------------------------------------------------------------

fn file(path: &str) -> FileEntry {
    FileEntry {
        path: path.to_string(),
        is_dir: false,
    }
}

/// A session whose composer holds `text` with the cursor at column `col`.
fn composer_state(text: &str, col: usize) -> SessionState {
    let mut s = SessionState::empty();
    s.composer = TextArea::from(vec![text.to_string()]);
    s.composer.move_cursor(CursorMove::Jump(0, col as u16));
    s
}

/// A session with a fetched file list and the composer at "read @".
fn state_with_files(files: Vec<FileEntry>) -> SessionState {
    let mut s = composer_state("read @", 6);
    s.file_picker.files = Some(files);
    s
}

fn paths(files: &[&FileEntry]) -> Vec<String> {
    files.iter().map(|f| f.path.clone()).collect()
}

// --- current_file_query ---------------------------------------------------

#[test]
fn current_file_query_extracts_mention_token() {
    let s = composer_state("read @src", 9);
    assert_eq!(s.current_file_query().as_deref(), Some("src"));
}

#[test]
fn current_file_query_empty_mention_is_some_empty() {
    let s = composer_state("read @", 6);
    assert_eq!(s.current_file_query().as_deref(), Some(""));
}

#[test]
fn current_file_query_cursor_before_mention_is_none() {
    let s = composer_state("read @src", 5);
    assert_eq!(s.current_file_query(), None);
}

#[test]
fn current_file_query_without_mention_is_none() {
    let s = composer_state("no mention here", 16);
    assert_eq!(s.current_file_query(), None);
}

#[test]
fn current_file_query_keeps_leading_dot_for_hidden_query() {
    let s = composer_state("@.env", 5);
    assert_eq!(s.current_file_query().as_deref(), Some(".env"));
}

// --- filtered_files: hidden paths -----------------------------------------

#[test]
fn filtered_files_hides_dotfiles_and_hidden_dirs_by_default() {
    let s = state_with_files(vec![
        file(".env"),
        file("README.md"),
        file("src/.git/config"),
        file("src/main.rs"),
    ]);
    assert_eq!(paths(&s.filtered_files()), ["README.md", "src/main.rs"]);
}

#[test]
fn filtered_files_shows_hidden_for_dot_query() {
    let mut s = state_with_files(vec![
        file(".env"),
        file("README.md"),
        file("src/.git/config"),
        file("src/main.rs"),
    ]);
    s.composer = TextArea::from(vec!["read @.".to_string()]);
    s.composer.move_cursor(CursorMove::Jump(0, 7));
    assert_eq!(
        paths(&s.filtered_files()),
        [".env", "src/main.rs", "README.md", "src/.git/config"]
    );
}

// --- filtered_files: ranking order ----------------------------------------

#[test]
fn filtered_files_orders_by_match_rank() {
    let mut s = state_with_files(vec![
        file("src/main.rs"),          // basename prefix  (rank 0)
        file("maintenance/notes.md"), // path prefix      (rank 1)
        file("sadomain.txt"),         // basename contains (rank 2)
        file("src/main/support.txt"), // path contains     (rank 3)
        file("mountain.txt"),         // subsequence       (rank 4)
    ]);
    s.composer = TextArea::from(vec!["read @main".to_string()]);
    s.composer.move_cursor(CursorMove::Jump(0, 10));
    assert_eq!(
        paths(&s.filtered_files()),
        [
            "src/main.rs",
            "maintenance/notes.md",
            "sadomain.txt",
            "src/main/support.txt",
            "mountain.txt",
        ]
    );
}

#[test]
fn filtered_files_empty_query_sorts_alphabetically() {
    let s = state_with_files(vec![file("b.txt"), file("a.txt"), file("c.txt")]);
    assert_eq!(paths(&s.filtered_files()), ["a.txt", "b.txt", "c.txt"]);
}

#[test]
fn filtered_files_no_files_is_empty() {
    let s = composer_state("read @", 6);
    assert!(s.filtered_files().is_empty());
}

// --- filtered_files: result cap -------------------------------------------

#[test]
fn filtered_files_caps_at_ten_results() {
    let files = (1..=11).map(|i| file(&format!("f{i:02}.txt"))).collect();
    let s = state_with_files(files);
    let filtered = s.filtered_files();
    assert_eq!(filtered.len(), 10);
    assert_eq!(filtered.first().unwrap().path, "f01.txt");
    assert_eq!(filtered.last().unwrap().path, "f10.txt");
}

// --- file_mention_token ---------------------------------------------------

#[test]
fn file_mention_token_builds_mention_strings() {
    assert_eq!(
        SessionState::file_mention_token("README.md", false),
        "@README.md"
    );
    assert_eq!(
        SessionState::file_mention_token("a/b/c.rs", false),
        "@a/b/c.rs"
    );
    assert_eq!(SessionState::file_mention_token("src", true), "@src/");
}
