//! External tests for the streamed-error mapper and its retry policy.

use mewcode_engine::EngineError;
use mewcode_engine::error::{engine_error_parts, retryable_status};
use mewcode_protocol::event::ErrorCode;

#[test]
fn engine_error_parts_maps_codes_and_sanitises_messages() {
    let (code, message, retryable) = engine_error_parts(&EngineError::MissingApiKey);
    assert_eq!(code, ErrorCode::MissingApiKey);
    assert!(!retryable);
    assert!(!message.contains("OPENCODE_GO_API_KEY"));

    let (code, message, retryable) =
        engine_error_parts(&EngineError::MissingNativeApiKey("OPENAI_API_KEY"));
    assert_eq!(code, ErrorCode::MissingApiKey);
    assert!(!retryable);
    assert!(message.contains("OPENAI_API_KEY"));

    // Upstream status: 429 is retryable, 401 is not; the provider body is
    // never leaked into the client-facing message.
    let (code, message, retryable) = engine_error_parts(&EngineError::UpstreamStatus {
        status: 429,
        body: "rate limit; key=sk-secret-123".into(),
    });
    assert_eq!(code, ErrorCode::Upstream);
    assert!(retryable);
    assert!(!message.contains("sk-secret-123"));

    let (code, _, retryable) = engine_error_parts(&EngineError::UpstreamStatus {
        status: 401,
        body: "unauthorized".into(),
    });
    assert_eq!(code, ErrorCode::Upstream);
    assert!(!retryable);

    let (code, _, retryable) = engine_error_parts(&EngineError::ContextOverflow("too long".into()));
    assert_eq!(code, ErrorCode::ContextOverflow);
    assert!(!retryable);

    let (code, message, retryable) = engine_error_parts(&EngineError::Tool {
        tool: "bash".into(),
        message: "/secret/rm".into(),
    });
    assert_eq!(code, ErrorCode::ToolFailed);
    assert!(!retryable);
    assert!(!message.contains("/secret/"));

    for fallible in [
        EngineError::Serde(serde_json::from_str::<serde_json::Value>("{").unwrap_err()),
        EngineError::Other("opaque failure".into()),
    ] {
        let (code, _, retryable) = engine_error_parts(&fallible);
        assert_eq!(code, ErrorCode::Internal);
        assert!(!retryable);
    }
}

#[test]
fn retryable_status_marks_transient_codes_only() {
    for status in [408, 409, 425, 429, 500, 502, 503, 504, 529] {
        assert!(retryable_status(status), "{status} should be retryable");
    }
    for status in [200, 400, 401, 403, 404, 422] {
        assert!(
            !retryable_status(status),
            "{status} should not be retryable"
        );
    }
}
