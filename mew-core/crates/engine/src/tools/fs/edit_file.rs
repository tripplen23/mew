//! `edit_file` tool — targeted string replacement in an existing file.

use async_trait::async_trait;
use mewcode_protocol::tool::names;
use mewcode_protocol::{
    DiffDisplay, ToolAnnotations, ToolContracts, ToolDescriptor, ToolDisplay, ToolError,
    ToolExample, ToolOutput,
};
use serde_json::{Value, json};

use crate::tools::ProjectContext;

/// `edit_file` tool.
pub struct EditFileTool {
    ctx: ProjectContext,
}

impl EditFileTool {
    /// Build the tool against a project context.
    pub fn new(ctx: ProjectContext) -> Self {
        Self { ctx }
    }
}

#[async_trait]
impl ToolContracts for EditFileTool {
    fn name(&self) -> &'static str {
        names::EDIT_FILE
    }

    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            name: names::EDIT_FILE.to_string(),
            description: "Make a single targeted text replacement in an existing file inside the project directory.

**When to use:** When you need to change a specific part of a file without rewriting the whole thing. Prefer this over `write_file` for small edits — it preserves the rest of the file and reports the exact byte range changed.

**Safety:** Refuses to edit a file that doesn't exist (use `write_file` for new files). Errors if `old_string` is not found or appears multiple times (ambiguous — include more context to make the match unique)."
                .to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Path to the file, relative to the project root. Must not escape the root."
                    },
                    "old_string": {
                        "type": "string",
                        "description": "The exact text to find in the file. Must be unique — if it appears multiple times the edit is rejected as ambiguous."
                    },
                    "new_string": {
                        "type": "string",
                        "description": "The replacement text. Set to an empty string to delete the matched text."
                    }
                },
                "required": ["path", "old_string", "new_string"],
                "additionalProperties": false,
            }),
            annotations: ToolAnnotations::WRITE_LOCAL,
            examples: vec![
                ToolExample {
                    description: "Replace a function name.".to_string(),
                    input: json!({
                        "path": "src/lib.rs",
                        "old_string": "fn old_name()",
                        "new_string": "fn new_name()"
                    }),
                },
                ToolExample {
                    description: "Delete a line by replacing it with nothing.".to_string(),
                    input: json!({
                        "path": "src/main.rs",
                        "old_string": "// TODO: fix this\n",
                        "new_string": ""
                    }),
                },
            ],
            max_response_chars: 1_000,
        }
    }

    async fn execute(&self, input: Value) -> Result<ToolOutput, ToolError> {
        let path = input.get("path").and_then(|v| v.as_str()).ok_or_else(|| {
            ToolError::invalid_input("missing `path`", "pass a string `path` field")
        })?;
        let old_string = input
            .get("old_string")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                ToolError::invalid_input("missing `old_string`", "pass a string `old_string` field")
            })?;
        if old_string.is_empty() {
            return Err(ToolError::invalid_input(
                "`old_string` must not be empty",
                "pass the exact text to replace",
            ));
        }
        let new_string = input
            .get("new_string")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                ToolError::invalid_input("missing `new_string`", "pass a string `new_string` field")
            })?;

        let resolved =
            mewcode_protocol::tool::resolve_inside_root(&self.ctx.root, std::path::Path::new(path))
                .map_err(|e| ToolError::Rejected {
                    message: e.to_string(),
                    hint: Some("paths must stay inside the project root".into()),
                })?;

        if !resolved.exists() {
            return Err(ToolError::Rejected {
                message: format!("file '{path}' does not exist"),
                hint: Some("use `write_file` to create new files".into()),
            });
        }

        let content = std::fs::read_to_string(&resolved).map_err(ToolError::Io)?;

        // Find the match. Reject if not found or if it appears multiple times
        // (ambiguous — the caller needs to include more surrounding context).
        let byte_start = match content.match_indices(old_string).collect::<Vec<_>>() {
            matches if matches.is_empty() => {
                return Err(ToolError::Rejected {
                    message: format!("`old_string` not found in '{path}'"),
                    hint: Some("check the exact text including whitespace and indentation".into()),
                });
            }
            matches if matches.len() > 1 => {
                return Err(ToolError::Rejected {
                    message: format!(
                        "`old_string` appears {} times in '{}' — edit is ambiguous",
                        matches.len(),
                        path
                    ),
                    hint: Some("include more surrounding context so the match is unique".into()),
                });
            }
            matches => matches[0].0,
        };

        let byte_end = byte_start + old_string.len();

        // Build the new content by splicing.
        let mut new_content =
            String::with_capacity(content.len() - old_string.len() + new_string.len());
        new_content.push_str(&content[..byte_start]);
        new_content.push_str(new_string);
        new_content.push_str(&content[byte_end..]);

        std::fs::write(&resolved, &new_content).map_err(ToolError::Io)?;

        // Report the 1-based line number where the edit starts.
        let line_number = content[..byte_start].lines().count() + 1;

        // Push diff to display sink.
        self.ctx.push_display(
            input.clone(),
            ToolDisplay::Diff(DiffDisplay::new(
                path,
                Some(line_number as u64),
                old_string,
                new_string,
            )),
        );

        Ok(ToolOutput(json!({
            "path": path,
            "bytes_replaced": old_string.len(),
            "bytes_inserted": new_string.len(),
            "start_byte": byte_start,
            "end_byte": byte_end,
            "start_line": line_number,
        })))
    }
}
