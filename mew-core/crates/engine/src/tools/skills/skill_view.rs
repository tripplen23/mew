//! `skill_view` tool — Level 1 (full `SKILL.md` body) and Level 2
//! (one sub-file). Replaces the previous `use_skill` tool, which
//! only supported Level 1.
//!
//! Without a `path`, returns the skill body truncated to
//! `DEFAULT_MAX_RESPONSE_CHARS` (with a marker so the model knows
//! more is available). With a `path`, returns one sub-file relative
//! to the skill root. The path is sandboxed to the skill directory
//! and cannot escape it.

use async_trait::async_trait;
use mewcode_protocol::{
    DEFAULT_MAX_RESPONSE_CHARS, SkillError, ToolAnnotations, ToolContracts, ToolDescriptor,
    ToolError, ToolExample, ToolOutput, truncate_with_marker,
};
use serde_json::{Value, json};

use crate::tools::Skills;

/// `skill_view` tool.
pub struct SkillViewTool {
    skills: Skills,
}

impl SkillViewTool {
    /// Build the tool against the engine's skill registry.
    pub fn new(skills: Skills) -> Self {
        Self { skills }
    }
}

#[async_trait]
impl ToolContracts for SkillViewTool {
    fn name(&self) -> &'static str {
        "skill_view"
    }

    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            name: "skill_view".to_string(),
            description: "Load a skill's instructions, or one of its sub-files, into your context. Implements progressive disclosure: Level 1 reads the full SKILL.md body; Level 2 reads a single sub-file (e.g. `references/checklist.md`, `scripts/build.sh`).

**When to use (Level 1, no `path`):** The user's request matches a skill's description in the system prompt. Load the skill first, then follow its instructions.

**When to use (Level 2, with `path`):** The skill's body tells you to read a specific sub-file, or `skills_list` shows a sub-file you need.

**When NOT to use:** If the request does not match any installed skill, do not invent a name. Skill names must come from the `<skills>` section of the system prompt or the `skills_list` tool."
                .to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "name": {
                        "type": "string",
                        "description": "Skill name, exactly as listed in the system prompt catalog or skills_list output."
                    },
                    "path": {
                        "type": "string",
                        "description": "Optional. Path to a sub-file inside the skill directory, relative to the skill root. Use `skills_list` to see available sub-files. Must not contain `..` or absolute paths."
                    }
                },
                "required": ["name"],
                "additionalProperties": false,
            }),
            annotations: ToolAnnotations::READ_ONLY,
            examples: vec![
                ToolExample {
                    description: "Load the full body of the review-pr skill.".to_string(),
                    input: json!({ "name": "review-pr" }),
                },
                ToolExample {
                    description: "Load a single sub-file from the review-pr skill.".to_string(),
                    input: json!({ "name": "review-pr", "path": "references/checklist.md" }),
                },
            ],
            max_response_chars: DEFAULT_MAX_RESPONSE_CHARS,
        }
    }

    async fn execute(&self, input: Value) -> Result<ToolOutput, ToolError> {
        let name = input
            .get("name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                ToolError::invalid_input("missing `name`", "pass a string `name` field")
            })?
            .to_string();
        let path = input.get("path").and_then(|v| v.as_str());

        match path {
            None => {
                let body = self.skills.view_body(&name).map_err(|e| match e {
                    SkillError::NotFound { .. } => ToolError::Rejected {
                        message: format!("no skill named '{name}' is installed"),
                        hint: Some(
                            "check the `<skills>` section of the system prompt or call skills_list() to see valid names"
                                .into(),
                        ),
                    },
                    other => ToolError::Other {
                        message: other.to_string(),
                        hint: Some("see the skill error type for the underlying cause".into()),
                    },
                })?;
                let truncated = body.chars().count() > DEFAULT_MAX_RESPONSE_CHARS;
                let body_out = if truncated {
                    truncate_with_marker(body, DEFAULT_MAX_RESPONSE_CHARS)
                } else {
                    body.to_string()
                };
                Ok(ToolOutput(json!({
                    "name": name,
                    "level": 1,
                    "body": body_out,
                    "truncated": truncated,
                    "instruction": "follow the skill's instructions above to complete the user's request",
                })))
            }
            Some(rel) => {
                let (resolved, content) = self.skills.view_subfile(&name, rel).map_err(|e| {
                    match e {
                        SkillError::NotFound { .. } => ToolError::Rejected {
                            message: format!("no skill named '{name}' is installed"),
                            hint: Some(
                                "check the `<skills>` section of the system prompt or call skills_list() to see valid names"
                                    .into(),
                            ),
                        },
                        SkillError::InvalidSubpath { path, reason } => ToolError::Rejected {
                            message: format!("invalid `path` '{path}': {reason}"),
                            hint: Some(
                                "the path must be relative to the skill root and must not contain `..`"
                                    .into(),
                            ),
                        },
                        SkillError::Read { path, source } => ToolError::Rejected {
                            message: format!("could not read {}: {}", path.display(), source),
                            hint: Some(
                                "call skills_list() to confirm the path exists; the sub-file may have been removed"
                                    .into(),
                            ),
                        },
                        other => ToolError::Other {
                            message: other.to_string(),
                            hint: Some("see the skill error type for the underlying cause".into()),
                        },
                    }
                })?;
                Ok(ToolOutput(json!({
                    "name": name,
                    "level": 2,
                    "path": resolved.to_string_lossy(),
                    "content": content,
                })))
            }
        }
    }
}
