//! Permission store — persistent "always allow" tool approvals.
//!
//! The interactive approval dialog's "Always allow" choice writes the tool
//! name here, and the broker is preloaded from here at startup, so the rule
//! survives restarts. Stored as a YAML list in `~/.config/mew/permissions.yaml`.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use mewcode_protocol::tool::ToolName;

/// File name inside the Mew config directory.
const PERMISSIONS_FILE: &str = "permissions.yaml";

/// Where Mew stores its configuration.
fn config_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("mew")
}

/// In-memory view of the persistent always-allow rules.
#[derive(Debug, Clone, Default)]
pub struct PermissionStore {
    /// Tool names the user always allows.
    pub allowed_tools: HashSet<String>,
}

impl PermissionStore {
    /// Load from the default config path. Missing or unparsable files load
    /// as empty — a broken permissions file must not brick tool runs.
    pub fn load() -> Self {
        Self::load_from(&config_dir().join(PERMISSIONS_FILE)).unwrap_or_else(|_| Self::default())
    }

    /// Load from an explicit path (also the test seam). Unknown tool names
    /// are dropped so a stale entry never widens approval scope silently.
    pub fn load_from(path: &Path) -> Result<Self, std::io::Error> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let contents = fs::read_to_string(path)?;
        let names: Vec<String> = serde_yaml::from_str(&contents).unwrap_or_default();
        let allowed_tools = names
            .into_iter()
            .filter(|name| ToolName::parse(name).is_some())
            .collect();
        Ok(Self { allowed_tools })
    }

    /// Persist one more always-allowed tool to the default config path.
    /// Missing parent dirs are created on first use; failures are returned to
    /// the caller (`allow_forever` treats them as best-effort).
    pub fn allow_forever(&mut self, tool: &str) -> Result<(), std::io::Error> {
        self.allow_forever_to(&config_dir().join(PERMISSIONS_FILE), tool)
    }

    /// Persist to an explicit path (also the test seam, so tests never touch
    /// the user's real config directory).
    pub fn allow_forever_to(&mut self, path: &Path, tool: &str) -> Result<(), std::io::Error> {
        if !self.allowed_tools.insert(tool.to_string()) {
            return Ok(());
        }
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut sorted: Vec<&String> = self.allowed_tools.iter().collect();
        sorted.sort();
        let body = serde_yaml::to_string(&sorted).unwrap_or_default();
        fs::write(path, body)
    }

    /// Loaded tool names as `&'static str` for broker seeding.
    pub fn as_static_names(&self) -> Vec<&'static str> {
        self.allowed_tools
            .iter()
            .filter_map(|name| ToolName::parse(name).map(|n| n.0))
            .collect()
    }
}
