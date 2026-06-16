//! `glob` tool — find files matching a glob pattern, with a sensible cap.

use async_trait::async_trait;
use mewcode_protocol::tool::names;
use mewcode_protocol::{
    ToolAnnotations, ToolContracts, ToolDescriptor, ToolError, ToolExample, ToolOutput,
};
use serde_json::{Value, json};

use super::ProjectContext;

/// `glob` tool.
pub struct GlobTool {
    ctx: ProjectContext,
}

impl GlobTool {
    /// Build the tool against a project context.
    pub fn new(ctx: ProjectContext) -> Self {
        Self { ctx }
    }
}

#[async_trait]
impl ToolContracts for GlobTool {
    fn name(&self) -> &'static str {
        names::GLOB
    }

    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            name: names::GLOB.to_string(),
            description: concat!(
                "Find files inside the project root whose path matches a glob pattern. ",
                "`node_modules` and `.git` are skipped. Results are sorted and capped at 200.\n\n",
                "**When to use:** When you need to find files by name or extension. ",
                "Prefer this over recursive `ls` — it is O(n) once and returns a flat list.\n\n",
                "**Examples of good patterns:** `**/*.rs`, `crates/*/Cargo.toml`, `src/**/*.{ts,tsx}`."
            )
            .to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "pattern": {
                        "type": "string",
                        "description": "Glob pattern, e.g. `**/*.rs` or `crates/*/Cargo.toml`."
                    },
                    "path": {
                        "type": "string",
                        "default": ".",
                        "description": "Directory to search from, relative to the project root."
                    }
                },
                "required": ["pattern"],
                "additionalProperties": false,
            }),
            annotations: ToolAnnotations::READ_ONLY_IDEMPOTENT,
            examples: vec![
                ToolExample {
                    description: "Find every Rust source file.".to_string(),
                    input: json!({ "pattern": "**/*.rs" }),
                },
                ToolExample {
                    description: "Find Cargo.toml files in any crate.".to_string(),
                    input: json!({ "pattern": "crates/*/Cargo.toml" }),
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
        let resolved =
            mewcode_protocol::tool::resolve_inside_root(&self.ctx.root, std::path::Path::new(path))
                .map_err(|e| ToolError::Rejected {
                    message: e.to_string(),
                    hint: Some("paths must stay inside the project root".into()),
                })?;

        let glob = globset::Glob::new(pattern)
            .map_err(|e| {
                ToolError::invalid_input(
                    format!("invalid glob: {e}"),
                    "double-check the pattern syntax",
                )
            })?
            .compile_matcher();

        let mut files: Vec<String> = Vec::new();
        let mut walker = ignore::WalkBuilder::new(&resolved);
        walker.hidden(false).git_ignore(true).require_git(false);
        for entry in walker.build() {
            let Ok(entry) = entry else { continue };
            if !entry.file_type().is_some_and(|t| t.is_file()) {
                continue;
            }
            let rel = entry.path().strip_prefix(&resolved).unwrap_or(entry.path());
            if glob.is_match(rel) {
                files.push(rel.to_string_lossy().to_string());
                if files.len() >= 200 {
                    break;
                }
            }
        }
        files.sort();
        Ok(ToolOutput(json!({
            "pattern": pattern,
            "files": files,
            "truncated": files.len() >= 200,
        })))
    }
}
