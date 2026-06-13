//! Skills.
//!
//! A skill is a Markdown document the model reads on demand, packaged
//! with a name and a short description. Following Anthropic's Skills
//! guide, a skill lives in its own directory and starts with a
//! `SKILL.md` whose YAML frontmatter names it and tells the model when
//! to use it. The body is loaded into context only when invoked.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Filename every skill bundle must contain.
pub const SKILL_FILE: &str = "SKILL.md";

/// Subdirectory (under `~/.config/mewcode/`) for globally-installed skills.
pub const GLOBAL_SKILLS_DIR: &str = "skills";

/// Subdirectory (under the project root) for per-project skills.
pub const PROJECT_SKILLS_DIR: &str = ".mewcode/skills";

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

        let mut assets = Vec::new();
        let entries = std::fs::read_dir(dir).map_err(|e| SkillError::Read {
            path: dir.to_path_buf(),
            source: e,
        })?;
        for entry in entries.flatten() {
            let p = entry.path();
            if p == path {
                continue;
            }
            if p.is_file() {
                assets.push(p);
            }
        }
        assets.sort();
        skill.assets = assets;
        Ok(skill)
    }

    /// Render the catalog entry (name + description) for the system
    /// prompt. The body is intentionally omitted.
    pub fn catalog_entry(&self) -> String {
        format!("- **{}** — {}\n  _invoke with: `use_skill(\"{}\")`_", self.name, self.description, self.name)
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
}

/// Parse a `SKILL.md` document. The expected layout is YAML frontmatter
/// between `---` markers, then a markdown body.
pub fn parse_skill_md(raw: &str, path: &Path) -> Result<Skill, SkillError> {
    let (frontmatter, body) = split_frontmatter(raw).ok_or_else(|| {
        SkillError::MalformedFrontmatter {
            path: path.to_path_buf(),
            message: "expected `---` markers around YAML frontmatter at the top of the file".into(),
        }
    })?;

    let parsed: Frontmatter = serde_yaml::from_str(frontmatter).map_err(|e| {
        SkillError::MalformedFrontmatter {
            path: path.to_path_buf(),
            message: format!("YAML parse error: {e}"),
        }
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
