//! Migration schemas — machine-readable types for golden task runs.
//!
//! These are the contract between the skill-driven workflow
//! and future Rust automation. Every migration run produces these artifacts
//! as JSON files for reproducibility and audit.

pub mod evidence;
pub mod manifest;
pub mod plan;
pub mod report;

pub use evidence::{Artifact, ArtifactKind, Evidence, TestResult, TokenUsage};
pub use manifest::{RunManifest, SourceInfo, TargetInfo};
pub use plan::{MigrationPlan, MigrationStep};
pub use report::{
    AssertionCategory, AssertionResult, Deviation, DeviationSeverity, ParityReport, Verdict,
};
