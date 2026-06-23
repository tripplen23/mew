//! `grep` tool — search file contents with a regex, respecting `.gitignore`.

use async_trait::async_trait;
use mewcode_protocol::tool::names;
use mewcode_protocol::{
    ToolAnnotations, ToolContracts, ToolDescriptor, ToolError, ToolExample, ToolOutput,
};
use regex::Regex;
use serde_json::{Value, json};

use crate::tools::ProjectContext;

/// Maximum number of matches before results are truncated.
const MAX_MATCHES: usize = 200;

/// `grep` tool.
pub struct GrepTool {
    ctx: ProjectContext,
}

impl GrepTool {
    /// Build the tool against a project context.
    pub fn new(ctx: ProjectContext) -> Self {
        Self { ctx }
    }
}

#[async_trait]
impl ToolContracts for GrepTool {
    fn name(&self) -> &'static str {
        names::GREP
    }

    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            name: names::GREP.to_string(),
            description: "Search file contents inside the project root using a regex pattern. Respects `.gitignore` and skips binary files. Results include file path, line number, and the matching line.

**When to use:** When you need to find where a string, function, or pattern appears in the codebase. Prefer this over reading files and searching manually.

**When NOT to use:** For finding files by name, use `glob` instead. For reading a specific file, use `read_file`.

**Pattern syntax:** Regex pattern. Lookarounds and backreferences are not supported."
                .to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "pattern": {
                        "type": "string",
                        "description": "Regex pattern to search for."
                    },
                    "path": {
                        "type": "string",
                        "default": ".",
                        "description": "Directory to search from, relative to the project root."
                    },
                    "max_results": {
                        "type": "integer",
                        "default": 200,
                        "description": "Maximum number of matches to return."
                    }
                },
                "required": ["pattern"],
                "additionalProperties": false,
            }),
            annotations: ToolAnnotations::READ_ONLY_IDEMPOTENT,
            examples: vec![
                ToolExample {
                    description: "Find all function definitions.".to_string(),
                    input: json!({ "pattern": "fn \\w+\\(" }),
                },
                ToolExample {
                    description: "Case-insensitive search in a subdirectory.".to_string(),
                    input: json!({
                        "pattern": "(?i)todo",
                        "path": "src"
                    }),
                },
            ],
            max_response_chars: 50_000,
        }
    }

    async fn execute(&self, input: Value) -> Result<ToolOutput, ToolError> {
        let pattern = input
            .get("pattern")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                ToolError::invalid_input("missing `pattern`", "pass a string `pattern` field")
            })?;
        let path = input.get("path").and_then(|v| v.as_str()).unwrap_or(".");
        let max_results = input
            .get("max_results")
            .and_then(|v| v.as_u64())
            .map(|n| n as usize)
            .unwrap_or(MAX_MATCHES);

        let resolved =
            mewcode_protocol::tool::resolve_inside_root(&self.ctx.root, std::path::Path::new(path))
                .map_err(|e| ToolError::Rejected {
                    message: e.to_string(),
                    hint: Some("paths must stay inside the project root".into()),
                })?;

        let re = Regex::new(pattern).map_err(|e| {
            ToolError::invalid_input(
                format!("invalid regex: {e}"),
                "check the pattern syntax at https://docs.rs/regex",
            )
        })?;

        let mut matches: Vec<Value> = Vec::new();
        let mut truncated = false;

        let walker = ignore::WalkBuilder::new(&resolved)
            .hidden(false)
            .git_ignore(true)
            .require_git(false)
            .build();

        for entry in walker {
            let Ok(entry) = entry else { continue };
            if !entry.file_type().is_some_and(|t| t.is_file()) {
                continue;
            }

            // Skip binary files — a failed read_to_string means non-UTF-8.
            let Ok(content) = std::fs::read_to_string(entry.path()) else {
                continue;
            };

            let rel = entry
                .path()
                .strip_prefix(&resolved)
                .unwrap_or(entry.path())
                .to_string_lossy()
                .to_string();

            for (line_num, line) in content.lines().enumerate() {
                if re.is_match(line) {
                    let truncated_line = if line.len() > 200 {
                        format!("{}… [truncated]", &line[..200])
                    } else {
                        line.to_string()
                    };
                    matches.push(json!({
                        "file": rel,
                        "line": line_num + 1,
                        "content": truncated_line,
                    }));
                    if matches.len() >= max_results {
                        truncated = true;
                        break;
                    }
                }
            }
            if truncated {
                break;
            }
        }

        Ok(ToolOutput(json!({
            "pattern": pattern,
            "path": path,
            "matches": matches,
            "match_count": matches.len(),
            "truncated": truncated,
        })))
    }
}
