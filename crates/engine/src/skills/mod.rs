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
//!   by the `skill_view` tool.
//! - L2: [`SkillRegistry::view_subfile`] — one sub-file under the
//!   skill directory, returned by the `skill_view` tool with a `path`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use mewcode_protocol::{
    GLOBAL_SKILLS_DIR, PROJECT_SKILLS_DIR, SKILL_FILE, Skill, SkillError, read_skill_subfile,
};
use tracing::{info, warn};

/// Source of a loaded skill.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkillSource {
    /// `~/.config/mewcode/skills/...` (global install).
    Global,
    /// `<project>/.mewcode/skills/...` (per-project, walks up).
    Project,
    /// A directory listed in `config.external_dirs` (shared install).
    External,
    /// `./skills/` (dev convenience — never loaded for end users).
    Dev,
    /// A directory listed in `config.bundled_dir` (the repo's own skills).
    Bundled,
}

impl SkillSource {
    /// Short label used in the tool's catalog output so the model can
    /// tell where a skill came from.
    pub fn label(self) -> &'static str {
        match self {
            SkillSource::Global => "global",
            SkillSource::Project => "project",
            SkillSource::External => "external",
            SkillSource::Dev => "dev",
            SkillSource::Bundled => "bundled",
        }
    }
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

/// Where to look for skills. The order of fields here is the
/// precedence order (later overrides earlier on name collision).
///
/// The constructor is `Default::default()` and produces a config that
/// discovers the three canonical locations (global, project, dev). The
/// server layer augments it with `external_dirs` from `mewcode.toml`
/// and `bundled_dir` from the engine binary's own data directory.
#[derive(Debug, Clone, Default)]
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

impl SkillLoadConfig {
    /// Standard config for end users: global + project + dev, no
    /// external dirs, no bundled dir.
    pub fn for_user() -> Self {
        Self {
            bundled_dir: None,
            external_dirs: Vec::new(),
            project_search_start: std::env::current_dir().ok(),
            include_dev_dir: true,
        }
    }

    /// Standard config for the bundled server: adds a `bundled_dir` for
    /// the skills shipped with the binary.
    pub fn for_server(bundled_dir: PathBuf, external_dirs: Vec<PathBuf>) -> Self {
        Self {
            bundled_dir: Some(bundled_dir),
            external_dirs,
            project_search_start: std::env::current_dir().ok(),
            include_dev_dir: true,
        }
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

    /// Load all skills from the default locations. Kept as a
    /// convenience for callers that don't need external_dirs; new code
    /// should prefer [`SkillRegistry::load`] with an explicit config.
    pub fn load_defaults() -> Self {
        Self::load(&SkillLoadConfig::for_user())
    }

    /// Load skills according to `config`. Project shadows external
    /// shadows bundled on name collision.
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

        // 3. Project (walks up from the search start).
        if let Some(start) = config.project_search_start.as_deref() {
            if let Some(p) = Self::find_project_skills_dir_from(start) {
                reg.load_dir(&p, SkillSource::Project);
            }
        }

        // 4. Global (`~/.config/mewcode/skills`).
        if let Some(home) = dirs::home_dir() {
            let global = home.join(".config").join("mewcode").join(GLOBAL_SKILLS_DIR);
            reg.load_dir(&global, SkillSource::Global);
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
            Ok(e) => e,
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

    /// Render the L0 system-prompt catalog. Empty string if no skills
    /// are loaded (so callers can prepend unconditionally).
    ///
    /// The format is intentionally compact: one line per skill, no
    /// body, no per-skill invocation hint. The model reads the
    /// `skills_list` and `skill_view` tool descriptors (1-2 lines
    /// each) for the *how*, and this block for the *what*.
    pub fn catalog_for_system_prompt(&self) -> String {
        if self.skills.is_empty() {
            return String::new();
        }
        let mut out = String::from("\n## Available skills\n\n");
        out.push_str("The following skills are installed. Each is a bundle of ");
        out.push_str("specialised instructions for a particular kind of task. To read a ");
        out.push_str("skill's full instructions before proceeding, call ");
        out.push_str("`skill_view(name=\"<name>\")`. To read a sub-file (e.g. ");
        out.push_str("`references/foo.md`), call `skill_view(name=\"<name>\", path=\"references/foo.md\")`. ");
        out.push_str("Do not invent skill names — only use the ones listed below.\n\n");

        for loaded in self.skills() {
            out.push_str(&loaded.skill.catalog_entry());
            out.push('\n');
        }
        out
    }

    /// L0 catalog for the `skills_list` tool (model-facing JSON).
    /// Returns `[{name, description, source, assets}, ...]`.
    pub fn list_for_tool(&self) -> Vec<SkillListEntry> {
        self.skills()
            .into_iter()
            .map(|loaded| SkillListEntry {
                name: loaded.skill.name.clone(),
                description: loaded.skill.description.clone(),
                source: loaded.source.label().to_string(),
                assets: loaded
                    .skill
                    .assets
                    .iter()
                    .map(|p| p.to_string_lossy().to_string())
                    .collect(),
            })
            .collect()
    }

    /// L1 / L2 read. With no `path`, returns the skill body (with a
    /// truncation marker if `max_chars` is hit). With `path`, returns
    /// the sub-file content. Errors are surfaced for the model to see
    /// and recover from.
    pub fn view(
        &self,
        name: &str,
        subpath: Option<&str>,
        max_chars: usize,
    ) -> Result<SkillView, SkillError> {
        let loaded = self
            .skills
            .get(name)
            .ok_or_else(|| SkillError::MissingField {
                path: PathBuf::from(format!("<skill:{name}>")),
                field: "name",
            })?;
        match subpath {
            None => {
                let body = &loaded.skill.body;
                if body.len() > max_chars {
                    Ok(SkillView::Body {
                        body: body[..max_chars].to_string(),
                        truncated: true,
                        total_chars: body.len(),
                    })
                } else {
                    Ok(SkillView::Body {
                        body: body.clone(),
                        truncated: false,
                        total_chars: body.len(),
                    })
                }
            }
            Some(rel) => {
                let (resolved_rel, content) =
                    read_skill_subfile(&loaded.skill.location, rel)?;
                Ok(SkillView::Subfile {
                    path: resolved_rel.to_string_lossy().to_string(),
                    content,
                })
            }
        }
    }
}

/// One entry returned by [`SkillRegistry::list_for_tool`].
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct SkillListEntry {
    /// Skill name.
    pub name: String,
    /// When to use the skill.
    pub description: String,
    /// Where it was loaded from (`bundled`, `project`, `external`, …).
    pub source: String,
    /// Sub-files inside the skill bundle, relative to its root.
    pub assets: Vec<String>,
}

/// Result of a `skill_view` call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkillView {
    /// Level 1: full `SKILL.md` body (or the first `max_chars` of it).
    Body {
        /// Body text returned to the model.
        body: String,
        /// `true` if the body was truncated to `max_chars`.
        truncated: bool,
        /// Total body length in chars (before truncation).
        total_chars: usize,
    },
    /// Level 2: a single sub-file.
    Subfile {
        /// The sub-file path that was read (relative to the skill root).
        path: String,
        /// The file contents.
        content: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    /// Build a unique temp dir for a test.
    fn fresh_dir(label: &str) -> PathBuf {
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("mewcode-skills-{label}-{n}-{nanos}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Write a `SKILL.md` and return the skill directory.
    fn write_skill(parent: &Path, name: &str, description: &str) -> PathBuf {
        let dir = parent.join(name);
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("SKILL.md"),
            format!("---\nname: {name}\ndescription: {description}\n---\n\nbody of {name}\n"),
        )
        .unwrap();
        dir
    }

    #[test]
    fn load_discovers_project_skills() {
        let project = fresh_dir("project");
        let project_skills = project.join(PROJECT_SKILLS_DIR);
        std::fs::create_dir_all(&project_skills).unwrap();
        write_skill(&project_skills, "alpha", "first skill");
        write_skill(&project_skills, "beta", "second skill");

        let cfg = SkillLoadConfig {
            bundled_dir: None,
            external_dirs: vec![],
            project_search_start: Some(project),
            include_dev_dir: false,
        };
        let reg = SkillRegistry::load(&cfg);
        assert_eq!(reg.len(), 2);
        assert!(reg.get("alpha").is_some());
        assert!(reg.get("beta").is_some());
    }

    #[test]
    fn project_shadows_external() {
        let external = fresh_dir("external");
        let project = fresh_dir("project");
        write_skill(&external, "shared", "from external");

        let project_skills = project.join(PROJECT_SKILLS_DIR);
        std::fs::create_dir_all(&project_skills).unwrap();
        write_skill(&project_skills, "shared", "from project");

        let cfg = SkillLoadConfig {
            bundled_dir: None,
            external_dirs: vec![external],
            project_search_start: Some(project),
            include_dev_dir: false,
        };
        let reg = SkillRegistry::load(&cfg);
        let loaded = reg.get("shared").unwrap();
        assert_eq!(loaded.skill.description, "from project");
        assert_eq!(loaded.source, SkillSource::Project);
    }

    #[test]
    fn external_shadows_bundled() {
        let bundled = fresh_dir("bundled");
        let external = fresh_dir("external");
        write_skill(&bundled, "shared", "from bundled");
        write_skill(&external, "shared", "from external");

        let cfg = SkillLoadConfig {
            bundled_dir: Some(bundled),
            external_dirs: vec![external],
            project_search_start: None,
            include_dev_dir: false,
        };
        let reg = SkillRegistry::load(&cfg);
        let loaded = reg.get("shared").unwrap();
        assert_eq!(loaded.skill.description, "from external");
        assert_eq!(loaded.source, SkillSource::External);
    }

    #[test]
    fn missing_paths_are_silently_skipped() {
        let cfg = SkillLoadConfig {
            bundled_dir: Some(PathBuf::from("/this/path/does/not/exist/anywhere")),
            external_dirs: vec![PathBuf::from("/also/not/here")],
            project_search_start: None,
            include_dev_dir: false,
        };
        let reg = SkillRegistry::load(&cfg);
        // No crash; registry is empty.
        assert!(reg.is_empty());
        // 2 missing: bundled + the global `~/.config/mewcode/skills` (which
        // also may or may not exist on a test machine). The contract is
        // "non-existent paths are silently skipped", not "exactly N
        // missing", so we assert at-least-2.
        assert!(
            reg.missing_paths().len() >= 2,
            "expected >= 2 missing paths, got {}",
            reg.missing_paths().len()
        );
    }

    #[test]
    fn view_subfile_returns_subfile() {
        let project = fresh_dir("subfile");
        let project_skills = project.join(PROJECT_SKILLS_DIR);
        std::fs::create_dir_all(&project_skills).unwrap();
        let dir = write_skill(&project_skills, "alpha", "alpha skill");
        let refs = dir.join("references");
        fs::create_dir_all(&refs).unwrap();
        fs::write(refs.join("checklist.md"), "- step 1\n- step 2\n").unwrap();

        let cfg = SkillLoadConfig {
            bundled_dir: None,
            external_dirs: vec![],
            project_search_start: Some(project),
            include_dev_dir: false,
        };
        let reg = SkillRegistry::load(&cfg);

        let view = reg
            .view("alpha", Some("references/checklist.md"), 10_000)
            .unwrap();
        match view {
            SkillView::Subfile { path, content } => {
                assert_eq!(path, "references/checklist.md");
                assert!(content.contains("step 1"));
            }
            other => panic!("expected Subfile, got {other:?}"),
        }
    }

    #[test]
    fn view_subfile_rejects_traversal() {
        let project = fresh_dir("traverse");
        let project_skills = project.join(PROJECT_SKILLS_DIR);
        std::fs::create_dir_all(&project_skills).unwrap();
        write_skill(&project_skills, "alpha", "alpha skill");

        let cfg = SkillLoadConfig {
            bundled_dir: None,
            external_dirs: vec![],
            project_search_start: Some(project),
            include_dev_dir: false,
        };
        let reg = SkillRegistry::load(&cfg);

        let err = reg
            .view("alpha", Some("../escape.md"), 10_000)
            .expect_err("must reject ..");
        assert!(matches!(err, SkillError::InvalidSubpath { .. }));
    }

    #[test]
    fn list_for_tool_returns_sorted_entries() {
        let project = fresh_dir("list");
        let project_skills = project.join(PROJECT_SKILLS_DIR);
        std::fs::create_dir_all(&project_skills).unwrap();
        write_skill(&project_skills, "zulu", "z");
        write_skill(&project_skills, "alpha", "a");

        let cfg = SkillLoadConfig {
            bundled_dir: None,
            external_dirs: vec![],
            project_search_start: Some(project),
            include_dev_dir: false,
        };
        let reg = SkillRegistry::load(&cfg);
        let entries = reg.list_for_tool();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].name, "alpha");
        assert_eq!(entries[1].name, "zulu");
    }
}
