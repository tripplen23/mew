//! Provider construction: key presence decides build success.

use mewcode_engine::EngineError;
use mewcode_engine::Provider;
use mewcode_engine::config::EngineConfig;
use mewcode_protocol::ModelId;

fn cfg_with(api_key: &str) -> EngineConfig {
    EngineConfig {
        api_key: api_key.to_string(),
        opencode_zen_api_key: None,
        openai_api_key: None,
        openai_base_url: None,
        anthropic_api_key: None,
        deepseek_api_key: None,
        openrouter_api_key: None,
        default_model: ModelId::DEFAULT,
        base_url: "http://localhost".into(),
    }
}

#[test]
fn opencodego_rejects_missing_or_blank_key() {
    for key in ["", "   ", "\n"] {
        assert!(
            matches!(
                Provider::for_model(&ModelId::DEFAULT.into(), &cfg_with(key)),
                Err(EngineError::MissingApiKey)
            ),
            "blank key {key:?} must surface as MissingApiKey"
        );
    }
}

#[test]
fn opencodego_present_key_builds_provider() {
    assert!(Provider::for_model(&ModelId::DEFAULT.into(), &cfg_with("k")).is_ok());
}

#[test]
fn dynamic_provider_models_route_with_provider_credentials() {
    let mut cfg = cfg_with("opencode-key");
    cfg.openai_api_key = Some("openai-key".into());
    cfg.deepseek_api_key = Some("deepseek-key".into());

    assert!(matches!(
        Provider::for_model(
            &mewcode_protocol::ModelRef::open_code_go("future-code-model").unwrap(),
            &cfg
        ),
        Ok(Provider::OpenCodeGo(_))
    ));
    assert!(matches!(
        Provider::for_model(
            &mewcode_protocol::ModelRef::openai("gpt-future").unwrap(),
            &cfg
        ),
        Ok(Provider::OpenAi(_))
    ));
    assert!(matches!(
        Provider::for_model(
            &mewcode_protocol::ModelRef::deepseek("deepseek-future").unwrap(),
            &cfg
        ),
        Ok(Provider::DeepSeek(_))
    ));
}

#[test]
fn openrouter_requires_its_key_and_builds_compatible_provider() {
    let model = mewcode_protocol::ModelRef::openrouter("Vendor/Exact:free").unwrap();
    let mut cfg = cfg_with("unused");
    for key in ["", "   ", "\n"] {
        cfg.openrouter_api_key = Some(key.into());
        assert!(matches!(
            Provider::for_model(&model, &cfg),
            Err(EngineError::MissingNativeApiKey("OPENROUTER_API_KEY"))
        ));
    }

    cfg.openrouter_api_key = Some("test-openrouter-key".into());
    assert!(matches!(
        Provider::for_model(&model, &cfg),
        Ok(Provider::OpenRouter(_))
    ));
    assert_eq!(model.raw_id(), "Vendor/Exact:free");
}

#[test]
fn native_anthropic_and_zen_transports_use_provider_credentials() {
    use mewcode_protocol::{ModelKind, ModelRef};

    let mut cfg = cfg_with("go-key");
    cfg.anthropic_api_key = Some("anthropic-key".into());
    cfg.opencode_zen_api_key = Some("zen-key".into());

    assert!(matches!(
        Provider::for_model_kind(
            &ModelRef::anthropic("claude-exact").unwrap(),
            Some(ModelKind::AnthropicMessages),
            &cfg,
        ),
        Ok(Provider::Anthropic(_))
    ));
    for (kind, expected) in [
        (ModelKind::AnthropicMessages, "anthropic"),
        (ModelKind::OpenAiResponses, "responses"),
        (ModelKind::OpenCodeZen, "chat"),
    ] {
        let provider = Provider::for_model_kind(
            &ModelRef::open_code_zen("exact/model").unwrap(),
            Some(kind),
            &cfg,
        )
        .unwrap();
        assert!(matches!(
            (&provider, expected),
            (Provider::Anthropic(_), "anthropic")
                | (Provider::OpenAiResponses(_), "responses")
                | (Provider::OpenCodeZen(_), "chat")
        ));
    }
}

#[test]
fn invalid_provider_transport_fails_before_credential_lookup() {
    use mewcode_protocol::{ModelKind, ModelRef};

    let cfg = cfg_with("");
    let error = Provider::for_model_kind(
        &ModelRef::anthropic("claude-exact").unwrap(),
        Some(ModelKind::OpenAiResponses),
        &cfg,
    )
    .expect_err("incompatible transport must fail");
    assert!(matches!(
        error,
        EngineError::UnsupportedProviderTransport { .. }
    ));
}

#[test]
fn transport_snapshot_overrides_dynamic_identity_and_legacy_fallback_remains() {
    use mewcode_protocol::{ModelKind, ModelRef};

    let mut cfg = cfg_with("go-key");
    cfg.opencode_zen_api_key = Some("zen-key".into());
    let zen = ModelRef::open_code_zen("exact/model").unwrap();
    assert!(matches!(
        Provider::for_model_kind(&zen, Some(ModelKind::OpenAiResponses), &cfg),
        Ok(Provider::OpenAiResponses(_))
    ));
    assert!(matches!(
        Provider::for_model_kind(&ModelId::MiniMaxM3.into(), None, &cfg),
        Ok(Provider::Anthropic(_))
    ));
}
