//! Wire-shape tests for the mid-turn token accounting event.

use mewcode_protocol::StreamEvent;

#[test]
fn token_usage_serialises_kebab_case() {
    let event = StreamEvent::TokenUsage {
        input_tokens: 1200,
        output_tokens: 300,
        session_tokens: 4200,
        context_limit: 200_000,
    };
    let json = serde_json::to_string(&event).unwrap();
    assert!(json.contains("\"type\":\"token-usage\""));
    assert!(json.contains("\"session_tokens\":4200"));
    let back: StreamEvent = serde_json::from_str(&json).unwrap();
    match back {
        StreamEvent::TokenUsage {
            input_tokens,
            output_tokens,
            session_tokens,
            context_limit,
        } => {
            assert_eq!(input_tokens, 1200);
            assert_eq!(output_tokens, 300);
            assert_eq!(session_tokens, 4200);
            assert_eq!(context_limit, 200_000);
        }
        other => panic!("expected TokenUsage, got {other:?}"),
    }
}
