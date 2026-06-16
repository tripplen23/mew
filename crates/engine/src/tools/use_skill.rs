//! `use_skill` tool — load a skill's full prompt body into the model's
//! context. This is the *only* way the model can read a skill's body.

use async_trait::async_trait;
use mewcode_protocol::{
    ToolAnnotations, ToolContracts, ToolDescriptor, ToolError, ToolExample, ToolOutput,
};
use serde_json::{Value, json};

use super::Skills;

/// `use_skill` tool.
pub struct UseSkillTool {
    skills: Skills,
}

impl UseSkillTool {
    /// Build the tool against the engine's skill registry.
    pub fn new(skills: Skills) -> Self {
        Self { skills }
    }
}

#[async_trait]
impl ToolContracts for UseSkillTool {
    fn name(&self) -> &'static str {
        "use_skill"
    }

    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            name: "use_skill".to_string(),
            description: concat!(
                "Load the full prompt body of a named skill into your context, then proceed ",
                "with the user's request following the skill's instructions.\n\n",
                "**When to use:** When the user's request matches a skill's description in the ",
                "system prompt. The skill body is loaded *only* into this tool's response — it ",
                "does not persist in your system prompt.\n\n",
                "**When NOT to use:** If the request does not match any installed skill, do not ",
                "invent a name. Skill names must come from the `Available skills` section of the ",
                "system prompt."
            )
            .to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "name": {
                        "type": "string",
                        "description": "Skill name, exactly as listed in the system prompt catalog."
                    }
                },
                "required": ["name"],
                "additionalProperties": false,
            }),
            annotations: ToolAnnotations::READ_ONLY,
            examples: vec![ToolExample {
                description: "Load a review skill before reviewing code.".to_string(),
                input: json!({ "name": "review-pr" }),
            }],
            max_response_chars: 100_000,
        }
    }

    async fn execute(&self, input: Value) -> Result<ToolOutput, ToolError> {
        let name = input.get("name").and_then(|v| v.as_str()).ok_or_else(|| {
            ToolError::invalid_input("missing `name`", "pass a string `name` field")
        })?;

        let body = self
            .skills
            .resolve_body(name)
            .map_err(|_| ToolError::Rejected {
                message: format!("no skill named '{name}' is installed"),
                hint: Some(
                    "check the `Available skills` section of the system prompt for valid names"
                        .into(),
                ),
            })?;

        Ok(ToolOutput(json!({
            "name": name,
            "body": body,
            "instruction": "follow the skill's instructions above to complete the user's request",
        })))
    }
}
