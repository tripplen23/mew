use mewcode_protocol::{ModelId, ModelKind, ProviderId};
use mewcode_server::provider_catalog::{
    MAX_CATALOG_BYTES, discover_models, discover_zen_models, parse_models,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

async fn catalog_stub(
    status: &'static str,
    body: &'static str,
) -> (String, tokio::task::JoinHandle<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut request = Vec::new();
        let mut chunk = [0; 1024];
        while !request.windows(4).any(|window| window == b"\r\n\r\n") {
            let read = socket.read(&mut chunk).await.unwrap();
            if read == 0 {
                break;
            }
            request.extend_from_slice(&chunk[..read]);
        }
        let response = format!(
            "HTTP/1.1 {status}\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        );
        socket.write_all(response.as_bytes()).await.unwrap();
        String::from_utf8(request).unwrap()
    });
    (format!("http://{address}/models"), server)
}

#[test]
fn catalogs_preserve_ids_and_enrich_legacy_models() {
    let models = parse_models(
        r#"{"data":[
            {"id":"gpt-4.1","future":true},
            {"id":"gpt-future/model:v1","name":"Future\n Model"},
            {"id":""}
        ]}"#,
        ProviderId::OpenAi,
    )
    .unwrap();

    assert_eq!(models.len(), 2);
    assert_eq!(models[0].id, ModelId::Gpt41.as_str());
    assert_eq!(models[0].display_name, ModelId::Gpt41.display_name());
    assert_eq!(models[0].kind, ModelKind::OpenAi);
    assert_eq!(
        models[0].context_length,
        Some(ModelId::Gpt41.context_limit())
    );
    assert_eq!(models[1].id, "gpt-future/model:v1");
    assert_eq!(models[1].display_name, "Future Model");
    assert_eq!(models[1].context_length, None);

    let spoofed = parse_models(
        r#"{"data":[{"id":"safe-id","name":"safe\u202eexe"}]}"#,
        ProviderId::DeepSeek,
    )
    .unwrap();
    assert_eq!(spoofed[0].display_name, "safe exe");
}

#[test]
fn provider_defaults_and_openrouter_metadata_are_applied() {
    let opencode = parse_models(
        r#"{"data":[{"id":"future-code-model"}]}"#,
        ProviderId::OpenCodeGo,
    )
    .unwrap();
    assert_eq!(opencode[0].kind, ModelKind::OpenCodeGo);

    let openrouter = parse_models(
        r#"{"data":[{"id":"Vendor/Model:free","name":"Vendor Model","context_length":131072}]}"#,
        ProviderId::OpenRouter,
    )
    .unwrap();
    assert!(openrouter[0].is_free);
    assert_eq!(openrouter[0].context_length, Some(131_072));
}

#[test]
fn openai_catalog_excludes_clearly_non_chat_endpoint_families() {
    for id in [
        "text-embedding-3-large",
        "omni-moderation-latest",
        "gpt-image-1",
        "dall-e-3",
        "tts-1-hd",
        "whisper-1",
        "gpt-4o-transcribe",
        "gpt-4o-audio-preview",
        "gpt-4o-realtime-preview",
        "sora-2",
        "babbage-002",
        "davinci-002",
        "computer-use-preview",
        "codex-mini-latest",
    ] {
        let body = format!(r#"{{"data":[{{"id":"{id}"}}]}}"#);
        assert!(
            parse_models(&body, ProviderId::OpenAi).unwrap().is_empty(),
            "unsupported endpoint family remained selectable: {id}"
        );
    }

    for id in ["gpt-4.1", "o3", "future-general-model"] {
        let body = format!(r#"{{"data":[{{"id":"{id}"}}]}}"#);
        assert_eq!(parse_models(&body, ProviderId::OpenAi).unwrap()[0].id, id);
    }
}

#[tokio::test]
async fn discovery_sends_bearer_auth_without_rewriting_models() {
    let (url, server) = catalog_stub("200 OK", r#"{"data":[{"id":"future/model:v1"}]}"#).await;

    let models = discover_models(ProviderId::OpenCodeGo, &url, "test-provider-key")
        .await
        .unwrap();
    let request = server.await.unwrap().to_ascii_lowercase();

    assert!(request.contains("authorization: bearer test-provider-key\r\n"));
    assert_eq!(models[0].id, "future/model:v1");
}

#[tokio::test]
async fn discovery_returns_stable_public_errors_without_internal_details() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    drop(listener);
    let url = format!("http://{address}/internal/models");

    let error = discover_models(ProviderId::OpenCodeGo, &url, "test-key")
        .await
        .unwrap_err();
    assert_eq!(error, "model catalog unavailable");
    assert!(!error.contains(&address.to_string()));

    let (url, server) = catalog_stub("200 OK", "not json").await;
    let error = discover_models(ProviderId::OpenAi, &url, "test-key")
        .await
        .unwrap_err();
    server.await.unwrap();
    assert_eq!(error, "invalid model catalog");
    assert!(!error.contains(&url));
}

#[test]
fn catalogs_filter_malformed_rows_without_losing_valid_models() {
    let models = parse_models(
        r#"{"data":[
            {"id":"valid/model","name":"Valid"},
            {"name":"missing id"},
            {"id":42},
            {"id":"   "},
            {"id":" leading-space"},
            {"id":"trailing-space "},
            {"id":"bad\u202eid"},
            {"id":"also-valid","context_length":"unknown"}
        ]}"#,
        ProviderId::DeepSeek,
    )
    .unwrap();

    assert_eq!(
        models
            .iter()
            .map(|model| model.id.as_str())
            .collect::<Vec<_>>(),
        vec!["valid/model"]
    );
}

#[test]
fn malformed_or_oversized_catalogs_are_rejected() {
    for body in ["not json", r#"{}"#, r#"{"data":{}}"#] {
        assert!(parse_models(body, ProviderId::DeepSeek).is_err());
    }
    let body = " ".repeat(MAX_CATALOG_BYTES + 1);
    assert!(
        parse_models(&body, ProviderId::OpenAi)
            .unwrap_err()
            .contains("too large")
    );
}

#[test]
fn anthropic_catalog_uses_display_names_and_messages_transport() {
    let models = mewcode_server::provider_catalog::parse_anthropic_models(
        r#"{"data":[{"id":"claude-exact","display_name":"Claude\nExact"},{"id":""}]}"#,
    )
    .unwrap();

    assert_eq!(models.len(), 1);
    assert_eq!(models[0].id, "claude-exact");
    assert_eq!(models[0].display_name, "Claude Exact");
    assert_eq!(models[0].provider, ProviderId::Anthropic);
    assert_eq!(models[0].kind, ModelKind::AnthropicMessages);
}

#[test]
fn zen_catalog_joins_live_ids_to_verified_models_dev_transports() {
    let live = r#"{"data":[
        {"id":"claude-sonnet-4-6"},
        {"id":"gpt-5.4"},
        {"id":"glm-5.2"},
        {"id":"gemini-3.5-flash"},
        {"id":"unknown-live"}
    ]}"#;
    let metadata = r#"{"opencode":{
        "id":"opencode","name":"OpenCode Zen","npm":"@ai-sdk/openai-compatible",
        "models":{
            "claude-sonnet-4-6":{"id":"claude-sonnet-4-6","name":"Claude Sonnet 4.6","limit":{"context":1000000},"provider":{"npm":"@ai-sdk/anthropic"}},
            "gpt-5.4":{"id":"gpt-5.4","name":"GPT 5.4","limit":{"context":1050000},"provider":{"npm":"@ai-sdk/openai"}},
            "glm-5.2":{"id":"glm-5.2","name":"GLM 5.2","limit":{"context":131072}},
            "gemini-3.5-flash":{"id":"gemini-3.5-flash","name":"Gemini 3.5 Flash","limit":{"context":1048576},"provider":{"npm":"@ai-sdk/google"}},
            "metadata-only":{"id":"metadata-only","name":"Metadata only","provider":{"npm":"@ai-sdk/openai-compatible"}},
            "unknown-package":{"id":"unknown-live","name":"Unknown","provider":{"npm":"@ai-sdk/future"}}
        }
    }}"#;

    let models = mewcode_server::provider_catalog::parse_zen_models(live, metadata).unwrap();
    assert_eq!(
        models
            .iter()
            .map(|model| model.id.as_str())
            .collect::<Vec<_>>(),
        vec!["claude-sonnet-4-6", "gpt-5.4"]
    );
    assert_eq!(models[0].kind, ModelKind::AnthropicMessages);
    assert_eq!(models[1].kind, ModelKind::OpenAiResponses);
    assert_eq!(models[0].context_length, Some(1_000_000));
    assert!(
        models
            .iter()
            .all(|model| model.provider == ProviderId::OpenCodeZen)
    );
}

#[test]
fn zen_catalog_filters_unverified_or_malformed_rows_and_preserves_live_ids() {
    let live = r#"{"data":[
        {"id":"valid-live"},
        {"id":"missing-package"},
        {"id":"mismatched-id"},
        {"id":"malformed-metadata"},
        {"name":"missing live id"},
        {"id":42}
    ]}"#;
    let metadata = r#"{"opencode":{
        "npm":"@ai-sdk/openai-compatible",
        "models":{
            "valid-live":{"id":"valid-live","name":"Valid","provider":{"npm":"@ai-sdk/openai"}},
            "missing-package":{"id":"missing-package","name":"Missing package"},
            "mismatched-id":{"id":"substituted-id","provider":{"npm":"@ai-sdk/anthropic"}},
            "malformed-metadata":{"id":42,"provider":{"npm":"@ai-sdk/openai"}}
        }
    }}"#;

    let models = mewcode_server::provider_catalog::parse_zen_models(live, metadata).unwrap();

    assert_eq!(models.len(), 1);
    assert_eq!(models[0].id, "valid-live");
    assert_eq!(models[0].kind, ModelKind::OpenAiResponses);
}

#[tokio::test]
async fn zen_discovery_authenticates_only_live_catalog_and_preserves_exact_ids() {
    let (live_url, live_server) =
        catalog_stub("200 OK", r#"{"data":[{"id":"exact/model:v1"}]}"#).await;
    let (metadata_url, metadata_server) = catalog_stub(
        "200 OK",
        r#"{"opencode":{"models":{"exact/model:v1":{"id":"exact/model:v1","provider":{"npm":"@ai-sdk/openai"}}}}}"#,
    )
    .await;

    let models = discover_zen_models(&live_url, &metadata_url, "test-zen-key")
        .await
        .unwrap();
    let live_request = live_server.await.unwrap().to_ascii_lowercase();
    let metadata_request = metadata_server.await.unwrap().to_ascii_lowercase();

    assert!(live_request.contains("authorization: bearer test-zen-key\r\n"));
    assert!(!metadata_request.contains("authorization:"));
    assert_eq!(models[0].id, "exact/model:v1");
    assert_eq!(models[0].kind, ModelKind::OpenAiResponses);
}

#[tokio::test]
async fn anthropic_discovery_uses_native_auth_headers() {
    let (url, server) = catalog_stub(
        "200 OK",
        r#"{"data":[{"id":"claude-exact","display_name":"Claude Exact"}]}"#,
    )
    .await;

    let models = discover_models(ProviderId::Anthropic, &url, "test-anthropic-key")
        .await
        .unwrap();
    let request = server.await.unwrap().to_ascii_lowercase();

    assert!(request.contains("x-api-key: test-anthropic-key\r\n"));
    assert!(request.contains("anthropic-version: 2023-06-01\r\n"));
    assert!(!request.contains("authorization: bearer"));
    assert_eq!(models[0].kind, ModelKind::AnthropicMessages);
}
