//! Permission store — persistent "always allow" tool approvals.
//!
//! The interactive approval dialog's "Always allow" choice writes a scoped
//! rule here, and the broker is preloaded from here at startup, so rules
//! survive restarts (Claude Code "don't ask again for `ls`" / OpenCode
//! permission config parity). Stored as a YAML list in
//! `~/.config/mew/permissions.yaml`:
//!
//! ```yaml
//! - bash: ls        # allow `ls`
//! - write_file      # allow the whole tool (edit this file by hand)
//! ```

use std::fs;
use std::path::{Path, PathBuf};

use mewcode_protocol::tool::ToolName;

/// File name inside the Mew config directory.
const PERMISSIONS_FILE: &str = "permissions.yaml";

/// Where Mew stores its configuration. `None` when the platform has no
/// user config directory — a caller must not fall back to the working
/// directory, or an untrusted checkout could pre-authorize tools via a
/// relative `mew/permissions.yaml`.
fn config_dir() -> Option<PathBuf> {
    dirs::config_dir().map(|dir| dir.join("mew"))
}

/// In-memory view of the persistent always-allow rules. Each entry is a
/// `(tool, scope)` pair; `None` scope grants the whole tool.
#[derive(Debug, Clone, Default)]
pub struct PermissionStore {
    /// Scoped or whole-tool allow rules.
    pub allowed: Vec<(String, Option<String>)>,
}

impl PermissionStore {
    /// Load from the default config path. Missing or unparsable files load
    /// as empty — a broken permissions file must not brick tool runs. No
    /// user config dir means no persistent rules.
    pub fn load() -> Self {
        match config_dir() {
            Some(dir) => {
                Self::load_from(&dir.join(PERMISSIONS_FILE)).unwrap_or_else(|_| Self::default())
            }
            None => Self::default(),
        }
    }

    /// Load rules from `path` (also the test seam). An entry is `"<tool>"`
    /// (whole tool) or `"<tool>: <scope>"` (one command/path). Hand-written
    /// `- bash: ls` parses unquoted too; unknown tool names are dropped.
    pub fn load_from(path: &Path) -> Result<Self, std::io::Error> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let contents = fs::read_to_string(path)?;
        let entries: Vec<serde_yaml::Value> = serde_yaml::from_str(&contents).unwrap_or_default();
        let allowed: Vec<(String, Option<String>)> = entries
            .into_iter()
            .filter_map(|entry| {
                let (tool, scope) = match entry {
                    serde_yaml::Value::String(s) => match s.split_once(':') {
                        Some((tool, scope)) => {
                            (tool.trim().to_string(), Some(scope.trim().to_string()))
                        }
                        None => (s.trim().to_string(), None),
                    },
                    // `- bash: ls` unquoted arrives as a one-entry mapping.
                    serde_yaml::Value::Mapping(mapping) if mapping.len() == 1 => {
                        let (key, value) = mapping.into_iter().next()?;
                        let scope = match value {
                            serde_yaml::Value::String(s) => s,
                            _ => return None,
                        };
                        (key.as_str()?.to_string(), Some(scope))
                    }
                    _ => return None,
                };
                ToolName::parse(&tool)?;
                let scope = scope.filter(|s| !s.is_empty());
                Some((tool, scope))
            })
            .collect();
        Ok(Self { allowed })
    }

    /// Persist one more always-allow rule to the default config path.
    /// Missing parent dirs are created on first use; failures are returned
    /// to the caller (`allow_forever` treats them as best-effort).
    pub fn allow_forever(&mut self, tool: &str, scope: Option<&str>) -> Result<(), std::io::Error> {
        let Some(dir) = config_dir() else {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "no user config directory",
            ));
        };
        self.allow_forever_to(&dir.join(PERMISSIONS_FILE), tool, scope)
    }

    /// Persist to an explicit path (also the test seam, so tests never touch
    /// the user's real config directory). The file is written before the
    /// in-memory rule commits, so a failed write never leaves a phantom
    /// always-allow grant that vanishes on restart.
    pub fn allow_forever_to(
        &mut self,
        path: &Path,
        tool: &str,
        scope: Option<&str>,
    ) -> Result<(), std::io::Error> {
        let entry = (
            tool.to_string(),
            scope.filter(|s| !s.is_empty()).map(str::to_string),
        );
        if !self.allowed.contains(&entry) {
            let mut allowed = self.allowed.clone();
            allowed.push(entry);
            let mut lines: Vec<String> = allowed
                .iter()
                .map(|(tool, scope)| match scope {
                    Some(scope) => format!("{tool}: {scope}"),
                    None => tool.clone(),
                })
                .collect();
            lines.sort();
            let body = serde_yaml::to_string(&lines).unwrap_or_default();
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(path, body)?;
            self.allowed = allowed;
        }
        Ok(())
    }

    /// Loaded rules as `(tool, scope)` seeds for the approval broker.
    pub fn as_seed(&self) -> Vec<(&'static str, Option<&str>)> {
        self.allowed
            .iter()
            .filter_map(|(tool, scope)| Some((ToolName::parse(tool)?.0, scope.as_deref())))
            .collect()
    }
}
