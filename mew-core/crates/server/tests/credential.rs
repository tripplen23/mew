use std::time::Duration;

use mewcode_server::credential::{
    ValidationError, classify_openrouter_status, validate_openrouter_key_at,
};
use reqwest::StatusCode;

#[test]
fn openrouter_validation_statuses_remain_distinguishable() {
    assert_eq!(classify_openrouter_status(StatusCode::OK), None);
    for status in [StatusCode::UNAUTHORIZED, StatusCode::FORBIDDEN] {
        assert!(matches!(
            classify_openrouter_status(status),
            Some(ValidationError::InvalidKey(_))
        ));
    }
    for status in [
        StatusCode::TOO_MANY_REQUESTS,
        StatusCode::INTERNAL_SERVER_ERROR,
        StatusCode::SERVICE_UNAVAILABLE,
    ] {
        assert!(matches!(
            classify_openrouter_status(status),
            Some(ValidationError::Unavailable(_))
        ));
    }
}

#[tokio::test]
async fn openrouter_validation_distinguishes_timeout() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/key", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        let _connection = listener.accept().await.unwrap();
        tokio::time::sleep(Duration::from_secs(1)).await;
    });

    let result = validate_openrouter_key_at("test-key", &url, Duration::from_millis(20)).await;

    assert!(matches!(
        result,
        Err(ValidationError::Unavailable(message)) if message.contains("timed out")
    ));
    server.abort();
}

#[tokio::test]
async fn openrouter_validation_distinguishes_network_failure() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    drop(listener);

    let result = validate_openrouter_key_at(
        "test-key",
        &format!("http://{address}/key"),
        Duration::from_secs(1),
    )
    .await;

    assert!(matches!(
        result,
        Err(ValidationError::Unavailable(message)) if message.contains("could not reach OpenRouter")
    ));
}

#[tokio::test]
async fn openrouter_validation_rejects_malformed_keys_locally() {
    for key in ["", "   ", "line\nbreak"] {
        assert!(matches!(
            validate_openrouter_key_at(key, "http://127.0.0.1:1/key", Duration::from_secs(1)).await,
            Err(ValidationError::InvalidKey(_))
        ));
    }
}

#[cfg(unix)]
#[test]
fn loading_legacy_credentials_repairs_file_and_directory_permissions() {
    use std::os::unix::fs::PermissionsExt;

    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("credentials.yaml");
    let credential = mewcode_protocol::credential::ProviderCredential::new(
        mewcode_protocol::ProviderId::Anthropic,
        "test-key".into(),
    );
    std::fs::write(&path, serde_yaml::to_string(&vec![credential]).unwrap()).unwrap();
    std::fs::set_permissions(temp.path(), std::fs::Permissions::from_mode(0o755)).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();

    let store = mewcode_server::credential::CredentialStore::load_at(&path).unwrap();

    assert!(
        store
            .credentials
            .contains_key(&mewcode_protocol::ProviderId::Anthropic)
    );
    assert_eq!(
        std::fs::metadata(temp.path()).unwrap().permissions().mode() & 0o777,
        0o700
    );
    assert_eq!(
        std::fs::metadata(path).unwrap().permissions().mode() & 0o777,
        0o600
    );
}

#[test]
fn failed_credential_persistence_removes_temporary_file() {
    let temp = tempfile::tempdir().unwrap();
    let target = temp.path().join("credentials.yaml");
    std::fs::create_dir(&target).unwrap();
    let mut store = mewcode_server::credential::CredentialStore::default();

    let result = store.store_at(
        mewcode_protocol::credential::ProviderCredential::new(
            mewcode_protocol::ProviderId::Anthropic,
            "test-key".into(),
        ),
        &target,
    );

    assert!(result.is_err());
    assert!(
        std::fs::read_dir(temp.path())
            .unwrap()
            .filter_map(Result::ok)
            .all(|entry| entry.path().extension().is_none_or(|ext| ext != "tmp"))
    );
}

#[test]
fn validation_never_buffers_or_reflects_provider_error_bodies() {
    let source = include_str!("../src/credential.rs");
    assert!(!source.contains(".text().await"));
}

#[test]
fn failed_credential_persistence_keeps_previous_in_memory_value() {
    let temp = tempfile::tempdir().unwrap();
    let invalid_parent = temp.path().join("not-a-directory");
    std::fs::write(&invalid_parent, "occupied").unwrap();
    let mut store = mewcode_server::credential::CredentialStore::default();
    store.credentials.insert(
        mewcode_protocol::ProviderId::OpenRouter,
        mewcode_protocol::credential::ProviderCredential::new(
            mewcode_protocol::ProviderId::OpenRouter,
            "previous".into(),
        ),
    );

    let result = store.store_at(
        mewcode_protocol::credential::ProviderCredential::new(
            mewcode_protocol::ProviderId::OpenRouter,
            "replacement".into(),
        ),
        &invalid_parent.join("credentials.yaml"),
    );

    assert!(result.is_err());
    assert_eq!(
        store.credentials[&mewcode_protocol::ProviderId::OpenRouter].api_key,
        "previous"
    );
}

#[tokio::test]
async fn provider_validation_rejects_malformed_keys_locally() {
    for provider in [
        mewcode_protocol::ProviderId::OpenCodeZen,
        mewcode_protocol::ProviderId::Anthropic,
    ] {
        for key in ["", "   ", "line\nbreak"] {
            assert!(matches!(
                mewcode_server::credential::validate_provider_key_at(
                    provider,
                    key,
                    "http://127.0.0.1:1/models",
                    Duration::from_secs(1),
                )
                .await,
                Err(ValidationError::InvalidKey(_))
            ));
        }
    }
}

#[tokio::test]
async fn anthropic_validation_sends_native_headers_without_bearer_auth() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/models", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut request = vec![0; 4096];
        let read = socket.read(&mut request).await.unwrap();
        socket
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\n{}")
            .await
            .unwrap();
        String::from_utf8_lossy(&request[..read]).to_ascii_lowercase()
    });

    mewcode_server::credential::validate_provider_key_at(
        mewcode_protocol::ProviderId::Anthropic,
        "test-anthropic-key",
        &url,
        Duration::from_secs(1),
    )
    .await
    .unwrap();
    let request = server.await.unwrap();
    assert!(request.contains("x-api-key: test-anthropic-key\r\n"));
    assert!(request.contains("anthropic-version: 2023-06-01\r\n"));
    assert!(!request.contains("authorization: bearer"));
}
