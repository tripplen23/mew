//! Durable memory scaffold — the agent's persistent fact store.
//!
//! Each profile gets one `.md` file under `~/.mewcode/memories/`.
//! The content is injected into the system prompt as a `# Memory` section
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

use std::path::PathBuf;

/// Root directory for memory profiles.
const MEMORIES_DIR: &str = "memories";

/// Default memory profile name.
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
        Some(format!("# Memory\n\n{}", body.trim()))
    }

    /// The path to the memory file (useful for the `mewcode_memory` tool).
    pub fn path(&self) -> &PathBuf {
        &self.path
    }
}
