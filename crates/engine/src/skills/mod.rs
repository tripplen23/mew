//! Skill registry. Loads from two scopes (global + per-project), with
//! per-project overriding global on name collision. The model sees only
//! the catalog (name + description) in its system prompt; the body is
//! loaded into context only when invoked.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use mewcode_protocol::{Skill, SkillError, GLOBAL_SKILLS_DIR, PROJECT_SKILLS_DIR, SKILL_FILE};
use tracing::{info, warn};

/// Source of a loaded skill.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkillSource {
    /// `~/.config/mewcode/skills/...`
    Global,
    /// `./.mewcode/skills/...` (or inherited from a parent directory).
    Project,
}

/// A loaded skill plus its provenance.
#[derive(Debug, Clone)]
pub struct LoadedSkill {
    /// The skill itself.
    pub skill: Skill,
    /// Where it was loaded from.
    pub source: SkillSource,
}

impl LoadedSkill {
    fn new(skill: Skill, source: SkillSource) -> Self {
        Self { skill, source }
    }
}

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

    /// Load all skills from the default locations. Per-project overrides
    /// global on name collision.
    pub fn load_defaults() -> Self {
        let mut reg = Self::new();
        if let Some(home) = dirs::home_dir() {
            let global = home.join(".config").join("mewcode").join(GLOBAL_SKILLS_DIR);
            reg.load_dir(&global, SkillSource::Global);
        }
        if let Some(p) = Self::find_project_skills_dir() {
            reg.load_dir(&p, SkillSource::Project);
        }
        // Dev convenience: load `./skills/` if present. Harmless for end users.
        if let Some(p) = std::env::current_dir()
            .ok()
            .and_then(|cwd| Self::find_dev_skills_dir_from(&cwd))
        {
            reg.load_dir(&p, SkillSource::Project);
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
        if dev.is_dir() {
            Some(dev)
        } else {
            None
        }
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
            Err(e) => {
                warn!(dir = %dir.display(), error = %e, "could not read skills directory");
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
                        source = ?source,
                        path = %path.display(),
                        "loaded skill"
                    );
                    self.skills.insert(
                        skill.name.clone(),
                        LoadedSkill::new(skill, source),
                    );
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

    /// Render the system-prompt catalog. Empty string if no skills are
    /// loaded (so callers can prepend unconditionally).
    pub fn catalog_for_system_prompt(&self) -> String {
        if self.skills.is_empty() {
            return String::new();
        }
        let mut out = String::from("\n## Available skills\n\n");
        out.push_str("You have the following skills installed. Each skill is a bundle of ");
        out.push_str("specialised instructions for a particular kind of task. When the user ");
        out.push_str("asks you to do something that matches a skill's description, invoke it ");
        out.push_str("with the `use_skill` tool to load the full instructions into your ");
        out.push_str("context before proceeding. Do not invent skill names — only use the ones ");
        out.push_str("listed below.\n\n");

        for loaded in self.skills() {
            out.push_str(&loaded.skill.catalog_entry());
            out.push('\n');
        }
        out
    }

    /// Resolve a skill by name, returning the full body. This is what
    /// the `use_skill` tool calls.
    pub fn resolve_body(&self, name: &str) -> Result<String, SkillError> {
        let loaded = self
            .skills
            .get(name)
            .ok_or_else(|| SkillError::MissingField {
                path: PathBuf::from(format!("<skill:{name}>")),
                field: "name",
            })?;
        Ok(loaded.skill.body.clone())
    }
}

