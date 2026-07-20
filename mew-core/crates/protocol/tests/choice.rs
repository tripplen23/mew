use mewcode_protocol::event::{ChoiceCancelReason, ChoiceOption, ChoiceRequest, ChoiceResponse};

#[test]
fn choice_request_round_trips_with_stable_option_ids() {
    let request = ChoiceRequest {
        request_id: "req-1".into(),
        title: "Pick one".into(),
        prompt: "Choose a path".into(),
        timeout_ms: 30_000,
        options: vec![ChoiceOption {
            id: "safe-id".into(),
            label: "User-facing label".into(),
            description: Some("Can change without changing id".into()),
        }],
    };

    let json = serde_json::to_string(&request).unwrap();
    let decoded: ChoiceRequest = serde_json::from_str(&json).unwrap();

    assert_eq!(decoded.options[0].id, "safe-id");
    assert_eq!(decoded.options[0].label, "User-facing label");
}

#[test]
fn choice_response_uses_option_id_or_cancel_reason() {
    let selected = ChoiceResponse::Selected {
        request_id: "req-1".into(),
        option_id: "safe-id".into(),
    };
    let cancelled = ChoiceResponse::Cancelled {
        request_id: "req-1".into(),
        reason: ChoiceCancelReason::NonInteractive,
    };

    assert!(
        serde_json::to_string(&selected)
            .unwrap()
            .contains("safe-id")
    );
    assert!(
        serde_json::to_string(&cancelled)
            .unwrap()
            .contains("non-interactive")
    );
}
