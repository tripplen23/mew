//! L0 catalog: render the registry as a system-prompt block and as
//! a tool-call JSON list.

use std::fmt::Write as _;

use super::{LoadedSkill, SkillRegistry};

/// Byte budget for the catalog's *entry list* (header excluded).
/// Approximate, not a hard cap on the whole `<skills>` block: the
/// `+N more` warning line and `</skills>` footer are appended after
/// budgeting, so the final block can slightly exceed this.
pub const CATALOG_BUDGET_CHARS: usize = 8_000;

/// Longest description kept verbatim; longer ones get `…` before
/// skills are dropped.
pub const MAX_CATALOG_DESCRIPTION_CHARS: usize = 120;

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
    /// Render the L0 system-prompt catalog ("" if none loaded, so
    /// callers can prepend unconditionally).
    ///
    /// One line per skill, no body. `disable-model-invocation` skills
    /// are excluded — the model must not know them. The entry list is
    /// budgeted at [`CATALOG_BUDGET_CHARS`] (byte length): overflow
    /// truncates descriptions, then drops skills (name-sorted,
    /// deterministic), then appends a `+N more` warning line.
    pub fn catalog_for_system_prompt(&self) -> String {
        let loaded: Vec<_> = self
            .skills()
            .into_iter()
            .filter(|l| !l.skill.disable_model_invocation)
            .collect();
        if loaded.is_empty() {
            return String::new();
        }
        let header = catalog_header();
        let budget = CATALOG_BUDGET_CHARS - header.len();
        let mut entries: Vec<String> = loaded.iter().map(|l| l.skill.catalog_entry()).collect();
        // Count the rendered newline per entry too — the render loop
        // below charges `len + 1` each. Mismatched preflight would
        // skip truncation and drop entries that only exceed the
        // budget once newlines are counted.
        if entries.iter().map(|entry| entry.len() + 1).sum::<usize>() > budget {
            entries = loaded
                .iter()
                .map(|l| {
                    l.skill
                        .catalog_entry_truncated(MAX_CATALOG_DESCRIPTION_CHARS)
                })
                .collect();
        }
        let mut out = String::new();
        out.push_str(header);
        let mut remaining = budget;
        let mut omitted = 0;
        for entry in entries {
            if entry.len() + 1 > remaining {
                omitted += 1;
                continue;
            }
            out.push_str(&entry);
            out.push('\n');
            remaining -= entry.len() + 1;
        }
        if omitted > 0 {
            let _ = writeln!(
                out,
                "… +{omitted} more skills installed. Run `skills_list()` for the full catalog."
            );
        }
        out.push_str("</skills>\n");
        out
    }

    /// L0 catalog for the `skills_list` tool (model-facing JSON).
    /// Returns `[{name, description, source, assets}, ...]`. Excludes
    /// skills with `disable-model-invocation: true`.
    pub fn list_for_tool(&self) -> Vec<SkillListEntry> {
        self.skills()
            .into_iter()
            .filter(|l| !l.skill.disable_model_invocation)
            .map(to_entry)
            .collect()
    }

    /// L0 catalog for the user-facing `/skills` picker (server route).
    /// Excludes skills with `user-invocable: false` — the user must
    /// not see skills only the model can invoke.
    pub fn list_for_user(&self) -> Vec<SkillListEntry> {
        self.skills()
            .into_iter()
            .filter(|l| l.skill.user_invocable)
            .map(to_entry)
            .collect()
    }
}

fn to_entry(loaded: &LoadedSkill) -> SkillListEntry {
    SkillListEntry {
        name: loaded.skill.name.clone(),
        description: loaded.skill.description.clone(),
        source: loaded.source.label(),
        assets: loaded
            .skill
            .assets
            .iter()
            .map(|p| p.to_string_lossy().to_string())
            .collect(),
    }
}

/// Static L0 catalog header: heading plus the tool-call cheat sheet
/// that tells the model how to read a skill body or sub-file.
fn catalog_header() -> &'static str {
    "
<skills>
The following skills are installed. Each is a bundle of specialised instructions for a particular kind of task. To read a skill's full instructions before proceeding, call `skill_view(name=\"<name>\")`. To read a sub-file (e.g. `references/foo.md`), call `skill_view(name=\"<name>\", path=\"references/foo.md\")`. Do not invent skill names — only use the ones listed below.

"
}
