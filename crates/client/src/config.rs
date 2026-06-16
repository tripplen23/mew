//! Client configuration.

use figment::Figment;
use figment::providers::{Env, Format, Toml};
use mewcode_protocol::env::CONFIG_FILE;
use serde::Deserialize;

/// Base URL the client talks to by default.
pub const DEFAULT_API_URL: &str = "http://127.0.0.1:3737";

/// Default TUI theme.
pub const DEFAULT_THEME: &str = "catppuccin-mocha";

/// Default `tracing` filter when `RUST_LOG` is unset.
pub const DEFAULT_LOG: &str = "info";

/// Env-var prefix figment reads for the client config.
pub const ENV_PREFIX: &str = "MEWCODE_CLIENT_";

/// Canonical env-var name for the server URL, recognised even though
/// it doesn't match [`ENV_PREFIX`].
pub const ENV_API_URL: &str = "MEWCODE_API_URL";

/// Client configuration, loaded from `mewcode.toml` and the environment.
#[derive(Debug, Clone, Deserialize)]
pub struct ClientConfig {
    /// Base URL of the mewcode server.
    #[serde(default = "default_api_url")]
    pub api_url: String,
    /// Default model id.
    #[serde(default)]
    pub default_model: Option<String>,
    /// Default theme name.
    #[serde(default = "default_theme")]
    pub theme: String,
    /// Log filter.
    #[serde(default = "default_log")]
    pub log: String,
}

fn default_api_url() -> String {
    DEFAULT_API_URL.to_string()
}
fn default_theme() -> String {
    DEFAULT_THEME.to_string()
}
fn default_log() -> String {
    DEFAULT_LOG.to_string()
}

/// Load from env + optional toml. `MEWCODE_API_URL` is the canonical
/// server URL env var.
impl ClientConfig {
    pub fn load() -> Result<Self, Box<figment::Error>> {
        let figment = Figment::new()
            .merge(Toml::file(CONFIG_FILE).nested())
            .merge(Env::prefixed(ENV_PREFIX).split("__"));
        if let Ok(url) = std::env::var(ENV_API_URL) {
            return figment.merge(("api_url", url)).extract().map_err(Box::new);
        }
        figment.extract().map_err(Box::new)
    }
}
