//! Provenance of a loaded skill.

use mewcode_protocol::Skill;

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
    pub(super) fn new(skill: Skill, source: SkillSource) -> Self {
        Self { skill, source }
    }
}
