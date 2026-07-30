//! Migration run manifest — describes what is being migrated.
//!
//! The single source of truth for a migration run, machine-readable
//! so skills produce consistent JSON and future Rust automation can ingest it.

use serde::{Deserialize, Serialize};

/// Top-level manifest for a single migration run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunManifest {
    /// Unique run identifier (UUID or human-readable slug).
    pub id: String,
    /// ISO-8601 timestamp when this run was initiated.
    pub created_at: String,
    /// Golden task this run belongs to (e.g. "golden-task-1").
    pub golden_task: String,
    /// What we are migrating from.
    pub source: SourceInfo,
    /// What we are migrating to.
    pub target: TargetInfo,
}

/// Information about the source system being migrated.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceInfo {
    /// Repository URL (e.g. <https://github.com/user/repo>).
    pub repo_url: String,
    /// Git commit hash pinned for reproducibility.
    pub commit: String,
    /// Programming language or system (e.g. "python", "bash").
    pub language: String,
    /// Entry point (main file, binary name, CLI subcommand).
    pub entry_point: Option<String>,
    /// Build or runtime dependencies as versioned strings.
    pub dependencies: Vec<String>,
}

/// Information about the target system.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TargetInfo {
    /// Target programming language (e.g. "rust").
    pub language: String,
    /// Framework or runtime (e.g. "axum", "clap").
    pub framework: Option<String>,
    /// Non-functional constraints (e.g. "no unsafe", "must pass clippy").
    pub constraints: Vec<String>,
    /// Whether the target must produce byte-identical output.
    #[serde(default)]
    pub deterministic: bool,
}
