//! Durable memory scaffold — the agent's persistent fact store.
//!
//! Each profile gets one `.md` file under `~/.mewcode/memories/`.
//! The content is injected into the system prompt as a `<memory>` section
//! so the agent sees its persistent facts every turn.
//!
//! This is the mewcode equivalent of Hermes Agent's MEMORY.md / USER.md
//! system: durable facts the agent can read and update via the
//! `mewcode_memory` tool.
//!
//! NOTE: This is intentionally a scaffold. The file read/write path, the
//! `mewcode_memory` tool, and the system-prompt injection are wired, but
//! higher-level behaviours — when the model should save a fact, memory
//! summarisation/compaction, multi-profile selection, and client-visible
//! memory UI — are not implemented yet. They will be fleshed out in a
//! future phase.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

/// Root directory for memory profiles.
const MEMORIES_DIR: &str = "memories";

/// Default memory profile name. Used only when no project root is known
/// (e.g. standalone tool tests) — production chat/compaction turns always
/// resolve a project-scoped profile via [`MemoryStore::for_project`] so
/// durable facts from one project never leak into an unrelated one.
const DEFAULT_PROFILE: &str = "default";

/// A durable fact store backed by a single markdown file.
#[derive(Debug, Clone)]
pub struct MemoryStore {
    /// Path to the memory file for the active profile.
    path: PathBuf,
}

impl MemoryStore {
    /// Build a store rooted at `data_dir/memories/` for the default profile.
    pub fn new(data_dir: PathBuf) -> Self {
        let path = data_dir
            .join(MEMORIES_DIR)
            .join(format!("{DEFAULT_PROFILE}.md"));
        Self { path }
    }

    /// Build a store for a specific profile name under `data_dir/memories/`.
    pub fn with_profile(data_dir: PathBuf, profile: &str) -> Self {
        let path = data_dir.join(MEMORIES_DIR).join(format!("{profile}.md"));
        Self { path }
    }

    /// Scoped memory: projects with the same directory name at different
    /// paths get distinct files (hash-suffixed). Falls back to global
    /// `"default"` when the path can't be canonicalized.
    pub fn for_project(data_dir: PathBuf, project_root: &Path) -> Self {
        let profile = match std::fs::canonicalize(project_root) {
            Ok(canonical) => {
                let mut hasher = DefaultHasher::new();
                canonical.hash(&mut hasher);
                let hash = hasher.finish();
                let name = canonical
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .filter(|n| !n.is_empty())
                    .unwrap_or_else(|| "project".to_string());
                // Project directory names can contain chars the tool rejects
                // (`/`, `\`, `..`, leading `.`). Sanitize before using.
                let sanitized: String = name
                    .chars()
                    .map(|c| {
                        if c.is_alphanumeric() || c == '-' || c == '_' {
                            c
                        } else {
                            '-'
                        }
                    })
                    .collect();
                let sanitized = sanitized.trim_start_matches('-').to_string();
                let sanitized = if sanitized.is_empty() {
                    "project".to_string()
                } else {
                    sanitized
                };
                format!("{sanitized}-{hash:016x}")
            }
            Err(_) => {
                let mut hasher = DefaultHasher::new();
                project_root.hash(&mut hasher);
                let hash = hasher.finish();
                format!("unresolved-{hash:016x}")
            }
        };
        Self::with_profile(data_dir, &profile)
    }

    /// Read the current memory content. Returns an empty string when the
    /// file does not exist or cannot be read (first use / corrupt file).
    pub fn read(&self) -> String {
        std::fs::read_to_string(&self.path).unwrap_or_default()
    }

    /// Overwrite the memory file with new content. Creates parent
    /// directories on first use.
    pub fn write(&self, content: &str) -> std::io::Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&self.path, content)
    }

    /// Format memory content as a system-prompt section. Returns `None`
    /// when memory is empty or absent.
    pub fn format(&self) -> Option<String> {
        let body = self.read();
        if body.trim().is_empty() {
            return None;
        }
        Some(format!("<memory>\n{}\n</memory>", body.trim()))
    }

    /// The path to the memory file (useful for the `mewcode_memory` tool).
    pub fn path(&self) -> &PathBuf {
        &self.path
    }

    /// The root data directory this store's file lives under (two levels up
    /// from the `.md` file: `<data_dir>/memories/<profile>.md`). `None` if
    /// the path is too shallow to have a `memories/` grandparent — shouldn't
    /// happen for any store built via `new`/`with_profile`/`for_project`.
    pub fn data_dir(&self) -> Option<&Path> {
        self.path.parent()?.parent()
    }
}
