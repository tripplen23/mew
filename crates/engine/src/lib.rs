//! AI agent harness for mewcode. Talks to the OpenCode Go API (both
//! Anthropic-compatible and OpenAI-compatible endpoints), registers local
//! tools, and runs the tool-calling loop that turns a user message into
//! a stream of [`mewcode_protocol::StreamEvent`]s.

#![forbid(unsafe_code)]

pub mod provider;
pub mod agent;
pub mod harness;
pub mod skills;
pub mod tools;
pub mod streaming;
pub mod trace;
pub mod error;
pub mod config;

pub use config::EngineConfig;
pub use error::EngineError;
pub use harness::Harness;
pub use provider::Provider;
pub use skills::{LoadedSkill, SkillRegistry, SkillSource};
