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
use std::sync::Arc;

use mewcode_protocol::{Mode, ToolContracts, ToolDescriptor, ToolError, ToolOutput};
use serde_json::Value;

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
pub use skills::UseSkillTool;

/// Engine-local alias for the shared skill registry. We keep the
/// engine's [`SkillRegistry`] in [`crate::skills`] and pass it in to
/// tool implementations that need it (today: `use_skill`).
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
}

/// Project context. Every tool needs to know what directory to operate on.
#[derive(Debug, Clone)]
pub struct ProjectContext {
    /// Absolute path to the project root the tools operate on.
    pub root: PathBuf,
}

impl ProjectContext {
    /// Build a context rooted at the given directory.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }
}

/// Build the default tool registry for the given mode.
///
/// In `Mode::Build` all tools are registered. In `Mode::Plan` only
/// read-only, non-destructive tools are registered — write tools
/// (`write_file`, `edit_file`, `bash`, `mewcode_memory`) are excluded
/// so the model physically cannot mutate state. A model that tries to
/// call a filtered tool gets the same `ToolNotFound` error as if it
/// were unregistered.
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
    reg.register(Arc::new(UseSkillTool::new(skills)));

    // Write-capable tools — only in Build mode. `mewcode_memory` is
    // `WRITE_LOCAL` (it can persist facts to disk) so it must be gated
    // the same way as `write_file` / `edit_file` / `bash`.
    if mode.allows_writes() {
        if let Some(store) = memory {
            reg.register(Arc::new(MewcodeMemoryTool::new(store)));
        }
        reg.register(Arc::new(WriteFileTool::new(ctx.clone())));
        reg.register(Arc::new(EditFileTool::new(ctx.clone())));
        reg.register(Arc::new(BashTool::new(ctx)));
    }

    reg
}
