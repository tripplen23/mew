//! Integration tests for `ApiClient::providers`: the time-bounded, typed
//! `GET /providers` path.

use std::time::Duration;

use mewcode_client::net::{ApiClient, NetError};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

async fn closed_port_url() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    drop(listener);
    format!("http://{addr}")
}

async fn stub_status_url(status: &'static str) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let (mut sock, _) = listener.accept().await.unwrap();
        let mut buf = [0u8; 1024];
        let _ = sock.read(&mut buf).await;
        let resp = format!("HTTP/1.1 {status}\r\nContent-Length: 0\r\n\r\n");
        let _ = sock.write_all(resp.as_bytes()).await;
        let _ = sock.flush().await;
    });
    format!("http://{addr}")
}

#[tokio::test]
async fn providers_on_closed_port_resolves_to_neterror_within_bound() {
    let client = ApiClient::new(closed_port_url().await);

    let result = tokio::time::timeout(Duration::from_secs(5), client.providers())
        .await
        .expect("providers() hung past the bound instead of failing fast");

    assert!(
        matches!(result, Err(NetError::Transport(_))),
        "expected a transport NetError from a closed port, got {result:?}"
    );
}

#[tokio::test]
async fn providers_on_500_maps_to_neterror_status() {
    let client = ApiClient::new(stub_status_url("500 Internal Server Error").await);

    let result = tokio::time::timeout(Duration::from_secs(5), client.providers())
        .await
        .expect("providers() hung instead of returning the stubbed status");

    match result {
        Err(NetError::Status(code)) => {
            assert_eq!(code.as_u16(), 500, "wrong status mapped: {code}");
        }
        other => panic!("expected NetError::Status(500), got {other:?}"),
    }
}
