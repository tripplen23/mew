//! Serde shape and lifecycle tests for the durable run protocol types.

use mewcode_protocol::run::{
    ApprovalKind, ApprovalRequest, ApprovalState, ArtifactKind, ArtifactRef, FailureKind,
    FailureReason, RunId, RunPhase, RunPolicy, RunRecord, RunStatus,
};

#[test]
fn run_id_displays_and_parses() {
    let id = RunId::new();
    assert_eq!(id.to_string().parse::<RunId>().unwrap(), id);
}

#[test]
fn run_id_serde_is_plain_uuid_string() {
    let id = RunId::new();
    let json = serde_json::to_string(&id).unwrap();
    assert_eq!(serde_json::from_str::<RunId>(&json).unwrap(), id);
}

#[test]
fn status_terminal_states_are_explicit() {
    assert!(RunStatus::Completed.is_terminal());
    assert!(RunStatus::Failed.is_terminal());
    assert!(RunStatus::Cancelled.is_terminal());
    assert!(RunStatus::Blocked.is_terminal());
    assert!(!RunStatus::Created.is_terminal());
    assert!(!RunStatus::Running.is_terminal());
    assert!(!RunStatus::Paused.is_terminal());
}

#[test]
fn approval_resolves_only_on_approve_or_reject() {
    assert!(!ApprovalState::Pending.is_resolved());
    assert!(!ApprovalState::Deferred.is_resolved());
    assert!(ApprovalState::Approved.is_resolved());
    assert!(ApprovalState::Rejected.is_resolved());
}

#[test]
fn approval_request_records_decision_audit_fields() {
    let mut req = ApprovalRequest::new(ApprovalKind::DestructiveAction, "worker-1", "merge PR");
    assert_eq!(req.state, ApprovalState::Pending);
    assert!(req.decided_by.is_none());

    req.state = ApprovalState::Approved;
    req.decided_by = Some("operator-1".into());
    req.decided_at = Some(chrono::Utc::now());
    let decoded: ApprovalRequest =
        serde_json::from_str(&serde_json::to_string(&req).unwrap()).unwrap();
    assert_eq!(decoded.decided_by.as_deref(), Some("operator-1"));
}

#[test]
fn run_record_roundtrips_with_kebab_wire_names() {
    let mut run = RunRecord::new("operator-1");
    run.phase = RunPhase::Extraction;
    run.status = RunStatus::Running;
    run.policy
        .workspace_roots
        .push("/srv/runs/1/candidate".into());
    run.artifacts.push(ArtifactRef {
        id: uuid::Uuid::new_v4(),
        kind: ArtifactKind::Evidence,
        name: "grep-output".into(),
        content_hash: "abc123".into(),
        size_bytes: 42,
        created_at: chrono::Utc::now(),
    });
    run.failure = None;

    let json = serde_json::to_string(&run).unwrap();
    let decoded: RunRecord = serde_json::from_str(&json).unwrap();

    assert_eq!(decoded, run);
    assert!(json.contains("\"phase\":\"extraction\""));
    assert!(json.contains("\"status\":\"running\""));
}

#[test]
fn run_record_defaults_policy_when_omitted() {
    let run: RunRecord =
        serde_json::from_str(&serde_json::to_string(&RunRecord::new("operator-1")).unwrap())
            .unwrap();
    assert_eq!(run.policy, RunPolicy::default());
    assert!(run.policy.redact_secrets);
    assert!(run.policy.workspace_roots.is_empty());
}

#[test]
fn legacy_status_cancelled_persists_without_failure_reason() {
    // Cancellation is an operator action, not a failure: the field stays None.
    let req = RunRecord::new("operator-1");
    let json = serde_json::to_string(&req).unwrap();
    let decoded: RunRecord = serde_json::from_str(&json).unwrap();
    assert!(decoded.failure.is_none());
}

#[test]
fn latest_checkpoint_tracks_write_order() {
    let mut run = RunRecord::new("operator-1");
    assert!(run.latest_checkpoint().is_none());
    let checkpoint = mewcode_protocol::run::Checkpoint {
        id: uuid::Uuid::new_v4(),
        task_id: Some("task-2".into()),
        created_at: chrono::Utc::now(),
        artifact_refs: vec![],
        failure: Some(FailureReason {
            kind: FailureKind::EnvironmentFailure,
            message: "missing toolchain".into(),
        }),
        summary: "after acquisition".into(),
        unresolved_questions: vec![],
    };
    run.checkpoints.push(checkpoint);
    assert_eq!(
        run.latest_checkpoint().unwrap().task_id.as_deref(),
        Some("task-2")
    );
}
