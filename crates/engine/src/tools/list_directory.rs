//! `list_directory` tool — list entries in a directory inside the project root.

use async_trait::async_trait;
use mewcode_protocol::tool::names;
use mewcode_protocol::{
    ToolAnnotations, ToolContracts, ToolDescriptor, ToolError, ToolExample, ToolOutput,
};
use serde_json::{json, Value};

use super::ProjectContext;

/// `list_directory` tool.
pub struct ListDirectoryTool {
    ctx: ProjectContext,
}

impl ListDirectoryTool {
    /// Build the tool against a project context.
    pub fn new(ctx: ProjectContext) -> Self {
        Self { ctx }
    }
}

#[async_trait]
impl ToolContracts for ListDirectoryTool {
    fn name(&self) -> &'static str {
        names::LIST_DIRECTORY
    }

    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            name: names::LIST_DIRECTORY.to_string(),
            description: concat!(
                "List entries in a directory inside the project root. Directories are listed first, ",
                "then files, both alphabetically. Hidden files (starting with `.`) and ",
                "`node_modules` are skipped.\n\n",
                "**When to use:** When you need to know what files or subdirectories exist. ",
                "Prefer this over running `ls` in `bash`.\n\n",
                "**When NOT to use:** For finding files by name, use `glob` instead — it is faster ",
                "and the result is sorted."
            )
            .to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "default": ".",
                        "description": "Directory to list, relative to the project root. Defaults to `.`."
                    }
                },
                "required": [],
                "additionalProperties": false,
            }),
            annotations: ToolAnnotations::READ_ONLY_IDEMPOTENT,
            examples: vec![ToolExample {
                description: "List the project root.".to_string(),
                input: json!({}),
            }],
            max_response_chars: 50_000,
        }
    }

    async fn execute(&self, input: Value) -> Result<ToolOutput, ToolError> {
        let path = input
            .get("path")
            .and_then(|v| v.as_str())
            .unwrap_or(".");
        let resolved = mewcode_protocol::tool::resolve_inside_root(&self.ctx.root, std::path::Path::new(path))
            .map_err(|e| ToolError::Rejected {
                message: e.to_string(),
                hint: Some("paths must stay inside the project root".into()),
            })?;

        let mut rd = tokio::fs::read_dir(&resolved).await.map_err(ToolError::Io)?;
        let mut dirs = Vec::new();
        let mut files = Vec::new();
        while let Some(entry) = rd.next_entry().await.map_err(ToolError::Io)? {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with('.') || name == "node_modules" {
                continue;
            }
            let meta = entry.metadata().await.map_err(ToolError::Io)?;
            let kind = if meta.is_dir() { "directory" } else { "file" };
            let entry = serde_json::json!({ "name": name, "kind": kind });
            if meta.is_dir() {
                dirs.push(entry);
            } else {
                files.push(entry);
            }
        }
        dirs.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
        files.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
        let mut all = dirs;
        all.extend(files);
        Ok(ToolOutput(json!({ "path": path, "entries": all })))
    }
}
