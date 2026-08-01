//! Upstream provider errors must keep their HTTP status so the streamed
//! `retryable` flag can be decided — otherwise an Anthropic overload (529)
//! surfaces as a generic non-retryable internal error.

use mewcode_engine::EngineError;
use mewcode_engine::agent::map_stream_error;
use mewcode_engine::error::engine_error_parts;
use mewcode_protocol::event::ErrorCode;

fn stream_error(status: u16, body: &str) -> rig_core::agent::StreamingError {
    rig_core::agent::StreamingError::Completion(
        rig_core::completion::CompletionError::from_http_response(
            http::StatusCode::from_u16(status).unwrap(),
            body,
        ),
    )
}

#[test]
fn overload_status_surfaces_as_retryable_upstream() {
    // 529 is Anthropic's overloaded_error — the motivating case.
    let engine = map_stream_error(&stream_error(
        529,
        r#"{"error":{"type":"overloaded_error"}}"#,
    ));
    let (code, message, retryable) = engine_error_parts(&engine);
    assert_eq!(code, ErrorCode::Upstream);
    assert!(retryable, "529 must be marked retryable");
    // The provider body stays server-side; the client only sees the status.
    assert!(!message.contains("overloaded_error"));
    assert_eq!(message, "provider returned HTTP 529");
}

#[test]
fn statusless_error_falls_back_to_internal() {
    let error = rig_core::agent::StreamingError::Completion(
        rig_core::completion::CompletionError::from_provider_body("boom"),
    );
    let engine = map_stream_error(&error);
    assert!(matches!(engine, EngineError::Other(_)));
    let (code, _, retryable) = engine_error_parts(&engine);
    assert_eq!(code, ErrorCode::Internal);
    assert!(!retryable);
}
