//! `write_file` tool — create or overwrite a file inside the project root.

use async_trait::async_trait;
use mewcode_protocol::tool::names;
use mewcode_protocol::{
    ToolAnnotations, ToolContracts, ToolDescriptor, ToolError, ToolExample, ToolOutput,
};
use serde_json::{Value, json};

use crate::tools::ProjectContext;

/// `write_file` tool.
pub struct WriteFileTool {
    ctx: ProjectContext,
}

impl WriteFileTool {
    /// Build the tool against a project context.
    pub fn new(ctx: ProjectContext) -> Self {
        Self { ctx }
    }
}

#[async_trait]
impl ToolContracts for WriteFileTool {
    fn name(&self) -> &'static str {
        names::WRITE_FILE
    }

    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            name: names::WRITE_FILE.to_string(),
            description: "Create a new file or overwrite an existing one inside the project directory.

**When to use:** When you need to write a complete file's contents. Creates parent directories if they don't exist.

**Safety:** Refuses to escape the project root. Refuses to overwrite a non-empty file unless `overwrite: true` — this prevents accidental data loss. For targeted edits to existing files, prefer `edit_file` instead."
                .to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Path to the file, relative to the project root. Must not escape the root."
                    },
                    "content": {
                        "type": "string",
                        "description": "Full contents to write to the file."
                    },
                    "overwrite": {
                        "type": "boolean",
                        "default": false,
                        "description": "Set to `true` to overwrite a non-empty file. Defaults to `false` for safety."
                    }
                },
                "required": ["path", "content"],
                "additionalProperties": false,
            }),
            annotations: ToolAnnotations::WRITE_LOCAL,
            examples: vec![
                ToolExample {
                    description: "Create a new source file.".to_string(),
                    input: json!({
                        "path": "src/main.rs",
                        "content": "fn main() { println!(\"hello\"); }"
                    }),
                },
                ToolExample {
                    description: "Overwrite an existing file.".to_string(),
                    input: json!({
                        "path": "README.md",
                        "content": "# Updated README",
                        "overwrite": true
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
        let content = input
            .get("content")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                ToolError::invalid_input("missing `content`", "pass a string `content` field")
            })?;
        let overwrite = input
            .get("overwrite")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let resolved =
            mewcode_protocol::tool::resolve_inside_root(&self.ctx.root, std::path::Path::new(path))
                .map_err(|e| ToolError::Rejected {
                    message: e.to_string(),
                    hint: Some("paths must stay inside the project root".into()),
                })?;

        // Safety gate: refuse to overwrite a non-empty file unless the caller
        // explicitly passes `overwrite: true`. Empty files and non-existent
        // files are safe to write without the flag. We use `metadata.len()`
        // (not `read_to_string`) so binary files and unreadable files still
        // count as non-empty and remain protected by the safety gate.
        if resolved.exists() {
            let len = std::fs::metadata(&resolved).map(|m| m.len()).unwrap_or(0);
            if len > 0 && !overwrite {
                return Err(ToolError::Rejected {
                    message: format!(
                        "file '{}' already exists and is non-empty ({} bytes)",
                        path,
                        len
                    ),
                    hint: Some(
                        "set `overwrite: true` to replace the file, or use `edit_file` for targeted edits"
                            .into(),
                    ),
                });
            }
        }

        // Create parent directories if needed.
        if let Some(parent) = resolved.parent() {
            std::fs::create_dir_all(parent).map_err(ToolError::Io)?;
        }

        let bytes = content.len();
        std::fs::write(&resolved, content).map_err(ToolError::Io)?;

        Ok(ToolOutput(json!({
            "path": path,
            "bytes_written": bytes,
            "overwritten": overwrite,
        })))
    }
}
