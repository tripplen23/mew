//! Shared protocol types for mewcode.
//!
//! No I/O. Defines the data shapes that flow between the client (TUI),
//! the server ([axum](https://docs.rs/axum/latest/axum/) + filesystem
//! store), and the engine
//! ([rig-core](https://docs.rs/rig-core/latest/rig_core/)-based agent
//! harness).

#![forbid(unsafe_code)]

pub mod credential;
pub mod display;
pub mod env;
pub mod event;
pub mod message;
pub mod migration;
pub mod mode;
pub mod model;
pub mod routes;
pub mod run;
pub mod skill;
pub mod tool;

pub use credential::{
    ConnectProviderRequest, ConnectProviderResponse, ProviderCredential, ProviderStatus,
};
pub use display::{DiffDisplay, TodoDisplay, TodoItem, TodoList, TodoStatus, ToolDisplay};
pub use event::StreamEvent;
pub use message::{Message, MessagePart, Role, ToolCall, ToolResult};
pub use mode::{Mode, ModeParseError};
pub use model::{ModelId, ModelKind, ProviderId};
pub use run::{
    ApprovalKind, ApprovalRequest, ApprovalState, ArtifactKind, ArtifactRef, Checkpoint,
    FailureKind, FailureReason, RunId, RunPhase, RunPolicy, RunRecord, RunStatus, SourceLock,
};
pub use skill::{
    GLOBAL_SKILLS_DIR, PROJECT_SKILLS_DIR, SKILL_FILE, Skill, SkillError, parse_skill_md,
    read_skill_subfile,
};
pub use tool::{
    DEFAULT_MAX_RESPONSE_CHARS, ResponseFormat, ToolAnnotations, ToolContracts, ToolDescriptor,
    ToolError, ToolErrorPayload, ToolExample, ToolName, ToolOutput, allowed_tools_for_mode,
    truncate_with_marker,
};
