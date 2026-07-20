//! Engine-local tool registry. This module holds the shared
//! scaffolding: the [`ToolRegistry`], the [`ProjectContext`] every tool
//! receives, the [`Skills`] type alias, the [`adapter`] that bridges
//! mewcode tools to Rig's `ToolDyn`, and the [`default_registry`] factory.
//!
//! Adding a new tool:
//! 1. Create it under the appropriate domain submodule
//!    (e.g. `crates/engine/src/tools/fs/<name>.rs`).
//! 2. Add `mod <name>;` and `pub use <name>::<Tool>;` in that
//!    submodule's `mod.rs`.
//! 3. Register it in [`default_registry`] (or wherever the harness
//!    builds its registry).

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use mewcode_protocol::{
    Mode, StreamEvent, ToolContracts, ToolDescriptor, ToolDisplay, ToolError, ToolOutput,
};
use serde_json::Value;
use tokio::sync::mpsc;
use uuid::Uuid;

use crate::approval::ApprovalBroker;
use crate::memory::MemoryStore;
use crate::skills::SkillRegistry;

pub mod adapter;
mod fs;
mod memory;
mod search;
mod shell;
mod skills;

pub use fs::{EditFileTool, GlobTool, ListDirectoryTool, ReadFileTool, WriteFileTool};
pub use memory::MewcodeMemoryTool;
pub use search::GrepTool;
pub use shell::BashTool;
pub use skills::{SkillViewTool, SkillsListTool};

/// Engine-local alias for the shared skill registry. We keep the
/// engine's [`SkillRegistry`] in [`crate::skills`] and pass it in to
/// tool implementations that need it (today: `skills_list`, `skill_view`).
pub type Skills = Arc<SkillRegistry>;

/// Registry of tools available to the harness.
#[derive(Default, Clone)]
pub struct ToolRegistry {
    inner: HashMap<&'static str, Arc<dyn ToolContracts>>,
}

impl std::fmt::Debug for ToolRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ToolRegistry")
            .field("tools", &self.inner.keys().collect::<Vec<_>>())
            .finish()
    }
}

impl ToolRegistry {
    /// Build an empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a tool. A tool with the same name replaces the previous.
    pub fn register(&mut self, tool: Arc<dyn ToolContracts>) {
        self.inner.insert(tool.name(), tool);
    }

    /// Look up a tool by its static name.
    pub fn get_by_name(&self, name: &str) -> Option<Arc<dyn ToolContracts>> {
        self.inner.get(name).cloned()
    }

    /// Names of all registered tools, in insertion order.
    pub fn names(&self) -> Vec<&'static str> {
        self.inner.keys().copied().collect()
    }

    /// `true` if no tools are registered.
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// Iterate over every registered tool's descriptor.
    pub fn descriptors(&self) -> Vec<ToolDescriptor> {
        self.inner.values().map(|t| t.descriptor()).collect()
    }

    /// Dispatch a tool call. Errors are returned as `ToolErrorPayload`-shaped JSON.
    pub async fn dispatch(&self, name: &str, input: Value) -> ToolOutput {
        match self.inner.get(name) {
            None => ToolError::ToolNotFound(name.to_string()).into(),
            Some(tool) => match tool.execute(input).await {
                Ok(out) => out,
                Err(e) => e.into(),
            },
        }
    }

    /// Return a copy that asks before executing Build-mode write/edit/bash tools.
    pub fn with_approval(
        &self,
        session_id: Uuid,
        broker: ApprovalBroker,
        events: mpsc::Sender<StreamEvent>,
    ) -> Self {
        let mut reg = ToolRegistry::new();
        for (name, tool) in &self.inner {
            if matches!(
                *name,
                mewcode_protocol::tool::names::WRITE_FILE
                    | mewcode_protocol::tool::names::EDIT_FILE
                    | mewcode_protocol::tool::names::BASH
            ) {
                reg.register(Arc::new(ApprovalTool {
                    inner: tool.clone(),
                    session_id,
                    broker: broker.clone(),
                    events: events.clone(),
                }));
            } else {
                reg.register(tool.clone());
            }
        }
        reg
    }
}

struct ApprovalTool {
    inner: Arc<dyn ToolContracts>,
    session_id: Uuid,
    broker: ApprovalBroker,
    events: mpsc::Sender<StreamEvent>,
}

#[async_trait]
impl ToolContracts for ApprovalTool {
    fn name(&self) -> &'static str {
        self.inner.name()
    }

    fn descriptor(&self) -> ToolDescriptor {
        let mut descriptor = self.inner.descriptor();
        descriptor.description = format!(
            "{}\n\n**Approval required:** Build mode asks the user before this tool executes. The user can allow once, allow matching calls in this chat session, or deny.",
            descriptor.description
        );
        descriptor
    }

    async fn execute(&self, input: Value) -> Result<ToolOutput, ToolError> {
        self.broker
            .approve_tool(self.session_id, self.name(), &input, &self.events)
            .await?;
        self.inner.execute(input).await
    }
}

/// Display record tagged with the tool's input arguments.
///
/// Tools never see Rig's call id, so they stamp display data with `args`
/// and the stream layer matches it back by value.
#[derive(Debug, Clone)]
pub struct DisplayRecord {
    /// The tool's input arguments — identical to what the model sent.
    pub args: Value,
    /// The render payload.
    pub display: ToolDisplay,
}

/// Out-of-band sink for render-only display (diffs, previews). Kept off
/// the model path so display payloads never enter the context window.
pub type DisplaySink = Arc<Mutex<Vec<DisplayRecord>>>;

struct PlanDeniedTool {
    inner: Arc<dyn ToolContracts>,
}

struct PlanReadOnlyBashTool {
    inner: Arc<dyn ToolContracts>,
}

impl PlanDeniedTool {
    fn new(inner: Arc<dyn ToolContracts>) -> Self {
        Self { inner }
    }
}

impl PlanReadOnlyBashTool {
    fn new(inner: Arc<dyn ToolContracts>) -> Self {
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
        descriptor.description = format!(
            "{}\n\n**Plan mode:** Only read-only inspection commands are allowed here: git status/log/diff/show/branch, ls, pwd, cat, grep, rg, wc, head, tail. Commands with shell composition or redirection are blocked. Switch to Build mode for mutating commands.",
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
        || [";", "&&", "||", "&", ">", "<", "|", "`", "$(", "\n", "\r"]
            .iter()
            .any(|token| command.contains(token))
    {
        return false;
    }
    [
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
    ]
    .iter()
    .any(|allowed| command == *allowed || command.starts_with(&format!("{allowed} ")))
}

/// Project context. Every tool needs to know what directory to operate on.
#[derive(Debug, Clone)]
pub struct ProjectContext {
    /// Absolute path to the project root the tools operate on.
    pub root: PathBuf,
    /// Display sink. `Some` when streaming to a UI, `None` in headless paths.
    pub display: Option<DisplaySink>,
}

impl ProjectContext {
    /// Build a context rooted at the given directory, with no display sink.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            display: None,
        }
    }

    /// Attach a display sink so mutating tools can emit render-only data.
    pub fn with_display(mut self, sink: DisplaySink) -> Self {
        self.display = Some(sink);
        self
    }

    /// Record a display payload for `args` if a sink is attached. Cheap no-op
    /// otherwise, so tools can call it unconditionally.
    pub fn push_display(&self, args: Value, display: ToolDisplay) {
        if let Some(sink) = &self.display {
            if let Ok(mut records) = sink.lock() {
                records.push(DisplayRecord { args, display });
            }
        }
    }
}

/// Build the default tool registry for the given mode.
///
/// In `Mode::Build` all tools execute normally. In `Mode::Plan`, mutating
/// tools are still visible to the model but return explicit permission errors;
/// bash is limited to a small read-only inspection allowlist.
pub fn default_registry(
    ctx: ProjectContext,
    skills: Skills,
    memory: Option<MemoryStore>,
    mode: Mode,
) -> ToolRegistry {
    let mut reg = ToolRegistry::new();

    // Read-only tools — always available.
    reg.register(Arc::new(ReadFileTool::new(ctx.clone())));
    reg.register(Arc::new(ListDirectoryTool::new(ctx.clone())));
    reg.register(Arc::new(GlobTool::new(ctx.clone())));
    reg.register(Arc::new(GrepTool::new(ctx.clone())));
    reg.register(Arc::new(SkillsListTool::new(skills.clone())));
    reg.register(Arc::new(SkillViewTool::new(skills)));

    // `mewcode_memory` persists to disk (WRITE_LOCAL) — gate it with the writers.
    if mode.allows_writes() {
        if let Some(store) = memory {
            reg.register(Arc::new(MewcodeMemoryTool::new(store)));
        }
        reg.register(Arc::new(WriteFileTool::new(ctx.clone())));
        reg.register(Arc::new(EditFileTool::new(ctx.clone())));
        reg.register(Arc::new(BashTool::new(ctx)));
    } else {
        if let Some(store) = memory {
            reg.register(Arc::new(PlanDeniedTool::new(Arc::new(
                MewcodeMemoryTool::new(store),
            ))));
        }
        reg.register(Arc::new(PlanDeniedTool::new(Arc::new(WriteFileTool::new(
            ctx.clone(),
        )))));
        reg.register(Arc::new(PlanDeniedTool::new(Arc::new(EditFileTool::new(
            ctx.clone(),
        )))));
        reg.register(Arc::new(PlanReadOnlyBashTool::new(Arc::new(
            BashTool::new(ctx),
        ))));
    }

    reg
}
