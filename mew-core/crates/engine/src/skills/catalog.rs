//! L0 catalog: render the registry as a system-prompt block and as
//! a tool-call JSON list.

use std::fmt::Write as _;

use super::SkillRegistry;

/// One entry returned by [`SkillRegistry::list_for_tool`].
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct SkillListEntry {
    /// Skill name.
    pub name: String,
    /// When to use the skill.
    pub description: String,
    /// Where it was loaded from (`bundled`, `project`, `external`, …).
    pub source: &'static str,
    /// Sub-files inside the skill bundle, relative to its root.
    pub assets: Vec<String>,
}

impl SkillRegistry {
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
        let mut out = String::new();
        out.push_str(catalog_header());
        for loaded in self.skills() {
            let _ = writeln!(out, "{}", loaded.skill.catalog_entry());
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
                source: loaded.source.label(),
                assets: loaded
                    .skill
                    .assets
                    .iter()
                    .map(|p| p.to_string_lossy().to_string())
                    .collect(),
            })
            .collect()
    }
}

/// Static L0 catalog header: heading plus the tool-call cheat sheet
/// that tells the model how to read a skill body or sub-file.
fn catalog_header() -> &'static str {
    "

## Available skills

The following skills are installed. Each is a bundle of specialised instructions for a particular kind of task. To read a skill's full instructions before proceeding, call `skill_view(name=\"<name>\")`. To read a sub-file (e.g. `references/foo.md`), call `skill_view(name=\"<name>\", path=\"references/foo.md\")`. Do not invent skill names — only use the ones listed below.

"
}
