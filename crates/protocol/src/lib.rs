//! Shared protocol types for mewcode.
//!
//! No I/O. Defines the data shapes that flow between the client (TUI),
//! the server (axum + filesystem store), and the engine (rig-based agent harness).

#![forbid(unsafe_code)]

pub mod env;
pub mod event;
pub mod message;
pub mod mode;
pub mod model;
pub mod routes;
pub mod skill;
pub mod tool;

pub use event::StreamEvent;
pub use message::{Message, MessagePart, Role, ToolCall, ToolResult};
pub use mode::{Mode, ModeParseError};
pub use model::{ModelId, ModelKind};
pub use skill::{
    parse_skill_md, Skill, SkillError, GLOBAL_SKILLS_DIR, PROJECT_SKILLS_DIR, SKILL_FILE,
};
pub use tool::{
    tools_for_mode, truncate_with_marker, ResponseFormat, ToolAnnotations, ToolContracts,
    ToolDescriptor, ToolError, ToolErrorPayload, ToolExample, ToolName, ToolOutput,
};
