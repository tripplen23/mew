use std::sync::Arc;

use async_trait::async_trait;
use mewcode_protocol::{ToolContracts, ToolDescriptor, ToolError, ToolOutput};
use serde_json::Value;

const BLOCKED_SHELL_TOKENS: &[&str] = &[";", "&&", "||", "&", ">", "<", "|", "`", "$(", "\n", "\r"];
const PLAN_READ_ONLY_COMMANDS: &[&str] = &[
    "git status",
    "git log",
    "git diff",
    "git show",
    "git branch",
    "git stash list",
    "ls",
    "pwd",
    "cat",
    "grep",
    "rg",
    "wc",
    "head",
    "tail",
];

pub struct PlanDeniedTool {
    inner: Arc<dyn ToolContracts>,
}

pub struct PlanReadOnlyBashTool {
    inner: Arc<dyn ToolContracts>,
}

impl PlanDeniedTool {
    pub fn new(inner: Arc<dyn ToolContracts>) -> Self {
        Self { inner }
    }
}

impl PlanReadOnlyBashTool {
    pub fn new(inner: Arc<dyn ToolContracts>) -> Self {
        Self { inner }
    }
}

#[async_trait]
impl ToolContracts for PlanDeniedTool {
    fn name(&self) -> &'static str {
        self.inner.name()
    }

    fn descriptor(&self) -> ToolDescriptor {
        let mut descriptor = self.inner.descriptor();
        descriptor.description = format!(
            "{}\n\n**Plan mode:** This tool is visible so denied requests are explicit, but executing it is blocked. Tell the user to switch to Build mode if they want this change applied.",
            descriptor.description
        );
        descriptor
    }

    async fn execute(&self, _input: Value) -> Result<ToolOutput, ToolError> {
        Err(ToolError::Rejected {
            message: format!("{} is blocked in Plan mode", self.name()),
            hint: Some(
                "Explain that Plan mode is read-only. Ask the user to switch to Build mode to apply file edits or shell commands."
                    .into(),
            ),
        })
    }
}

#[async_trait]
impl ToolContracts for PlanReadOnlyBashTool {
    fn name(&self) -> &'static str {
        self.inner.name()
    }

    fn descriptor(&self) -> ToolDescriptor {
        let mut descriptor = self.inner.descriptor();
        let commands = PLAN_READ_ONLY_COMMANDS.join(", ");
        descriptor.description = format!(
            "{}\n\n**Plan mode:** Only read-only inspection commands are allowed here: {commands}. Commands with shell composition or redirection are blocked. Switch to Build mode for mutating commands.",
            descriptor.description
        );
        descriptor
    }

    async fn execute(&self, input: Value) -> Result<ToolOutput, ToolError> {
        let command = input.get("command").and_then(|v| v.as_str()).unwrap_or("");
        if !is_plan_read_only_command(command) {
            return Err(ToolError::Rejected {
                message: "bash command is blocked in Plan mode".into(),
                hint: Some(
                    "Use read-only inspection commands only, or ask the user to switch to Build mode for commands that modify files, git state, or external systems."
                        .into(),
                ),
            });
        }
        self.inner.execute(input).await
    }
}

fn is_plan_read_only_command(command: &str) -> bool {
    let command = command.trim();
    if command.is_empty()
        || BLOCKED_SHELL_TOKENS
            .iter()
            .any(|token| command.contains(token))
    {
        return false;
    }
    PLAN_READ_ONLY_COMMANDS
        .iter()
        .any(|allowed| command == *allowed || command.starts_with(&format!("{allowed} ")))
}
