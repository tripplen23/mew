//! Server configuration.

use figment::providers::{Env, Format, Toml};
use figment::Figment;
use mewcode_protocol::env::{CONFIG_FILE, OPENCODE_GO_API_KEY};
use serde::Deserialize;

/// Default host the server binds to.
pub const DEFAULT_HOST: &str = "127.0.0.1";

/// Default port the server binds to.
pub const DEFAULT_PORT: u16 = 3737;

/// Default `tracing` filter when `RUST_LOG` is unset.
pub const DEFAULT_LOG: &str = "info,mewcode_engine=debug";

/// Env-var prefix figment reads for the server config.
pub const ENV_PREFIX: &str = "MEWCODE_";

/// Server configuration, loaded from `mewcode.toml` and the environment.
#[derive(Debug, Clone, Deserialize)]
pub struct ServerConfig {
    /// Host to bind to.
    #[serde(default = "default_host")]
    pub host: String,
    /// Port to bind to.
    #[serde(default = "default_port")]
    pub port: u16,
    /// OpenCode Go API key. Required.
    pub opencode_go_api_key: String,
    /// Default model.
    #[serde(default)]
    pub default_model: Option<String>,
    /// Log level.
    #[serde(default = "default_log")]
    pub log: String,
}

fn default_host() -> String { DEFAULT_HOST.to_string() }
fn default_port() -> u16 { DEFAULT_PORT }
fn default_log() -> String { DEFAULT_LOG.to_string() }

impl ServerConfig {
    /// Load from env vars and optional `mewcode.toml`.
    pub fn load() -> Result<Self, Box<figment::Error>> {
        let mut figment = Figment::new()
            .merge(Toml::file(CONFIG_FILE).nested())
            .merge(Env::prefixed(ENV_PREFIX).split("__"));

        // `OPENCODE_GO_API_KEY` is the canonical env var; pull it in if the
        // prefixed form isn't set.
        if let Ok(key) = std::env::var(OPENCODE_GO_API_KEY) {
            if figment.find_metadata("opencode_go_api_key").is_none() {
                figment = figment.merge(("opencode_go_api_key", key));
            }
        }

        figment.extract().map_err(Box::new)
    }
}
