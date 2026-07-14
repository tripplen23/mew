//! Skill registry.
//!
//! ## Loading
//!
//! `SkillRegistry::load(&config)` is the single entry point. It walks
//! every directory in `config` (in precedence order: project → external
//! → bundled), reads every immediate subdirectory that contains a
//! `SKILL.md`, and inserts the resulting `Skill` into a name →
//! `LoadedSkill` map. **Project shadows external shadows bundled** on
//! name collisions, so a client can override a bundled skill locally
//! without forking the project.
//!
//! Non-existent paths are silently skipped (Hermes behaviour).
//! `${VAR}` and `~` are expanded before the directory is tested.
//!
//! ## Progressive disclosure
//!
//! Three levels, exposed through the skill tools:
//! - L0: [`SkillRegistry::catalog_for_system_prompt`] — name +
//!   description only, shipped in the system prompt.
//! - L1: [`SkillRegistry::view_body`] — full `SKILL.md` body, returned
//!   by the `skill_view` tool with no `path`.
//! - L2: [`SkillRegistry::view_subfile`] — one sub-file under the
//!   skill directory, returned by the `skill_view` tool with a `path`.
//!
//! ## File layout
//!
//! ```text
//! mod.rs     — this file: SkillRegistry struct + loading + introspection
//! config.rs  — SkillLoadConfig (where to look + precedence)
//! source.rs  — SkillSource + LoadedSkill (provenance)
//! catalog.rs — SkillListEntry + the L0 catalog/list renderers
//! view.rs    — the L1/L2 read methods (view_body, view_subfile)
//! ```

mod catalog;
mod config;
mod source;
mod view;

pub use catalog::SkillListEntry;
pub use config::SkillLoadConfig;
pub use source::{LoadedSkill, SkillSource};

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use mewcode_protocol::{GLOBAL_SKILLS_DIR, PROJECT_SKILLS_DIR, SKILL_FILE, Skill};
use tracing::{info, warn};

/// Registry of skills available to the engine.
#[derive(Debug, Default, Clone)]
pub struct SkillRegistry {
    skills: HashMap<String, LoadedSkill>,
    /// Directories we successfully loaded from, in load order.
    loaded_paths: Vec<PathBuf>,
    /// Directories we attempted to load from but didn't find.
    missing_paths: Vec<PathBuf>,
}

impl SkillRegistry {
    /// Build an empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Load all skills from the default locations (global + project).
    /// New code should prefer [`SkillRegistry::load`] with an explicit
    /// config (e.g. to add `external_dirs` or `bundled_dir`).
    pub fn load_defaults() -> Self {
        Self::load(&SkillLoadConfig::default())
    }

    /// Load skills according to `config`. Later sources override
    /// earlier ones on name collision — bundled is lowest, dev is
    /// highest. Project skills intentionally shadow global installs
    /// so a repo can override a shared skill locally.
    pub fn load(config: &SkillLoadConfig) -> Self {
        let mut reg = Self::new();

        // 1. Bundled (lowest precedence).
        if let Some(dir) = config.bundled_dir.as_deref() {
            reg.load_dir(dir, SkillSource::Bundled);
        }

        // 2. External dirs (in the order they were declared).
        for dir in &config.external_dirs {
            reg.load_dir(dir, SkillSource::External);
        }

        // 3. Global (`~/.config/mewcode/skills`). Loaded before project
        //    so project can shadow global on name collision.
        if let Some(home) = dirs::home_dir() {
            let global = home.join(".config").join("mewcode").join(GLOBAL_SKILLS_DIR);
            reg.load_dir(&global, SkillSource::Global);
        }

        // 4. Project (walks up from the search start). Shadows global.
        if let Some(start) = config.project_search_start.as_deref() {
            if let Some(p) = Self::find_project_skills_dir_from(start) {
                reg.load_dir(&p, SkillSource::Project);
            }
        }

        // 5. Dev convenience (`./skills/`).
        if config.include_dev_dir {
            if let Some(p) = std::env::current_dir()
                .ok()
                .and_then(|cwd| Self::find_dev_skills_dir_from(&cwd))
            {
                reg.load_dir(&p, SkillSource::Dev);
            }
        }

        reg
    }

    /// Walk up from `start` looking for the first `.mewcode/skills` directory.
    pub fn find_project_skills_dir() -> Option<PathBuf> {
        Self::find_project_skills_dir_from(&std::env::current_dir().ok()?)
    }

    /// Walk up from `start` looking for the first `.mewcode/skills` directory.
    pub fn find_project_skills_dir_from(start: &Path) -> Option<PathBuf> {
        let mut cur: Option<&Path> = Some(start);
        while let Some(dir) = cur {
            let candidate = dir.join(PROJECT_SKILLS_DIR);
            if candidate.is_dir() {
                return Some(candidate);
            }
            cur = dir.parent();
        }
        None
    }

    /// Dev convenience: also look for `./skills/`. Production users
    /// should keep skills in `.mewcode/skills/`.
    pub fn find_dev_skills_dir_from(start: &Path) -> Option<PathBuf> {
        let dev = start.join("skills");
        if dev.is_dir() { Some(dev) } else { None }
    }

    /// Load all skills from a directory. Each immediate subdirectory of
    /// `dir` that contains a `SKILL.md` is treated as a skill bundle.
    pub fn load_dir(&mut self, dir: &Path, source: SkillSource) {
        if !dir.is_dir() {
            self.missing_paths.push(dir.to_path_buf());
            return;
        }
        self.loaded_paths.push(dir.to_path_buf());

        let entries = match std::fs::read_dir(dir) {
            Ok(entries) => entries,
            Err(err) => {
                warn!(path = %dir.display(), error = %err, "could not read skills dir");
                return;
            }
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let skill_md = path.join(SKILL_FILE);
            if !skill_md.is_file() {
                continue;
            }
            match Skill::load_from_dir(&path) {
                Ok(skill) => {
                    info!(
                        name = %skill.name,
                        source = source.label(),
                        path = %path.display(),
                        "loaded skill"
                    );
                    self.skills
                        .insert(skill.name.clone(), LoadedSkill::new(skill, source));
                }
                Err(e) => {
                    warn!(
                        path = %path.display(),
                        error = %e,
                        "could not load skill bundle"
                    );
                }
            }
        }
    }

    /// Look up a skill by name.
    pub fn get(&self, name: &str) -> Option<&LoadedSkill> {
        self.skills.get(name)
    }

    /// Number of loaded skills.
    pub fn len(&self) -> usize {
        self.skills.len()
    }

    /// `true` if no skills are loaded.
    pub fn is_empty(&self) -> bool {
        self.skills.is_empty()
    }

    /// All loaded skills, sorted by name.
    pub fn skills(&self) -> Vec<&LoadedSkill> {
        let mut v: Vec<&LoadedSkill> = self.skills.values().collect();
        v.sort_by(|a, b| a.skill.name.cmp(&b.skill.name));
        v
    }

    /// Directories we successfully loaded from.
    pub fn loaded_paths(&self) -> &[PathBuf] {
        &self.loaded_paths
    }

    /// Directories we attempted to load from but didn't find.
    pub fn missing_paths(&self) -> &[PathBuf] {
        &self.missing_paths
    }
}
