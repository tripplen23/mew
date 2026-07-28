//! Parity verification report — compares baseline against candidate.
//!
//! The final output of a migration run. Answers: did we preserve behavior?

use serde::{Deserialize, Serialize};

use super::evidence::TokenUsage;

/// Top-level parity report for a migration run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParityReport {
    /// Run manifest this report belongs to.
    pub run_id: String,
    /// Baseline version identifier (commit hash or tag).
    pub baseline_version: String,
    /// Candidate version identifier (commit hash or tag).
    pub candidate_version: String,
    /// Per-assertion results comparing baseline vs candidate.
    pub assertions: Vec<AssertionResult>,
    /// Deviations that cannot be attributed to a single assertion.
    #[serde(default)]
    pub deviations: Vec<Deviation>,
    /// Token consumption for the full run.
    pub token_usage: TokenUsage,
    /// Overall verdict.
    pub verdict: Verdict,
}

/// Result of a single behavioral assertion check.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssertionResult {
    /// Unique assertion identifier.
    pub id: String,
    /// Human-readable description of what was tested.
    pub description: String,
    /// Whether the candidate matched the baseline.
    pub passed: bool,
    /// Category for grouping (functional, performance, error_handling, etc.).
    #[serde(default)]
    pub category: Option<AssertionCategory>,
    /// Baseline behavior description.
    pub baseline_behavior: String,
    /// Candidate behavior description.
    pub candidate_behavior: String,
    /// Optional fix suggestion if this failed.
    pub suggestion: Option<String>,
}

/// Classification of what an assertion tests.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssertionCategory {
    /// Correctness of output or return values.
    Functional,
    /// Runtime performance (latency, throughput).
    Performance,
    /// Error handling and edge cases.
    ErrorHandling,
    /// Side effects (filesystem, network, database).
    SideEffect,
    /// Other category.
    Other,
}

/// A deviation observed during verification that isn't covered by assertions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Deviation {
    /// Human-readable identifier.
    pub id: String,
    /// What was observed in the baseline.
    pub description: String,
    /// What was observed in the candidate.
    pub baseline_behavior: String,
    /// What was observed in the candidate.
    pub candidate_behavior: String,
    /// Severity.
    pub severity: DeviationSeverity,
    /// Whether this deviation is explained and acceptable.
    #[serde(default)]
    pub explained: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeviationSeverity {
    /// Expected and documented. Not a defect.
    Expected,
    /// Minor difference, no behavioral impact.
    Minor,
    /// Meaningful difference that warrants investigation.
    Major,
    /// Candidate is incorrect, fails the migration.
    Blocker,
}

/// Overall migration run verdict.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    /// All assertions passed, no unexplained deviations.
    Passed,
    /// One or more assertions failed or unexplained deviations exist.
    Failed,
    /// Cannot determine (missing evidence, environment issue).
    Inconclusive,
}
