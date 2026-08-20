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

use std::sync::Arc;

use mewcode_protocol::Mode;

use crate::context::{MemoryStore, TodoStore};
use crate::skills::SkillRegistry;

pub mod adapter;
mod approval;
mod context;
mod fs;
mod memory;
mod policy;
mod registry;
mod search;
mod shell;
mod skills;
mod todo;

pub use approval::ApprovalBroker;
pub use context::{DisplayRecord, DisplaySink, ProjectContext};
pub use fs::{EditFileTool, GlobTool, ListDirectoryTool, ReadFileTool, WriteFileTool};
pub use memory::MewcodeMemoryTool;
use policy::{PlanDeniedTool, PlanReadOnlyBashTool};
pub use registry::ToolRegistry;
pub use search::GrepTool;
pub use shell::BashTool;
pub use skills::{SkillViewTool, SkillsListTool};
pub use todo::{TodoReadTool, TodoWriteTool};

/// Engine-local alias for the shared skill registry. We keep the
/// engine's [`SkillRegistry`] in [`crate::skills`] and pass it in to
/// tool implementations that need it (today: `skills_list`, `skill_view`).
pub type Skills = Arc<SkillRegistry>;

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
    default_registry_with_todos(ctx, skills, memory, None, mode)
}

/// Build the default tool registry, with an optional per-session [`TodoStore`].
///
/// When a store is present the scratch `todo_write`/`todo_read` tools are
/// registered in BOTH modes (they never touch project state). `default_registry`
/// without a store omits them entirely — headless paths that have no session.
pub fn default_registry_with_todos(
    ctx: ProjectContext,
    skills: Skills,
    memory: Option<MemoryStore>,
    todos: Option<TodoStore>,
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

    // Session-scratch todo tools — both modes, never approval-gated.
    if let Some(store) = todos {
        reg.register(Arc::new(TodoWriteTool::new(store.clone(), ctx.clone())));
        reg.register(Arc::new(TodoReadTool::new(store)));
    }

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
