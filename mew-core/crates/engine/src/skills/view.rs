//! L1/L2 read paths: return the `SKILL.md` body, or one sub-file
//! inside the skill directory.

use std::path::PathBuf;

use mewcode_protocol::{SkillError, read_skill_subfile};

use super::SkillRegistry;

impl SkillRegistry {
    /// L1 read: return the full `SKILL.md` body. Truncation to the
    /// per-tool response budget is the caller's responsibility (the
    /// `skill_view` tool uses `truncate_with_marker` for that). The
    /// body is borrowed, not cloned — the registry is the source of
    /// truth and outlives the call.
    pub fn view_body(&self, name: &str) -> Result<&str, SkillError> {
        let loaded = self
            .skills
            .get(name)
            .ok_or_else(|| SkillError::NotFound { name: name.into() })?;
        Ok(loaded.skill.body.as_str())
    }

    /// L2 read: return one sub-file under the skill directory, sandboxed
    /// inside it.
    pub fn view_subfile(&self, name: &str, subpath: &str) -> Result<(PathBuf, String), SkillError> {
        let loaded = self
            .skills
            .get(name)
            .ok_or_else(|| SkillError::NotFound { name: name.into() })?;
        read_skill_subfile(&loaded.skill.location, subpath)
    }
}
