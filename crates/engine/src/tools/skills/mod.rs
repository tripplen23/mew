//! Skill runtime tools: the bridge that lets the agent load skill
//! bodies and sub-files on demand. Implements the progressive-
//! disclosure pattern from the Anthropic Skills guide and the
//! Hermes / agentskills.io open standard.
//!
//! Two tools (not three — see Phase 13 in PHASES.md for the WHY):
//! - [`SkillsListTool`] — Level 0 catalog. The model can call this to
//!   re-read the catalog (it's already in the system prompt; this
//!   exists for cases where the catalog is large or off-context).
//! - [`SkillViewTool`] — Level 1 (no `path`): full body. Level 2
//!   (`path` set): one sub-file.
//!
//! Kept separate from [`crate::skills`] because this module is the
//! *tool-facing* wrapper, while [`crate::skills`] owns the catalog
//! and parsing logic.

pub mod skill_view;
pub mod skills_list;

pub use skill_view::SkillViewTool;
pub use skills_list::SkillsListTool;
