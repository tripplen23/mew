//! AI agent harness for mewcode. Talks to the OpenCode Go API (both
//! Anthropic-compatible and OpenAI-compatible endpoints) via
//! [rig-core](https://docs.rs/rig-core/latest/rig_core/), registers local
//! tools, and runs the tool-calling loop that turns a user message into
//! a stream of [`mewcode_protocol::StreamEvent`]s.

#![forbid(unsafe_code)]

pub mod agent;
pub mod compaction;
pub mod config;
pub mod context;
pub mod error;
pub mod harness;
pub mod observability;
pub mod skills;
pub mod tools;
pub mod wc;

pub use agent::{Agent, Provider, build_system_prompt};
pub use compaction::{CompactionResult, compact_history};
pub use config::EngineConfig;
pub use error::EngineError;
pub use harness::Harness;
pub use skills::{LoadedSkill, SkillRegistry, SkillSource};
