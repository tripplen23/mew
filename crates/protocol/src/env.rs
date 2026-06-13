//! Cross-crate naming conventions: env-var names recognised by the
//! mewcode binaries, and the config-file name they all read from.
//!
//! Per-crate env vars (`MEWCODE_HOST`, `MEWCODE_API_URL`,
//! `MEWCODE_ENGINE_BASE_URL`, ...) live in the crate that owns them.

/// OpenCode Go subscription key. Required by the engine; the server
/// proxies it to the engine.
pub const OPENCODE_GO_API_KEY: &str = "OPENCODE_GO_API_KEY";

/// Postgres connection string. Optional in in-memory mode.
pub const DATABASE_URL: &str = "DATABASE_URL";

/// Optional TOML config file read by both the server and the client.
pub const CONFIG_FILE: &str = "mewcode.toml";
