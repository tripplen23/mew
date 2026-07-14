//! `bash` tool — run a shell command in the project directory with a timeout.

use std::process::Stdio;
use std::time::Duration;

use async_trait::async_trait;
use mewcode_protocol::tool::names;
use mewcode_protocol::{
    ToolAnnotations, ToolContracts, ToolDescriptor, ToolError, ToolExample, ToolOutput,
    truncate_with_marker,
};
use serde_json::{Value, json};
use tokio::io::AsyncReadExt;
use tokio::process::Command;

use crate::tools::ProjectContext;

/// Default timeout for a bash command (30 seconds).
const DEFAULT_TIMEOUT_MS: u64 = 30_000;

/// Maximum allowed timeout (5 minutes). The model cannot exceed this.
const MAX_TIMEOUT_MS: u64 = 300_000;

/// Maximum output size before truncation (in characters).
const MAX_OUTPUT_CHARS: usize = 100_000;

/// `bash` tool.
pub struct BashTool {
    ctx: ProjectContext,
}

impl BashTool {
    /// Build the tool against a project context.
    pub fn new(ctx: ProjectContext) -> Self {
        Self { ctx }
    }
}

#[async_trait]
impl ToolContracts for BashTool {
    fn name(&self) -> &'static str {
        names::BASH
    }

    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            name: names::BASH.to_string(),
            description: "Run a shell command in the project directory. The command runs via `bash -c` with a configurable timeout (max 5 minutes).

**When to use:** When you need to run builds, tests, git operations, or other shell commands. The working directory is the project root.

**Safety:** Output is truncated at ~100k characters with a clear marker. Commands that exceed the timeout are killed. This tool is marked `destructive` — it is blocked in PLAN mode."
                .to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "command": {
                        "type": "string",
                        "description": "The shell command to run. Executed via `bash -c`."
                    },
                    "timeout_ms": {
                        "type": "integer",
                        "default": 30000,
                        "description": "Maximum execution time in milliseconds (capped at 300000). The command is killed if it exceeds this."
                    }
                },
                "required": ["command"],
                "additionalProperties": false,
            }),
            annotations: ToolAnnotations::BASH,
            examples: vec![
                ToolExample {
                    description: "Run a build.".to_string(),
                    input: json!({ "command": "cargo build --release" }),
                },
                ToolExample {
                    description: "Check git status with a short timeout.".to_string(),
                    input: json!({
                        "command": "git status --short",
                        "timeout_ms": 5000
                    }),
                },
            ],
            max_response_chars: MAX_OUTPUT_CHARS,
        }
    }

    async fn execute(&self, input: Value) -> Result<ToolOutput, ToolError> {
        let command = input
            .get("command")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                ToolError::invalid_input("missing `command`", "pass a string `command` field")
            })?;
        let timeout_ms = input
            .get("timeout_ms")
            .and_then(|v| v.as_u64())
            .unwrap_or(DEFAULT_TIMEOUT_MS)
            .min(MAX_TIMEOUT_MS);

        let mut cmd = Command::new("bash");
        cmd.arg("-c")
            .arg(command)
            .current_dir(&self.ctx.root)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);

        let mut child = cmd.spawn().map_err(|e| ToolError::Other {
            message: format!("failed to spawn command: {e}"),
            hint: Some("ensure `bash` is available in PATH".into()),
        })?;

        // Read stdout and stderr concurrently with wait() to avoid the
        // classic pipe-buffer deadlock: if the child writes more than the
        // OS pipe buffer (~64KB) and the parent is blocked on wait(),
        // the child blocks on write() and neither side makes progress.
        let mut stdout = child.stdout.take().expect("stdout was piped");
        let mut stderr = child.stderr.take().expect("stderr was piped");

        let mut stdout_buf = String::new();
        let mut stderr_buf = String::new();

        let timeout = Duration::from_millis(timeout_ms);

        // Spawn concurrent readers so the pipe is drained while the child runs.
        let stdout_fut = async { stdout.read_to_string(&mut stdout_buf).await };
        let stderr_fut = async { stderr.read_to_string(&mut stderr_buf).await };

        // Race the child exit against the timeout, but keep draining the
        // pipes in all cases.
        let (wait_result, _, _) = tokio::join!(
            tokio::time::timeout(timeout, child.wait()),
            stdout_fut,
            stderr_fut,
        );

        let exit_status = match wait_result {
            Ok(status) => status.map_err(ToolError::Io)?,
            Err(_) => {
                let _ = child.kill().await;
                let _ = child.wait().await;
                return Err(ToolError::Other {
                    message: format!("command timed out after {timeout_ms}ms"),
                    hint: Some("increase `timeout_ms` or optimize the command".into()),
                });
            }
        };

        let stdout_truncated = truncate_with_marker(&stdout_buf, MAX_OUTPUT_CHARS);
        let stderr_truncated = truncate_with_marker(&stderr_buf, MAX_OUTPUT_CHARS);

        Ok(ToolOutput(json!({
            "command": command,
            "exit_code": exit_status.code().unwrap_or(-1),
            "stdout": stdout_truncated,
            "stderr": stderr_truncated,
            "stdout_truncated": stdout_buf.chars().count() > MAX_OUTPUT_CHARS,
            "stderr_truncated": stderr_buf.chars().count() > MAX_OUTPUT_CHARS,
        })))
    }
}
