//! Migration step-by-step plan.
//!
//! Produced by the planning skill; consumed by the execution loop.

use serde::{Deserialize, Serialize};

/// Ordered sequence of migration steps with dependency tracking.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationPlan {
    /// Run manifest this plan belongs to.
    pub run_id: String,
    /// Ordered steps. The agent should execute these in sequence,
    /// respecting dependencies.
    pub steps: Vec<MigrationStep>,
    /// Estimated total tokens for this run.
    pub estimated_tokens: u64,
}

/// A single step in the migration plan.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationStep {
    /// Unique identifier within this plan (e.g. "step-1", "parse-source").
    pub id: String,
    /// Execution order (1-based, gaps allowed for reordering).
    pub order: u32,
    /// Human-readable description the agent follows.
    pub description: String,
    /// IDs of steps that must complete before this one.
    #[serde(default)]
    pub depends_on: Vec<String>,
    /// Estimated token budget for this step.
    pub estimated_tokens: u64,
    /// How to verify this step succeeded (e.g. "cargo build", "diff output").
    pub verification: Option<String>,
}
