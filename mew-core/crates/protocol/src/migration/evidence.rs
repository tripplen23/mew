//! Evidence artifacts produced during a migration run.
//!
//! Captured incrementally during execution; the agent appends
//! diffs, test results, and token counts as it works.

use serde::{Deserialize, Serialize};

/// All evidence collected for a single migration run.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Evidence {
    /// Run manifest this evidence belongs to.
    pub run_id: String,
    /// File-level artifacts (source, diffs, logs).
    #[serde(default)]
    pub artifacts: Vec<Artifact>,
    /// Per-assertion test results from verification.
    #[serde(default)]
    pub test_results: Vec<TestResult>,
    /// Cumulative token usage.
    pub token_usage: TokenUsage,
}

/// A file artifact produced by the migration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Artifact {
    /// File path relative to the run workspace.
    pub path: String,
    /// Kind of artifact.
    pub kind: ArtifactKind,
    /// SHA-256 hash of the file content for integrity checking.
    pub content_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactKind {
    /// Source file produced by the migration.
    Source,
    /// Unified diff (before → after).
    Diff,
    /// Raw output from a build or test command.
    Log,
    /// Parsed test report.
    TestOutput,
    /// Any other artifact.
    Other,
}

/// Result of verifying a single behavioral assertion.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestResult {
    /// Maps to an assertion ID in the parity report.
    pub assertion_id: String,
    /// Whether the test passed.
    pub passed: bool,
    /// Captured stdout/stderr from the test.
    pub output: Option<String>,
    /// Wall-clock duration in milliseconds.
    pub duration_ms: u64,
}

/// Token consumption for a run (both prompt and completion).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TokenUsage {
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub total_tokens: u64,
}

impl TokenUsage {
    /// Merge another TokenUsage into self (for accumulating across steps).
    ///
    /// Recomputes `total_tokens` as `prompt + completion` so the
    /// invariant is preserved even if the incoming totals are inconsistent.
    pub fn merge(&mut self, other: &TokenUsage) {
        self.prompt_tokens += other.prompt_tokens;
        self.completion_tokens += other.completion_tokens;
        self.total_tokens = self.prompt_tokens + self.completion_tokens;
    }
}
