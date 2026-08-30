//! Server configuration.

use std::collections::BTreeMap;

use figment::Figment;
use figment::providers::{Env, Format, Toml};
use mewcode_engine::mcp::McpServerConfig;
use mewcode_protocol::env::{
    ANTHROPIC_API_KEY, CONFIG_FILE, OPENAI_API_KEY, OPENCODE_GO_API_KEY, OPENCODE_ZEN_API_KEY,
    OPENROUTER_API_KEY,
};
use serde::Deserialize;

pub const DEFAULT_HOST: &str = "127.0.0.1";
pub const DEFAULT_PORT: u16 = 3737;
pub const DEFAULT_LOG: &str = "info,mewcode_engine=debug";
pub const ENV_PREFIX: &str = "MEWCODE_";
const PREFIXED_OPENCODE_GO_API_KEY: &str = "MEWCODE_OPENCODE_GO_API_KEY";
const PREFIXED_OPENCODE_ZEN_API_KEY: &str = "MEWCODE_OPENCODE_ZEN_API_KEY";
const PREFIXED_OPENAI_API_KEY: &str = "MEWCODE_OPENAI_API_KEY";
const PREFIXED_ANTHROPIC_API_KEY: &str = "MEWCODE_ANTHROPIC_API_KEY";
const PREFIXED_OPENROUTER_API_KEY: &str = "MEWCODE_OPENROUTER_API_KEY";

/// Expand a `~` and `${VAR}` placeholders in `raw`. Returns the path
/// unchanged if the placeholder is unset. Used for `external_dirs`
/// (Hermes-compatible behaviour).
fn expand_path(raw: &str) -> String {
    let mut s = raw.to_string();
    if let Some(stripped) = s.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            s = format!("{}/{}", home.display(), stripped);
        }
    } else if s == "~" {
        if let Some(home) = dirs::home_dir() {
            s = home.display().to_string();
        }
    }
    // ${VAR} substitution. Char-based walk so non-ASCII bytes
    // (e.g. `café` in a path) survive intact.
    let mut result = String::with_capacity(s.len());
    let mut rest = s.as_str();
    while let Some(start) = rest.find("${") {
        result.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        if let Some(end) = after.find('}') {
            let var = &after[..end];
            match std::env::var(var) {
                Ok(v) => result.push_str(&v),
                Err(_) => result.push_str(&rest[start..start + 2 + end + 1]),
            }
            rest = &after[end + 1..];
        } else {
            result.push_str(&rest[start..]);
            return result;
        }
    }
    result.push_str(rest);
    result
}

/// Server configuration, loaded from `mew.toml` and the environment.
#[derive(Debug, Clone, Deserialize)]
pub struct ServerConfig {
    /// Host to bind to.
    #[serde(default = "default_host")]
    pub host: String,
    /// Port to bind to.
    #[serde(default = "default_port")]
    pub port: u16,
    /// OpenCode Go API key. Optional.
    #[serde(default)]
    pub opencode_go_api_key: Option<String>,
    /// OpenCode Zen API key. Optional.
    #[serde(default)]
    pub opencode_zen_api_key: Option<String>,
    /// Native OpenAI API key. Optional.
    #[serde(default)]
    pub openai_api_key: Option<String>,
    /// Native Anthropic API key. Optional.
    #[serde(default)]
    pub anthropic_api_key: Option<String>,
    /// OpenRouter API key. Optional.
    #[serde(default)]
    pub openrouter_api_key: Option<String>,
    /// Default model.
    #[serde(default)]
    pub default_model: Option<String>,
    /// Log level.
    #[serde(default = "default_log")]
    pub log: String,
    /// Skill configuration.
    #[serde(default)]
    pub skills: SkillServerConfig,
    /// GitHub App configuration (the @mewcli review bot).
    #[serde(default)]
    pub github: GithubServerConfig,
    /// External MCP servers Mew connects to as a client, keyed by name.
    #[serde(default)]
    pub mcp: BTreeMap<String, McpServerConfig>,
}

/// GitHub App subsection of [`ServerConfig`].
///
/// Loaded from `MEWCODE_GITHUB__*` env vars; leaving them unset disables
/// the webhook endpoint.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct GithubServerConfig {
    /// Webhook secret for `X-Hub-Signature-256` verification.
    #[serde(default)]
    pub webhook_secret: Option<String>,
    /// GitHub App ID (JWT `iss` for installation tokens).
    #[serde(default)]
    pub app_id: Option<u64>,
    /// Path to the GitHub App private key (`.pem`).
    #[serde(default)]
    pub private_key_path: Option<String>,
}

impl GithubServerConfig {
    /// All three app credentials present; anything less disables the webhook.
    pub fn is_complete(&self) -> bool {
        self.webhook_secret.is_some() && self.app_id.is_some() && self.private_key_path.is_some()
    }
}

/// Skills subsection of [`ServerConfig`].
#[derive(Debug, Clone, Default, Deserialize)]
pub struct SkillServerConfig {
    /// Additional skill directories to scan, in addition to the
    /// defaults. `~` and `${VAR}` are expanded at load time.
    /// `https://hermes-agent.nousresearch.com/docs/user-guide/features/skills`).
    #[serde(default)]
    pub external_dirs: Vec<String>,
}

fn default_host() -> String {
    DEFAULT_HOST.to_string()
}
fn default_port() -> u16 {
    DEFAULT_PORT
}
fn default_log() -> String {
    DEFAULT_LOG.to_string()
}

impl ServerConfig {
    /// Load from env vars and optional `mew.toml`.
    pub fn load() -> Result<Self, Box<figment::Error>> {
        let mut figment = Figment::new().merge(Toml::file(CONFIG_FILE).nested());

        for (field, canonical, prefixed) in [
            (
                "opencode_go_api_key",
                OPENCODE_GO_API_KEY,
                PREFIXED_OPENCODE_GO_API_KEY,
            ),
            (
                "opencode_zen_api_key",
                OPENCODE_ZEN_API_KEY,
                PREFIXED_OPENCODE_ZEN_API_KEY,
            ),
            ("openai_api_key", OPENAI_API_KEY, PREFIXED_OPENAI_API_KEY),
            (
                "anthropic_api_key",
                ANTHROPIC_API_KEY,
                PREFIXED_ANTHROPIC_API_KEY,
            ),
            (
                "openrouter_api_key",
                OPENROUTER_API_KEY,
                PREFIXED_OPENROUTER_API_KEY,
            ),
        ] {
            if std::env::var(prefixed).is_err() {
                if let Ok(key) = std::env::var(canonical) {
                    figment = figment.merge((field, key));
                }
            }
        }

        figment
            .merge(Env::prefixed(ENV_PREFIX).split("__"))
            .extract()
            .map_err(Box::new)
    }

    /// The `[mcp]` table with `~` and `${VAR}` expanded in command arguments
    /// and environment values, so an API key can live in `.env` instead of
    /// being committed to `mew.toml`.
    pub fn resolved_mcp(&self) -> BTreeMap<String, McpServerConfig> {
        self.mcp
            .iter()
            .map(|(name, server)| {
                let mut server = server.clone();
                server.command = server.command.iter().map(|s| expand_path(s)).collect();
                server.environment = server
                    .environment
                    .iter()
                    .map(|(k, v)| (k.clone(), expand_path(v)))
                    .collect();
                (name.clone(), server)
            })
            .collect()
    }
}

impl SkillServerConfig {
    /// Resolve `external_dirs` to a list of absolute paths with `~` and
    /// `${VAR}` placeholders expanded. Non-existent paths are still
    /// returned — the engine's `SkillRegistry::load` will silently
    /// skip them (Hermes behaviour).
    pub fn resolved_dirs(&self) -> Vec<std::path::PathBuf> {
        self.external_dirs
            .iter()
            .map(|s| std::path::PathBuf::from(expand_path(s)))
            .collect()
    }
}
