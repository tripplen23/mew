//! Durable migration-run types.
//!
//! A [`RunRecord`] is the first-class durable object of a migration run —
//! the unit of work that owns policy, workspaces, artifacts, approvals,
//! checkpoints, and results (PRD sections 8.6, FR-1). It is deliberately
//! independent from the chat-session types in [`crate::message`] and the
//! server's session store: a run can be inspected without loading its model
//! transcript, and survives a process restart from its own durable state.

use std::fmt;
use std::str::FromStr;

/// Stable identifier of a migration run. A UUID newtype so run ids cannot
/// be confused with session ids in mixed flows (see PRD FR-1, M1).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize, utoipa::ToSchema,
)]
#[serde(transparent)]
pub struct RunId(pub uuid::Uuid);

impl RunId {
    /// Generate a fresh run id.
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4())
    }
}

impl Default for RunId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for RunId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for RunId {
    type Err = uuid::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(uuid::Uuid::parse_str(s)?))
    }
}

/// Which workflow phase a run is in. Follows PRD section 9's product
/// workflow, in order.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize, utoipa::ToSchema,
)]
#[serde(rename_all = "kebab-case")]
pub enum RunPhase {
    /// Collecting run input: source locator, intent, scope, and policy.
    Intake,
    /// Pinning and reproducing the source system.
    Acquisition,
    /// Static and dynamic investigation of the source.
    Reconnaissance,
    /// Extracting a behavioral contract backed by evidence.
    Extraction,
    /// Operator review and approval of the draft contract.
    Review,
    /// Splitting the approved contract into implementable slices.
    Planning,
    /// Evolving the candidate in reviewable slices.
    Implementation,
    /// Comparing the candidate against the approved contract.
    Verification,
    /// Producing final artifacts for handoff.
    Handoff,
}

/// Current lifecycle status of a [`RunRecord`].
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize, utoipa::ToSchema,
)]
#[serde(rename_all = "kebab-case")]
pub enum RunStatus {
    /// Run created but not yet started.
    Created,
    /// Actively executing its current phase.
    Running,
    /// Paused by an operator; resumable.
    Paused,
    /// Waiting on an operator decision or a missing prerequisite.
    Blocked,
    /// Finished successfully, including its handoff artifacts.
    Completed,
    /// Terminated by a failure. [`RunRecord::failure`] holds the reason.
    Failed,
    /// Cancelled by an operator.
    Cancelled,
}

impl RunStatus {
    /// Whether this status ends the run. `Blocked` is terminal in the sense
    /// of M1's acceptance criteria: the run cannot proceed without an
    /// operator decision, though that decision may resume it later.
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            RunStatus::Completed | RunStatus::Failed | RunStatus::Cancelled | RunStatus::Blocked
        )
    }
}

/// One kind of operator gated decision, per PRD section 4.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize, utoipa::ToSchema,
)]
#[serde(rename_all = "kebab-case")]
pub enum ApprovalKind {
    /// Owner relationship and access basis recorded at intake.
    Contract,
    /// Amending an already-approved behavioral contract.
    ContractAmendment,
    /// Accepting an observed deviation.
    AcceptedDeviation,
    /// A destructive or externally visible action (merge, publish, deploy).
    DestructiveAction,
    /// Final handoff of the run's output.
    Handoff,
    /// A semantic decision that needs operator judgment.
    SemanticDecision,
}

/// Decision state of an [`ApprovalRequest`].
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize, utoipa::ToSchema,
)]
#[serde(rename_all = "kebab-case")]
pub enum ApprovalState {
    /// Awaiting an operator decision.
    Pending,
    /// Approved by an operator.
    Approved,
    /// Rejected by an operator.
    Rejected,
    /// Deferred; blocks whatever depends on it until decided.
    Deferred,
}

impl ApprovalState {
    /// Whether no further decision is expected for this request.
    pub fn is_resolved(self) -> bool {
        matches!(self, ApprovalState::Approved | ApprovalState::Rejected)
    }
}

/// A request for an operator gated decision, plus its outcome once decided.
///
/// Audit record shape follows PRD FR-7: requester, timestamps, outgoing
/// kind, the operator who decided, and optional rationale.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
pub struct ApprovalRequest {
    /// Stable id of this request.
    pub id: uuid::Uuid,
    /// What the operator is deciding.
    pub kind: ApprovalKind,
    /// Current decision state.
    pub state: ApprovalState,
    /// Who or what asked for the decision (operator id or worker id).
    pub requested_by: String,
    /// When the request was raised.
    pub requested_at: chrono::DateTime<chrono::Utc>,
    /// Short description of what is being decided.
    pub summary: String,
    /// Artifacts (with content hashes) the decision applies to.
    #[serde(default)]
    pub artifact_refs: Vec<ArtifactRef>,
    /// Operator who decided, once decided.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decided_by: Option<String>,
    /// When the decision was recorded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decided_at: Option<chrono::DateTime<chrono::Utc>>,
    /// Optional decision rationale.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rationale: Option<String>,
}

impl ApprovalRequest {
    /// Build a pending request.
    pub fn new(
        kind: ApprovalKind,
        requested_by: impl Into<String>,
        summary: impl Into<String>,
    ) -> Self {
        Self {
            id: uuid::Uuid::new_v4(),
            kind,
            state: ApprovalState::Pending,
            requested_by: requested_by.into(),
            requested_at: chrono::Utc::now(),
            summary: summary.into(),
            artifact_refs: Vec::new(),
            decided_by: None,
            decided_at: None,
            rationale: None,
        }
    }
}

/// Stored kind of a durable run artifact.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize, utoipa::ToSchema,
)]
#[serde(rename_all = "kebab-case")]
pub enum ArtifactKind {
    /// Run manifest produced at intake.
    Manifest,
    /// Immutable source identity for the run.
    SourceLock,
    /// Pinned toolchain and environment inventory.
    EnvironmentInventory,
    /// Behavioral contract, any version.
    Contract,
    /// Evolution plan with its slices.
    Plan,
    /// Evidence entries (observations, test runs, logs).
    Evidence,
    /// Durable context checkpoint.
    Checkpoint,
    /// Verification or parity report.
    Report,
    /// Unified or raw diff.
    Diff,
    /// Other typed artifact.
    Other,
}

/// A reference to a durable run artifact: identity and content hash, not the
/// content itself. Evidence rules (PRD section 8.4) make the hash the point
/// of reference so consumers can verify what they are looking at.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
pub struct ArtifactRef {
    /// Stable artifact id.
    pub id: uuid::Uuid,
    /// What the artifact is.
    pub kind: ArtifactKind,
    /// Short human-readable name.
    pub name: String,
    /// SHA-256 hex digest of the stored content.
    pub content_hash: String,
    /// Stored content size in bytes.
    pub size_bytes: u64,
    /// When the artifact was written.
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// Healthy-run policy summary enforced by the runtime (PRD FR-11).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
pub struct RunPolicy {
    /// Filesystem roots candidate tools may touch.
    #[serde(default)]
    pub workspace_roots: Vec<String>,
    /// Network domain allowlist. Empty means no network.
    #[serde(default)]
    pub allowed_domains: Vec<String>,
    /// Command allowlist. Empty means no unlisted commands may run.
    #[serde(default)]
    pub allowed_commands: Vec<String>,
    /// Command denylist, checked regardless of the allowlist.
    #[serde(default)]
    pub denied_commands: Vec<String>,
    /// Secret-matching patterns that block tool output escaping the runtime.
    #[serde(default)]
    pub secret_patterns: Vec<String>,
    /// Whether captured output is redacted before storage.
    #[serde(default = "default_true")]
    pub redact_secrets: bool,
    /// Optional per-run token budget.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_budget: Option<u64>,
    /// Optional per-run disk budget in bytes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disk_budget_bytes: Option<u64>,
    /// Optional deadline, after which the run blocks or cancels.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deadline: Option<chrono::DateTime<chrono::Utc>>,
    /// Approval kinds that are mandatory for this run.
    #[serde(default)]
    pub required_approvals: Vec<ApprovalKind>,
}

fn default_true() -> bool {
    true
}

impl Default for RunPolicy {
    /// Safe defaults: untouched filesystem and network until the operator
    /// grants roots and domains explicitly.
    fn default() -> Self {
        Self {
            workspace_roots: Vec::new(),
            allowed_domains: Vec::new(),
            allowed_commands: Vec::new(),
            denied_commands: Vec::new(),
            secret_patterns: Vec::new(),
            redact_secrets: true,
            token_budget: None,
            disk_budget_bytes: None,
            deadline: None,
            required_approvals: Vec::new(),
        }
    }
}

/// Identity of the source system for this run (PRD section 8.1): the
/// strongest available revision or fingerprint, with any ambiguity recordable.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
pub struct SourceLock {
    /// Locator: repository URL, archive path, or live-system URL.
    pub locator: String,
    /// Pinned revision (commit hash or tag), when the source is versioned.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision: Option<String>,
    /// Strongest available live identity (release, deployment id, fingerprint).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fingerprint: Option<String>,
    /// Capture time for live systems; pin time for versioned sources.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub captured_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// Why a run failed, using the failure taxonomy the runtime and worker
/// tools share (PRD FR-11, M6).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize, utoipa::ToSchema,
)]
#[serde(rename_all = "kebab-case")]
pub enum FailureKind {
    /// Defect in the source (baseline) system.
    BaselineDefect,
    /// Defect in the evolved candidate.
    CandidateDefect,
    /// Environment problem: missing tooling, credentials, or services.
    EnvironmentFailure,
    /// Behaviour differed between repeat runs without a code difference.
    Nondeterminism,
    /// A policy rule was violated.
    PolicyViolation,
    /// An intentional, approved change that verification cannot score.
    IntentionalChange,
    /// Not enough evidence to draw a conclusion.
    Inconclusive,
    /// Runtime-internal failure (crash, storage error).
    Internal,
}

/// A structured failure with its classification and a human-readable message.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
pub struct FailureReason {
    /// Machine-readable classification consumers branch on.
    pub kind: FailureKind,
    /// Sanitised human-readable message.
    pub message: String,
}

/// A durable handoff snapshot: what a fresh worker needs to continue the run
/// without the original transcript (PRD section 8.12, FR-16).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
pub struct Checkpoint {
    /// Stable checkpoint id.
    pub id: uuid::Uuid,
    /// Task this checkpoint describes, when within a task.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
    /// When the checkpoint was written.
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// Artifacts this checkpoint references.
    #[serde(default)]
    pub artifact_refs: Vec<ArtifactRef>,
    /// Current failure classification, if the run is failing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure: Option<FailureReason>,
    /// Compact summary of decisions and discoveries since the last checkpoint.
    #[serde(default)]
    pub summary: String,
    /// Questions still open when the checkpoint was written.
    #[serde(default)]
    pub unresolved_questions: Vec<String>,
}

/// The durable state of one migration run. First-class object, independent
/// of chat sessions: everything an operator or a fresh worker needs to
/// inspect or resume the run lives here (not in a transcript).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
pub struct RunRecord {
    /// Stable run identifier.
    pub id: RunId,
    /// Operator or owner the run belongs to.
    pub owner: String,
    /// Current workflow phase.
    pub phase: RunPhase,
    /// Current lifecycle status.
    pub status: RunStatus,
    /// Immutable source identity, once acquisition has pinned it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_lock: Option<SourceLock>,
    /// Candidate workspace root owned by this run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub candidate_workspace: Option<String>,
    /// Policy summary enforced by the runtime.
    pub policy: RunPolicy,
    /// Id of the task currently executing, when inside a task.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
    /// When the run record was created.
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// Last time any part of the record changed.
    pub updated_at: chrono::DateTime<chrono::Utc>,
    /// Last heartbeat from the executor (stale-run detection).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub heartbeat_at: Option<chrono::DateTime<chrono::Utc>>,
    /// Checkpoints in write order.
    #[serde(default)]
    pub checkpoints: Vec<Checkpoint>,
    /// Artifacts the run has produced.
    #[serde(default)]
    pub artifacts: Vec<ArtifactRef>,
    /// Approval requests raised during the run, in raise order.
    #[serde(default)]
    pub approvals: Vec<ApprovalRequest>,
    /// Failure classification, set when the status is [`RunStatus::Failed`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure: Option<FailureReason>,
}

impl RunRecord {
    /// Build a new run in the [`RunPhase::Intake`] phase with a fresh id.
    pub fn new(owner: impl Into<String>) -> Self {
        let now = chrono::Utc::now();
        Self {
            id: RunId::new(),
            owner: owner.into(),
            phase: RunPhase::Intake,
            status: RunStatus::Created,
            source_lock: None,
            candidate_workspace: None,
            policy: RunPolicy::default(),
            task_id: None,
            created_at: now,
            updated_at: now,
            heartbeat_at: None,
            checkpoints: Vec::new(),
            artifacts: Vec::new(),
            approvals: Vec::new(),
            failure: None,
        }
    }

    /// The most recently written checkpoint, if any.
    pub fn latest_checkpoint(&self) -> Option<&Checkpoint> {
        self.checkpoints.last()
    }
}
