//! `mewcode_memory` tool — read, write, and list mewcode memory profiles.
//!
//! The tool wraps a [`MemoryStore`] so the agent can inspect and update its
//! own durable facts. The tool is *registered* in the tool registry but
//! won't be executable until the tool-calling loop lands.

use async_trait::async_trait;
use mewcode_protocol::tool::names;
use mewcode_protocol::{
    ToolAnnotations, ToolContracts, ToolDescriptor, ToolError, ToolExample, ToolOutput,
};
use serde_json::{Value, json};

use crate::memory::MemoryStore;

/// `mewcode_memory` tool.
pub struct MewcodeMemoryTool {
    store: MemoryStore,
}

impl MewcodeMemoryTool {
    /// Build the tool against a memory store.
    pub fn new(store: MemoryStore) -> Self {
        Self { store }
    }
}

#[async_trait]
impl ToolContracts for MewcodeMemoryTool {
    fn name(&self) -> &'static str {
        names::MEMORY
    }

    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            name: names::MEMORY.to_string(),
            description: "Read, write, and list mewcode persistent memory profiles. Memory holds durable facts about the user that the agent should remember across sessions.

**When to use:** Use `read` to see what you already know about the user. Use `write` to save a new fact (overwrites the entire memory file). Use `list` to see available profiles."
                .to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "action": {
                        "type": "string",
                        "enum": ["read", "write", "list"],
                        "description": "What to do with the memory store."
                    },
                    "content": {
                        "type": "string",
                        "description": "New memory content (required for `write`). Ignored for `read` and `list`."
                    },
                    "profile": {
                        "type": "string",
                        "description": "Optional profile name (default: 'default')."
                    }
                },
                "required": ["action"],
                "additionalProperties": false,
            }),
            // `write` mutates a file under `memories/`, so this is a
            // local writer — not read-only, not idempotent (overwrites).
            annotations: ToolAnnotations::WRITE_LOCAL,
            examples: vec![
                ToolExample {
                    description: "Read the active memory profile.".to_string(),
                    input: json!({ "action": "read" }),
                },
                ToolExample {
                    description: "Write a new fact to memory.".to_string(),
                    input: json!({ "action": "write", "content": "User prefers concise responses." }),
                },
                ToolExample {
                    description: "List available memory profiles.".to_string(),
                    input: json!({ "action": "list" }),
                },
            ],
            max_response_chars: 10_000,
        }
    }

    async fn execute(&self, input: Value) -> Result<ToolOutput, ToolError> {
        let action = input
            .get("action")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                ToolError::invalid_input("missing `action`", "pass one of: read, write, list")
            })?;

        let profile = input
            .get("profile")
            .and_then(|v| v.as_str())
            .unwrap_or("default");

        // Profile names become file paths under `memories/`, so they must
        // be simple identifiers — reject anything that could escape the
        // directory via path separators or `..` segments.
        if profile.is_empty()
            || profile.contains('/')
            || profile.contains('\\')
            || profile.contains("..")
            || profile.starts_with('.')
        {
            return Err(ToolError::invalid_input(
                format!("invalid profile name: {profile:?}"),
                "use a simple identifier like 'default' or 'work'",
            ));
        }

        let store = if profile != "default" {
            // Derive data_dir from store path by going up two levels:
            //   <data_dir>/memories/<profile>.md
            let parent = self.store.path().parent().and_then(|p| p.parent());
            match parent {
                Some(data_dir) => MemoryStore::with_profile(data_dir.to_path_buf(), profile),
                None => {
                    return Err(ToolError::invalid_input(
                        "cannot resolve data directory for custom profile",
                        "use 'default' profile or check memory store configuration",
                    ));
                }
            }
        } else {
            self.store.clone()
        };

        match action {
            "read" => {
                let content = store.read();
                Ok(ToolOutput(json!({
                    "profile": profile,
                    "content": content,
                })))
            }
            "write" => {
                let content = input
                    .get("content")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| {
                        ToolError::invalid_input(
                            "missing `content` for write action",
                            "pass a `content` string field",
                        )
                    })?;
                store.write(content).map_err(|e| {
                    ToolError::Io(std::io::Error::other(format!(
                        "failed to write memory: {e}"
                    )))
                })?;
                Ok(ToolOutput(json!({
                    "profile": profile,
                    "status": "written",
                })))
            }
            "list" => {
                // For now, just show available profiles by scanning the memories dir.
                let parent = self.store.path().parent().and_then(|p| p.parent());
                let profiles = match parent {
                    Some(data_dir) => {
                        let memories_dir = data_dir.join("memories");
                        let mut names: Vec<String> = Vec::new();
                        if let Ok(entries) = std::fs::read_dir(&memories_dir) {
                            for entry in entries.flatten() {
                                let path = entry.path();
                                if path
                                    .extension()
                                    .is_some_and(|e| e == std::ffi::OsStr::new("md"))
                                {
                                    if let Some(stem) = path.file_stem() {
                                        names.push(stem.to_string_lossy().to_string());
                                    }
                                }
                            }
                        }
                        names.sort();
                        names
                    }
                    None => vec!["default".to_string()],
                };
                Ok(ToolOutput(json!({
                    "profiles": profiles,
                })))
            }
            _ => Err(ToolError::invalid_input(
                format!("unknown action: {action}"),
                "use 'read', 'write', or 'list'",
            )),
        }
    }
}
