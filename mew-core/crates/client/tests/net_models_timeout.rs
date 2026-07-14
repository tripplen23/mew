//! Integration tests for `ApiClient::models`: the time-bounded, typed
//! `GET /models` path.
//!
//! These exercise the public `models()` API against real sockets — no mocks:
//!
//! - A closed/unused port yields a fast transport error, proving the call
//!   resolves to a `NetError` within its bound instead of hanging.
//! - A stub server returning `500` proves a non-success status maps to
//!   `NetError::Status`.
//!
//! The "stub" is a one-shot raw-TCP responder rather than a real HTTP
//! framework — enough to drive one `GET /models` and hand back a fixed status
//! line. Ceiling: it handles exactly one connection and ignores the request
//! body; upgrade to a real server harness if these tests ever need routing.

use std::time::Duration;

use mewcode_client::net::{ApiClient, NetError};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

/// Bind an ephemeral port, then drop the listener so connecting to it fails
/// fast with "connection refused" — a transport error, no waiting required.
async fn closed_port_url() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    drop(listener);
    format!("http://{addr}")
}

/// Spawn a one-shot stub that answers the first connection with `status` and an
/// empty body, then returns the base URL pointing at it.
async fn stub_status_url(status: &'static str) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let (mut sock, _) = listener.accept().await.unwrap();
        let mut buf = [0u8; 1024];
        let _ = sock.read(&mut buf).await; // drain the request line/headers
        let resp = format!("HTTP/1.1 {status}\r\nContent-Length: 0\r\n\r\n");
        let _ = sock.write_all(resp.as_bytes()).await;
        let _ = sock.flush().await;
    });
    format!("http://{addr}")
}

#[tokio::test]
async fn models_on_closed_port_resolves_to_neterror_within_bound() {
    let client = ApiClient::new(closed_port_url().await);

    // The request must settle well inside its 10 s budget. A 5 s outer guard
    // turns a hypothetical hang into a test failure instead of a wedge.
    let result = tokio::time::timeout(Duration::from_secs(5), client.models())
        .await
        .expect("models() hung past the bound instead of failing fast");

    assert!(
        matches!(result, Err(NetError::Transport(_))),
        "expected a transport NetError from a closed port, got {result:?}"
    );
}

#[tokio::test]
async fn models_on_500_maps_to_neterror_status() {
    let client = ApiClient::new(stub_status_url("500 Internal Server Error").await);

    let result = tokio::time::timeout(Duration::from_secs(5), client.models())
        .await
        .expect("models() hung instead of returning the stubbed status");

    match result {
        Err(NetError::Status(code)) => {
            assert_eq!(code.as_u16(), 500, "wrong status mapped: {code}");
        }
        other => panic!("expected NetError::Status(500), got {other:?}"),
    }
}
