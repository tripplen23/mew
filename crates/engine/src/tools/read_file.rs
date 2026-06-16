//! `read_file` tool — read the contents of a file inside the project root.

use async_trait::async_trait;
use mewcode_protocol::tool::names;
use mewcode_protocol::{
    ResponseFormat, ToolAnnotations, ToolContracts, ToolDescriptor, ToolError, ToolExample,
    ToolOutput,
};
use serde_json::{Value, json};

use super::ProjectContext;

/// `read_file` tool.
pub struct ReadFileTool {
    ctx: ProjectContext,
}

impl ReadFileTool {
    /// Build the tool against a project context.
    pub fn new(ctx: ProjectContext) -> Self {
        Self { ctx }
    }
}

#[async_trait]
impl ToolContracts for ReadFileTool {
    fn name(&self) -> &'static str {
        names::READ_FILE
    }

    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            name: names::READ_FILE.to_string(),
            description: concat!(
                "Read the contents of a file inside the project directory.\n\n",
                "**When to use:** When you need to see the actual code or text in a file. ",
                "Prefer this over `bash` with `cat` because it is sandboxed to the project root ",
                "and returns structured, truncated output.\n\n",
                "**When NOT to use:** Don't read files you have already read in this conversation. ",
                "For large files (> 100k chars) the response is truncated — use `grep` first to ",
                "find the relevant region, then `read_file` with a smaller `from_line`/`limit` scope ",
                "once Phase 10 lands range selection. For binary files, this tool returns an error.\n\n",
                "**Token efficiency:** The response is truncated to ~100k characters with a clear ",
                "marker. To see a specific section, prefer a follow-up `grep` rather than reading ",
                "the whole file again."
            )
            .to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Path to the file, relative to the project root. Must not escape the root."
                    },
                    "response_format": {
                        "type": "string",
                        "enum": ["concise", "detailed"],
                        "default": "concise",
                        "description": "`concise` returns just the file contents. `detailed` includes line numbers and a header."
                    }
                },
                "required": ["path"],
                "additionalProperties": false,
            }),
            annotations: ToolAnnotations::READ_ONLY_IDEMPOTENT,
            examples: vec![
                ToolExample {
                    description: "Read a small file at the project root.".to_string(),
                    input: json!({ "path": "Cargo.toml" }),
                },
                ToolExample {
                    description: "Read a deeply-nested source file with line numbers.".to_string(),
                    input: json!({
                        "path": "crates/server/src/routes/chat.rs",
                        "response_format": "detailed"
                    }),
                },
            ],
            max_response_chars: 100_000,
        }
    }

    async fn execute(&self, input: Value) -> Result<ToolOutput, ToolError> {
        let path = input.get("path").and_then(|v| v.as_str()).ok_or_else(|| {
            ToolError::invalid_input("missing `path`", "pass a string `path` field")
        })?;

        let response_format: ResponseFormat = input
            .get("response_format")
            .and_then(|v| v.as_str())
            .map(|s| match s {
                "detailed" => ResponseFormat::Detailed,
                _ => ResponseFormat::Concise,
            })
            .unwrap_or_default();

        let resolved =
            mewcode_protocol::tool::resolve_inside_root(&self.ctx.root, std::path::Path::new(path))
                .map_err(|e| ToolError::Rejected {
                    message: e.to_string(),
                    hint: Some("paths must stay inside the project root".into()),
                })?;

        let content = std::fs::read_to_string(&resolved).map_err(|e| {
            let hint: Option<String> = if e.kind() == std::io::ErrorKind::NotFound {
                Some("check the file exists; use `glob` to find files when uncertain".into())
            } else if e.kind() == std::io::ErrorKind::InvalidData {
                Some(
                    "the file is binary; try `grep` with a pattern, or read a different file"
                        .into(),
                )
            } else {
                None
            };
            match hint {
                Some(h) => ToolError::Rejected {
                    message: e.to_string(),
                    hint: Some(h),
                },
                None => ToolError::Io(e),
            }
        })?;

        let value = match response_format {
            ResponseFormat::Concise => json!({
                "path": path,
                "content": content,
            }),
            ResponseFormat::Detailed => {
                let numbered: String = content
                    .lines()
                    .enumerate()
                    .map(|(i, l)| format!("{:>4} │ {}", i + 1, l))
                    .collect::<Vec<_>>()
                    .join("\n");
                json!({
                    "path": path,
                    "line_count": content.lines().count(),
                    "byte_count": content.len(),
                    "content": numbered,
                })
            }
        };

        Ok(ToolOutput(value))
    }
}
