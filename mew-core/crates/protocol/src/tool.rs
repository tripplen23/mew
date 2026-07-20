//! Tool contracts and tool-name constants. Follows the design
//! principles from Anthropic's
//! "[Writing effective tools for agents](https://www.anthropic.com/engineering/writing-tools-for-agents)"
//! guide: snake_case names, prompt-engineered descriptions, safety
//! annotations, examples, token-efficient responses, actionable errors.

use std::path::Path;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::Mode;

/// Canonical tool name constants. Wire format the model sees.
pub mod names {
    /// Read the contents of a file.
    pub const READ_FILE: &str = "read_file";
    /// List entries in a directory.
    pub const LIST_DIRECTORY: &str = "list_directory";
    /// Find files matching a glob pattern.
    pub const GLOB: &str = "glob";
    /// Search file contents with a regex.
    pub const GREP: &str = "grep";
    /// Create or overwrite a file.
    pub const WRITE_FILE: &str = "write_file";
    /// Make a targeted text replacement in a file.
    pub const EDIT_FILE: &str = "edit_file";
    /// Run a shell command in the project directory.
    pub const BASH: &str = "bash";
    /// Read, write, and list mewcode memory profiles.
    pub const MEMORY: &str = "mewcode_memory";
}

/// Read-only tool set, available in both `Build` and `Plan` modes.
pub const READ_ONLY_TOOLS: &[&str] = &[
    names::READ_FILE,
    names::LIST_DIRECTORY,
    names::GLOB,
    names::GREP,
];

/// Full tool set, available only in `Build` mode.
pub const ALL_TOOLS: &[&str] = &[
    names::READ_FILE,
    names::LIST_DIRECTORY,
    names::GLOB,
    names::GREP,
    names::WRITE_FILE,
    names::EDIT_FILE,
    names::BASH,
];

/// Return the list of tool names allowed to execute in the given mode.
pub fn allowed_tools_for_mode(mode: Mode) -> &'static [&'static str] {
    if mode.allows_writes() {
        ALL_TOOLS
    } else {
        READ_ONLY_TOOLS
    }
}

/// Default cap on a single tool response, in characters (a tokenizer-free
/// proxy for ~25k tokens, the ceiling Claude Code uses for its own tools).
pub const DEFAULT_MAX_RESPONSE_CHARS: usize = 100_000;

/// Truncate `value` to `limit` characters, appending a clear marker so the
/// model knows it got a partial result and can ask for more.
pub fn truncate_with_marker(value: &str, limit: usize) -> String {
    if value.chars().count() <= limit {
        return value.to_string();
    }
    let truncated: String = value.chars().take(limit).collect();
    let total = value.chars().count();
    format!(
        "{truncated}\n\n… [truncated: {total} total chars, showing first {limit}. Re-call with a narrower path/pattern/limit to see more.]"
    )
}

/// Verbosity the model can request for a tool's response.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ResponseFormat {
    /// Default. Drop raw ids and surrounding context.
    #[default]
    Concise,
    /// Include metadata, ids, timestamps, and surrounding context.
    Detailed,
}

/// Safety annotations for a tool. Modelled after the MCP tool annotations
/// spec (<https://modelcontextprotocol.io/specification/2025-06-18/server/tools>).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolAnnotations {
    /// The tool does not modify project state.
    #[serde(default)]
    pub read_only: bool,
    /// The tool may make destructive changes worth confirming.
    #[serde(default)]
    pub destructive: bool,
    /// The tool talks to something outside the local project (network, etc.).
    #[serde(default)]
    pub open_world: bool,
    /// Repeated calls with the same input have the same effect as one.
    #[serde(default)]
    pub idempotent: bool,
}

impl ToolAnnotations {
    /// Read-only, idempotent, sandboxed — the safest possible tool.
    pub const READ_ONLY_IDEMPOTENT: Self = Self {
        read_only: true,
        destructive: false,
        open_world: false,
        idempotent: true,
    };

    /// Read-only, sandboxed but not necessarily idempotent.
    pub const READ_ONLY: Self = Self {
        read_only: true,
        destructive: false,
        open_world: false,
        idempotent: false,
    };

    /// Mutates the project filesystem.
    pub const WRITE_LOCAL: Self = Self {
        read_only: false,
        destructive: false,
        open_world: false,
        idempotent: false,
    };

    /// Runs a shell command. May have any side-effects.
    pub const BASH: Self = Self {
        read_only: false,
        destructive: true,
        open_world: false,
        idempotent: false,
    };
}

/// A single input/output example, loaded into the model's context to
/// ground the schema.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolExample {
    /// One-line description of the scenario the example demonstrates.
    pub description: String,
    /// The `input` object the model would send.
    pub input: Value,
}

/// Full description of a tool, sent to the model and stored in the
/// `ToolRegistry`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolDescriptor {
    /// snake_case tool name.
    pub name: String,
    /// Prompt-engineered description: when to use, when not to, side effects.
    pub description: String,
    /// JSON Schema describing the tool's input.
    pub input_schema: Value,
    /// Safety profile of the tool.
    #[serde(default)]
    pub annotations: ToolAnnotations,
    /// A handful of concrete examples to ground the schema.
    #[serde(default)]
    pub examples: Vec<ToolExample>,
    /// Cap the output at roughly this many characters.
    #[serde(default = "default_max_chars")]
    pub max_response_chars: usize,
}

fn default_max_chars() -> usize {
    DEFAULT_MAX_RESPONSE_CHARS
}

/// A tool's actual implementation.
#[async_trait]
pub trait ToolContracts: Send + Sync + 'static {
    /// The tool's stable name.
    fn name(&self) -> &'static str;
    /// Full descriptor the model sees.
    fn descriptor(&self) -> ToolDescriptor;
    /// Execute the tool with validated input.
    async fn execute(&self, input: Value) -> Result<ToolOutput, ToolError>;
}

/// The output of a successful tool invocation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ToolOutput(pub Value);

impl ToolOutput {
    /// Wrap a value as a tool output.
    pub fn new(value: impl Serialize) -> Self {
        ToolOutput(serde_json::to_value(value).unwrap_or(Value::Null))
    }

    /// Wrap a value as a tool output, pre-serialised to a string.
    pub fn text(s: impl Into<String>) -> Self {
        ToolOutput(Value::String(s.into()))
    }
}

/// Structured access to a tool name. Newtype over `&'static str` to make
/// API boundaries explicit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ToolName(pub &'static str);

impl ToolName {
    /// All known tool names.
    pub const ALL: &'static [ToolName] = &[
        ToolName(names::READ_FILE),
        ToolName(names::LIST_DIRECTORY),
        ToolName(names::GLOB),
        ToolName(names::GREP),
        ToolName(names::WRITE_FILE),
        ToolName(names::EDIT_FILE),
        ToolName(names::BASH),
        ToolName(names::MEMORY),
    ];

    /// Parse a tool name from a string the model emitted.
    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|n| n.0 == s)
    }
}

impl std::fmt::Display for ToolName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0)
    }
}

/// Error returned by a tool's `execute` impl. Per the Anthropic guide,
/// error responses should be specific and actionable; use the `hint`
/// field to add a remediation the model can act on.
#[derive(Debug, thiserror::Error)]
pub enum ToolError {
    /// The model produced malformed input (or business rules rejected it).
    #[error("invalid input: {message}")]
    InvalidInput {
        /// What was wrong with the input.
        message: String,
        /// Optional, actionable remediation hint for the model.
        hint: Option<String>,
    },
    /// I/O failure (read, write, exec).
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    /// Operation rejected (e.g. path escapes the project root).
    #[error("rejected: {message}")]
    Rejected {
        /// Why the operation was rejected.
        message: String,
        /// Optional actionable hint.
        hint: Option<String>,
    },
    /// A tool the model depended on could not be resolved.
    #[error("tool not found: {0}")]
    ToolNotFound(String),
    /// Anything else.
    #[error("{message}")]
    Other {
        /// The error message.
        message: String,
        /// Optional actionable hint.
        hint: Option<String>,
    },
}

impl ToolError {
    /// Build an `InvalidInput` error with an actionable hint.
    pub fn invalid_input(message: impl Into<String>, hint: impl Into<String>) -> Self {
        ToolError::InvalidInput {
            message: message.into(),
            hint: Some(hint.into()),
        }
    }

    /// Build a `Rejected` error with an actionable hint.
    pub fn rejected(message: impl Into<String>, hint: impl Into<String>) -> Self {
        ToolError::Rejected {
            message: message.into(),
            hint: Some(hint.into()),
        }
    }

    /// `true` if the model should be told to retry, `false` if it should
    /// give up or try a different tool.
    pub fn is_retryable(&self) -> bool {
        matches!(self, ToolError::Io(_))
    }
}

/// `serde_json` shape returned to the model when a tool errors.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolErrorPayload {
    /// `true`.
    pub error: bool,
    /// The kind of error (`"invalid_input"`, `"io"`, `"rejected"`, …).
    pub kind: String,
    /// Human-readable error message.
    pub message: String,
    /// Optional actionable hint.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
    /// Whether the model should retry.
    pub retryable: bool,
}

impl From<&ToolError> for ToolErrorPayload {
    fn from(e: &ToolError) -> Self {
        let (kind, message, hint) = match e {
            ToolError::InvalidInput { message, hint } => {
                ("invalid_input".into(), message.clone(), hint.clone())
            }
            ToolError::Io(io) => ("io".into(), io.to_string(), None),
            ToolError::Rejected { message, hint } => {
                ("rejected".into(), message.clone(), hint.clone())
            }
            ToolError::ToolNotFound(name) => (
                "tool_not_found".into(),
                format!("tool '{name}' is not registered"),
                Some("use one of the tools listed in your system prompt".into()),
            ),
            ToolError::Other { message, hint } => ("other".into(), message.clone(), hint.clone()),
        };
        ToolErrorPayload {
            error: true,
            kind,
            message,
            hint,
            retryable: e.is_retryable(),
        }
    }
}

impl From<ToolError> for ToolOutput {
    fn from(e: ToolError) -> Self {
        ToolOutput(serde_json::to_value(ToolErrorPayload::from(&e)).unwrap_or(Value::Null))
    }
}

/// Convenience: ensure a path stays inside a project root. Returns the
/// absolute, resolved path on success.
pub fn resolve_inside_root(root: &Path, target: &Path) -> std::io::Result<std::path::PathBuf> {
    use std::path::PathBuf;
    let joined: PathBuf = if target.is_absolute() {
        target.to_path_buf()
    } else {
        root.join(target)
    };
    let resolved = joined.canonicalize().unwrap_or(joined);
    let canonical_root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    if !resolved.starts_with(&canonical_root) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            format!(
                "path '{}' is outside the project root '{}'",
                resolved.display(),
                canonical_root.display()
            ),
        ));
    }
    Ok(resolved)
}
