//! Skills.
//!
//! A skill is a Markdown document the model reads on demand, packaged
//! with a name and a short description. Following Anthropic's Skills
//! guide, a skill lives in its own directory and starts with a
//! `SKILL.md` whose YAML frontmatter names it and tells the model when
//! to use it. The body is loaded into context only when invoked.
//!
//! ## Progressive disclosure (Anthropic Skills guide, Hermes pattern)
//!
//! | Level | What is loaded | Cost |
//! |-------|----------------|------|
//! | L0 | name + description (system-prompt catalog) | ~80 bytes per skill |
//! | L1 | full `SKILL.md` body (via `skill_view`) | varies; truncated |
//! | L2 | one sub-file (via `skill_view(name, path)`) | bounded by file size |
//!
//! The L0 list is the only thing that ships in the system prompt; L1 and
//! L2 are tool calls so the model pays the per-skill cost only when it
//! actually needs that skill.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Filename every skill bundle must contain.
pub const SKILL_FILE: &str = "SKILL.md";

/// Subdirectory (under `~/.config/mewcode/`) for globally-installed skills.
pub const GLOBAL_SKILLS_DIR: &str = "skills";

/// Subdirectory (under the project root) for per-project skills.
pub const PROJECT_SKILLS_DIR: &str = ".mewcode/skills";

/// Conventional sub-folder names a skill bundle may contain. The engine
/// does not interpret these — they are exposed via `skill_view(name,
/// path)` so the model can navigate them on demand.
pub const CONVENTIONAL_SUBDIRS: &[&str] = &["references", "scripts", "assets", "templates"];

/// A single skill.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Skill {
    /// Machine name, kebab-case.
    pub name: String,
    /// When the model should use this skill. The only field the model sees
    /// before invoking it.
    pub description: String,
    /// Full prompt body, loaded only when the skill is invoked.
    pub body: String,
    /// Where the skill was loaded from on disk.
    pub location: PathBuf,
    /// Other files in the skill directory (scripts, references, assets).
    /// Not auto-loaded into the prompt; the engine may expose them via tools.
    #[serde(default)]
    pub assets: Vec<PathBuf>,
}

impl Skill {
    /// Build a skill from a directory containing a `SKILL.md` file.
    pub fn load_from_dir(dir: &Path) -> Result<Self, SkillError> {
        let path = dir.join(SKILL_FILE);
        let raw = std::fs::read_to_string(&path).map_err(|e| SkillError::Read {
            path: path.clone(),
            source: e,
        })?;
        let mut skill = parse_skill_md(&raw, &path)?;
        skill.location = dir.to_path_buf();
        skill.assets = list_skill_assets(dir, &path);
        Ok(skill)
    }

    /// Render the **compact** catalog entry (name + description) for the
    /// system prompt. The body is intentionally omitted. This is the L0
    /// progressive-disclosure entry: small enough to keep ~30 skills
    /// under 3 kB of prompt.
    pub fn catalog_entry(&self) -> String {
        format!("- **{}** — {}", self.name, self.description)
    }
}

/// Read a sub-file from a skill directory by its path relative to the
/// skill root. The path is sandboxed inside the skill directory and
/// cannot escape it (e.g. `..` segments are rejected).
///
/// Used by the `skill_view` tool's Level 2 path. Returns the file
/// contents as a string along with the canonical relative path that
/// was read (handy for the model to confirm what it got).
pub fn read_skill_subfile(
    skill_root: &Path,
    relative_path: &str,
) -> Result<(PathBuf, String), SkillError> {
    let rel = Path::new(relative_path);
    if rel.is_absolute() {
        return Err(SkillError::InvalidSubpath {
            path: relative_path.into(),
            reason: "must be relative to the skill root".into(),
        });
    }
    if rel.components().any(|c| matches!(c, std::path::Component::ParentDir)) {
        return Err(SkillError::InvalidSubpath {
            path: relative_path.into(),
            reason: "must not contain `..` segments".into(),
        });
    }
    let resolved = skill_root.join(rel);
    let canonical = std::fs::canonicalize(&resolved).map_err(|e| SkillError::Read {
        path: resolved.clone(),
        source: e,
    })?;
    let canon_root = std::fs::canonicalize(skill_root).map_err(|e| SkillError::Read {
        path: skill_root.to_path_buf(),
        source: e,
    })?;
    if !canonical.starts_with(&canon_root) {
        return Err(SkillError::InvalidSubpath {
            path: relative_path.into(),
            reason: "resolved path escapes the skill root".into(),
        });
    }
    let content = std::fs::read_to_string(&canonical).map_err(|e| SkillError::Read {
        path: canonical.clone(),
        source: e,
    })?;
    Ok((rel.to_path_buf(), content))
}

/// List the asset files in a skill directory (everything except the
/// `SKILL.md`). Returned paths are relative to the skill root so the
/// catalog can ship them to the model as plain strings.
fn list_skill_assets(skill_dir: &Path, skill_md: &Path) -> Vec<PathBuf> {
    let mut assets = Vec::new();
    let Ok(entries) = std::fs::read_dir(skill_dir) else {
        return assets;
    };
    for entry in entries.flatten() {
        let p = entry.path();
        if p == skill_md {
            continue;
        }
        // Walk into conventional sub-folders (references/, scripts/, …)
        // so the catalog can advertise the full tree.
        if p.is_dir() {
            collect_files_recursive(&p, &p, &mut assets);
        } else if p.is_file() {
            if let Ok(rel) = p.strip_prefix(skill_dir) {
                assets.push(rel.to_path_buf());
            }
        }
    }
    assets.sort();
    assets
}

fn collect_files_recursive(root: &Path, dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let p = entry.path();
        if p.is_dir() {
            collect_files_recursive(root, &p, out);
        } else if p.is_file() {
            if let Ok(rel) = p.strip_prefix(root) {
                out.push(rel.to_path_buf());
            }
        }
    }
}

/// Errors that can occur while loading a skill.
#[derive(Debug, thiserror::Error)]
pub enum SkillError {
    /// `SKILL.md` could not be read.
    #[error("could not read skill file {path}: {source}")]
    Read {
        /// The file we tried to read.
        path: PathBuf,
        /// The underlying I/O error.
        #[source]
        source: std::io::Error,
    },
    /// The `SKILL.md` frontmatter is missing or malformed.
    #[error("malformed frontmatter in {path}: {message}")]
    MalformedFrontmatter {
        /// The file with the bad frontmatter.
        path: PathBuf,
        /// What was wrong.
        message: String,
    },
    /// A required field is missing.
    #[error("missing required field {field} in {path}")]
    MissingField {
        /// The file missing the field.
        path: PathBuf,
        /// Which field is missing.
        field: &'static str,
    },
    /// The `skill_view` path argument was rejected.
    #[error("invalid skill subpath '{path}': {reason}")]
    InvalidSubpath {
        /// The path the model tried to read.
        path: String,
        /// Why it was rejected.
        reason: String,
    },
}

/// Parse a `SKILL.md` document. The expected layout is YAML frontmatter
/// between `---` markers, then a markdown body.
pub fn parse_skill_md(raw: &str, path: &Path) -> Result<Skill, SkillError> {
    let (frontmatter, body) =
        split_frontmatter(raw).ok_or_else(|| SkillError::MalformedFrontmatter {
            path: path.to_path_buf(),
            message: "expected `---` markers around YAML frontmatter at the top of the file".into(),
        })?;

    let parsed: Frontmatter =
        serde_yaml::from_str(frontmatter).map_err(|e| SkillError::MalformedFrontmatter {
            path: path.to_path_buf(),
            message: format!("YAML parse error: {e}"),
        })?;

    let name = parsed.name.ok_or_else(|| SkillError::MissingField {
        path: path.to_path_buf(),
        field: "name",
    })?;
    let description = parsed.description.ok_or_else(|| SkillError::MissingField {
        path: path.to_path_buf(),
        field: "description",
    })?;

    Ok(Skill {
        name,
        description,
        body: body.trim().to_string(),
        location: PathBuf::new(),
        assets: Vec::new(),
    })
}

#[derive(Debug, Default, Deserialize)]
struct Frontmatter {
    name: Option<String>,
    description: Option<String>,
    #[serde(default)]
    #[allow(dead_code)]
    version: Option<String>,
}

/// Split a `SKILL.md` document into frontmatter and body. Returns
/// `(frontmatter, body)` if both `---` markers are present.
fn split_frontmatter(raw: &str) -> Option<(&str, &str)> {
    let trimmed = raw.trim_start_matches('\u{feff}');
    let rest = trimmed.strip_prefix("---")?;
    let rest = rest.trim_start_matches(['\r', '\n']);
    let end = rest.find("\n---")?;
    let frontmatter = &rest[..end];
    let after = &rest[end + 4..];
    let body = after.trim_start_matches(['\r', '\n']);
    Some((frontmatter, body))
}
