//! Pure helpers for selecting the user prompt for a harness turn. These stay
//! free of the runtime, the network, and the mpsc channel.

use std::fmt::Write as _;
use std::io::Read as _;
use std::path::Path;

use mewcode_protocol::{Message, MessagePart, Role};

const MAX_REFERENCED_FILES: usize = 10;
const MAX_REFERENCED_FILE_BYTES: u64 = 50 * 1024;
const REFERENCED_FILES_HEADER: &str = "Referenced files:";
const USER_MESSAGE_HEADER: &str = "User message:";
const TRUNCATED_MARKER: &str = "[truncated]";
const NOT_LOADED_MARKER: &str = "not loaded";
const NOT_A_FILE_ERROR: &str = "not a file";
const MENTION_PREFIX: char = '@';
const MENTION_TRAILING_PUNCTUATION: &[char] = &[',', '.', ':', ';'];
const CODE_FENCE: &str = "```";

/// Text of the most recent [`Role::User`] message or
/// `None` when the history holds no user message.
pub fn last_user_text(messages: &[Message]) -> Option<String> {
    messages
        .iter()
        .rev()
        .find(|m| m.role == Role::User)
        .map(|m| {
            m.parts
                .iter()
                .filter_map(|p| match p {
                    MessagePart::Text { text } => Some(text.as_str()),
                    _ => None,
                })
                .collect()
        })
}

/// Test-facing prompt expansion for file mentions; production calls it via [`crate::harness::Harness`].
#[doc(hidden)]
pub fn user_text_with_file_context(messages: &[Message], root: &Path) -> Option<String> {
    let msg = messages.iter().rev().find(|m| m.role == Role::User)?;
    let text = text_of(msg);
    let mut paths = mentioned_paths(&text);
    for part in &msg.parts {
        if let MessagePart::FileMention { path } = part {
            paths.push(path.clone());
        }
    }
    paths.sort();
    paths.dedup();

    let mut expanded = Vec::new();
    for path in paths {
        if path.ends_with('/') {
            let dir_path = path.trim_end_matches('/');
            let resolved = root.join(dir_path);
            if resolved.is_dir() {
                let remaining = MAX_REFERENCED_FILES.saturating_sub(expanded.len());
                collect_dir_files(&resolved, root, &mut expanded, remaining);
            }
        } else {
            expanded.push(path);
        }
    }
    expanded.sort();
    expanded.dedup();
    expanded.truncate(MAX_REFERENCED_FILES);

    if expanded.is_empty() {
        return Some(text);
    }

    let mut out = String::new();
    let _ = writeln!(out, "{REFERENCED_FILES_HEADER}");
    for path in expanded {
        out.push_str(&format_file_context(root, &path));
    }
    let _ = writeln!(out, "\n{USER_MESSAGE_HEADER}");
    out.push_str(&text);
    Some(out)
}

fn collect_dir_files(dir: &Path, root: &Path, out: &mut Vec<String>, remaining: usize) {
    if remaining == 0 {
        return;
    }
    let mut entries: Vec<_> = match std::fs::read_dir(dir) {
        Ok(rd) => rd.filter_map(|e| e.ok()).collect(),
        Err(_) => return,
    };
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        if out.len() >= remaining {
            break;
        }
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if mewcode_protocol::tool::SKIPPED_DIRS.contains(&name.as_ref()) {
            continue;
        }
        if path.is_file() {
            if let Ok(rel) = path.strip_prefix(root) {
                out.push(rel.to_string_lossy().replace('\\', "/"));
            }
        } else if path.is_dir() {
            collect_dir_files(&path, root, out, remaining - out.len());
        }
    }
}

fn text_of(msg: &Message) -> String {
    msg.parts
        .iter()
        .filter_map(|p| match p {
            MessagePart::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect()
}

fn mentioned_paths(text: &str) -> Vec<String> {
    text.split_whitespace()
        .filter_map(|token| {
            token
                .strip_prefix(MENTION_PREFIX)
                .map(|path| path.trim_end_matches(MENTION_TRAILING_PUNCTUATION))
                .filter(|path| !path.is_empty())
                .map(ToOwned::to_owned)
        })
        .collect()
}

fn format_file_context(root: &Path, path: &str) -> String {
    match read_file_context(root, path) {
        Ok((content, truncated)) => {
            let suffix = if truncated {
                format!("\n{TRUNCATED_MARKER}")
            } else {
                String::new()
            };
            format!("\n{MENTION_PREFIX}{path}\n{CODE_FENCE}\n{content}{suffix}\n{CODE_FENCE}\n")
        }
        Err(e) => format!("\n{MENTION_PREFIX}{path}\n[{NOT_LOADED_MARKER}: {e}]\n"),
    }
}

fn read_file_context(root: &Path, path: &str) -> std::io::Result<(String, bool)> {
    let resolved = mewcode_protocol::tool::resolve_inside_root(root, Path::new(path))?;
    if !resolved.is_file() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            NOT_A_FILE_ERROR,
        ));
    }
    let mut file = std::fs::File::open(&resolved)?;
    let mut bytes = Vec::new();
    file.by_ref()
        .take(MAX_REFERENCED_FILE_BYTES + 1)
        .read_to_end(&mut bytes)?;
    let truncated = bytes.len() as u64 > MAX_REFERENCED_FILE_BYTES;
    bytes.truncate(MAX_REFERENCED_FILE_BYTES as usize);
    Ok((String::from_utf8_lossy(&bytes).into_owned(), truncated))
}
