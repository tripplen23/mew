//! Cross-crate naming conventions: env-var names recognised by the
//! mew binaries, and the config-file name they all read from.
//!
//! Per-crate env vars (`MEWCODE_HOST`, `MEWCODE_API_URL`,
//! `MEWCODE_ENGINE_BASE_URL`, ...) live in the crate that owns them.
//! The `MEWCODE_` prefix predates the `mew` product rename and is kept
//! for backward compatibility; new env vars should use `MEW_`.

/// OpenCode Go subscription key. Required by the engine; the server
/// proxies it to the engine.
pub const OPENCODE_GO_API_KEY: &str = "OPENCODE_GO_API_KEY";

/// Native OpenAI API key. Optional — when set, native OpenAI models
/// become available.
pub const OPENAI_API_KEY: &str = "OPENAI_API_KEY";

/// Native DeepSeek API key. Optional — when set, native DeepSeek models
/// become available.
pub const DEEPSEEK_API_KEY: &str = "DEEPSEEK_API_KEY";

/// Optional override for the data directory where sessions are stored.
/// Falls back to `$XDG_DATA_HOME/mew`, then `~/.local/share/mew`.
pub const MEWCODE_DATA_DIR: &str = "MEWCODE_DATA_DIR";

/// Optional TOML config file read by both the server and the client.
pub const CONFIG_FILE: &str = "mew.toml";
