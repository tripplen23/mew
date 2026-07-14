//! Skill runtime tools: the bridge that lets the agent load skill
//! bodies and sub-files on demand. Implements the progressive-
//! disclosure pattern from the Anthropic Skills guide and the
//! Hermes / agentskills.io open standard.
//!
//! Kept separate from [`crate::skills`] because this module is the
//! *tool-facing* wrapper, while [`crate::skills`] owns the catalog
//! and parsing logic.

pub mod skill_view;
pub mod skills_list;

pub use skill_view::SkillViewTool;
pub use skills_list::SkillsListTool;
