//! Where to look for skills, and in what order.

use std::path::PathBuf;

/// Where to look for skills. The order of fields here is the
/// precedence order (later overrides earlier on name collision).
///
/// `Default::default()` produces a config that discovers the standard
/// locations (global + project), so `SkillRegistry::load(&config)`
/// works out of the box.
#[derive(Debug, Clone)]
pub struct SkillLoadConfig {
    /// Where to look for *bundled* skills (the repo's own skills).
    /// These are the lowest precedence.
    pub bundled_dir: Option<PathBuf>,
    /// Additional shared/external skill directories. Inserted between
    /// bundled and project precedence.
    pub external_dirs: Vec<PathBuf>,
    /// If set, walk up from here looking for `<start>/.mewcode/skills`.
    /// This is `None` only in tests; production always sets it.
    pub project_search_start: Option<PathBuf>,
    /// If true, also load `./skills/` (dev convenience). Production
    /// users should keep their skills in `.mewcode/skills/`.
    pub include_dev_dir: bool,
}

impl Default for SkillLoadConfig {
    /// Discovers the standard locations: `~/.config/mewcode/skills/`
    /// (global) and `<cwd>/.mewcode/skills/` (project, walking up).
    /// The dev `./skills/` dir is not included; opt in with
    /// `include_dev_dir: true` if you want it.
    fn default() -> Self {
        Self {
            bundled_dir: None,
            external_dirs: Vec::new(),
            project_search_start: std::env::current_dir().ok(),
            include_dev_dir: false,
        }
    }
}
