//! Provider construction: key presence decides build success.

use mewcode_engine::EngineError;
use mewcode_engine::Provider;
use mewcode_engine::config::EngineConfig;
use mewcode_protocol::ModelId;

fn cfg_with(api_key: &str) -> EngineConfig {
    EngineConfig {
        api_key: api_key.to_string(),
        openai_api_key: None,
        openai_base_url: None,
        default_model: ModelId::DEFAULT,
        base_url: "http://localhost".into(),
    }
}

#[test]
fn opencodego_rejects_missing_or_blank_key() {
    for key in ["", "   ", "\n"] {
        assert!(
            matches!(
                Provider::for_model(ModelId::DEFAULT, &cfg_with(key)),
                Err(EngineError::MissingApiKey)
            ),
            "blank key {key:?} must surface as MissingApiKey"
        );
    }
}

#[test]
fn opencodego_present_key_builds_provider() {
    assert!(Provider::for_model(ModelId::DEFAULT, &cfg_with("k")).is_ok());
}
