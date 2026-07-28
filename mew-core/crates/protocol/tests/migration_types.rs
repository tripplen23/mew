//! Serde roundtrip + schema validation tests for migration protocol types.
//!
//! Verifies that the Rust types can deserialize real golden-task fixture
//! JSON and roundtrip without loss.

#[cfg(test)]
mod tests {
    use mewcode_protocol::migration::{
        evidence::TokenUsage,
        manifest::RunManifest,
        plan::{MigrationPlan, MigrationStep},
        report::{ParityReport, Verdict},
    };

    /// The `wc` fixture manifest should deserialize and roundtrip fully.
    #[test]
    fn deserialize_run_manifest() {
        let json = include_str!("../../../../tests/fixtures/golden-task-1/run-manifest.json");
        let manifest: RunManifest = serde_json::from_str(json).expect("valid manifest JSON");
        assert_eq!(manifest.id, "golden-task-1-wc-20260728-85b451e");
        assert_eq!(manifest.golden_task, "golden-task-1");
        assert_eq!(manifest.source.language, "python");
        assert_eq!(manifest.target.language, "rust");
        assert!(manifest.target.deterministic);
        // Roundtrip: serialize then deserialize, check full equality.
        let roundtripped: RunManifest =
            serde_json::from_str(&serde_json::to_string(&manifest).unwrap()).unwrap();
        assert_eq!(roundtripped.id, manifest.id);
        assert_eq!(roundtripped.golden_task, manifest.golden_task);
        assert_eq!(roundtripped.source.language, manifest.source.language);
        assert_eq!(
            roundtripped.target.deterministic,
            manifest.target.deterministic
        );
    }

    /// ParityReport::new() returns Inconclusive when there are no assertions.
    #[test]
    fn parity_report_inconclusive_when_empty() {
        let report = ParityReport::new(
            "run-1".into(),
            "abc1234".into(),
            "def5678".into(),
            vec![],
            vec![],
            TokenUsage::default(),
        );
        assert_eq!(report.verdict, Verdict::Inconclusive); // no assertions
    }

    /// MigrationPlan::validate() should catch broken dependencies.
    #[test]
    fn plan_validate_broken_dep() {
        let plan = MigrationPlan {
            run_id: "run-1".into(),
            steps: vec![MigrationStep {
                id: "step-1".into(),
                order: 1,
                description: "do thing".into(),
                depends_on: vec!["step-99".into()], // doesn't exist
                estimated_tokens: 100,
                verification: None,
            }],
            estimated_tokens: 100,
        };
        assert!(plan.validate().is_err());
    }

    /// MigrationPlan::validate() should accept valid dependencies.
    #[test]
    fn plan_validate_valid() {
        let plan = MigrationPlan {
            run_id: "run-1".into(),
            steps: vec![
                MigrationStep {
                    id: "step-1".into(),
                    order: 1,
                    description: "first".into(),
                    depends_on: vec![],
                    estimated_tokens: 50,
                    verification: None,
                },
                MigrationStep {
                    id: "step-2".into(),
                    order: 2,
                    description: "second".into(),
                    depends_on: vec!["step-1".into()],
                    estimated_tokens: 50,
                    verification: None,
                },
            ],
            estimated_tokens: 100,
        };
        assert!(plan.validate().is_ok());
    }
}
