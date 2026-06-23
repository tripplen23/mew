//! `skills_list` tool — the Level 0 catalog. Returns the
//! `[{name, description, source, assets}, ...]` list so the model can
//! discover what skills are installed and what sub-files each one
//! ships with. The same data is also rendered in the system prompt as
//! a compact one-line-per-skill list, but this tool is the
//! authoritative JSON form the model can re-read on demand.

use async_trait::async_trait;
use mewcode_protocol::{
    ToolAnnotations, ToolContracts, ToolDescriptor, ToolError, ToolExample, ToolOutput,
};
use serde_json::{Value, json};

use crate::tools::Skills;

/// `skills_list` tool.
pub struct SkillsListTool {
    skills: Skills,
}

impl SkillsListTool {
    /// Build the tool against the engine's skill registry.
    pub fn new(skills: Skills) -> Self {
        Self { skills }
    }
}

#[async_trait]
impl ToolContracts for SkillsListTool {
    fn name(&self) -> &'static str {
        "skills_list"
    }

    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            name: "skills_list".to_string(),
            description: "List every installed skill with its name, description, source, and bundled sub-files. The same data also appears in the system prompt's `## Available skills` section, but you can call this tool to get the full JSON form (including each skill's `assets` list of sub-files) when you need to plan a multi-skill workflow.

**When to use:** Before calling `skill_view` with a `path`, to confirm the file exists. To audit which skills are bundled vs project vs external. To refresh your view of the catalog mid-task.

**When NOT to use:** The system prompt catalog is enough for ordinary single-skill invocation. Don't call this every turn — the catalog doesn't change."
                .to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {},
                "additionalProperties": false,
            }),
            annotations: ToolAnnotations::READ_ONLY,
            examples: vec![ToolExample {
                description: "List all installed skills.".to_string(),
                input: json!({}),
            }],
            max_response_chars: 10_000,
        }
    }

    async fn execute(&self, _input: Value) -> Result<ToolOutput, ToolError> {
        let entries = self.skills.list_for_tool();
        Ok(ToolOutput(json!({
            "skills": entries,
            "count": entries.len(),
        })))
    }
}
