//! Integration tests for `mewcode_protocol::model`.

use std::collections::HashSet;

use mewcode_protocol::{ModelEntry, ModelId, ModelKind, ModelRef, ProviderId};

#[test]
fn all_have_unique_strs() {
    let mut seen = HashSet::new();
    for m in ModelId::ALL {
        assert!(seen.insert(m.as_str()), "duplicate: {:?}", m);
    }
}

#[test]
fn default_is_deepseek_v4_flash() {
    assert_eq!(ModelId::default(), ModelId::DeepSeekV4Flash);
}

#[test]
fn parse_known() {
    assert_eq!("minimax-m3".parse::<ModelId>().unwrap(), ModelId::MiniMaxM3);
    assert_eq!("MiniMax M3".parse::<ModelId>().unwrap(), ModelId::MiniMaxM3);
}

#[test]
fn parse_unknown() {
    assert!("gpt-99".parse::<ModelId>().is_err());
}

#[test]
fn serde_roundtrip() {
    for m in ModelId::ALL {
        let s = serde_json::to_string(m).unwrap();
        let back: ModelId = serde_json::from_str(&s).unwrap();
        assert_eq!(m, &back, "serde roundtrip failed for {m:?}");
    }
}

#[test]
fn model_ref_preserves_builtin_wire_strings() {
    for model in ModelId::ALL {
        let reference = mewcode_protocol::ModelRef::from(*model);
        assert_eq!(
            serde_json::to_string(&reference).unwrap(),
            serde_json::to_string(model).unwrap()
        );
    }
}

#[test]
fn dynamic_provider_model_refs_roundtrip_without_rewriting_raw_ids() {
    for (reference, provider, kind, wire) in [
        (
            ModelRef::open_code_go("future/model:v1").unwrap(),
            ProviderId::OpenCodeGo,
            ModelKind::OpenCodeGo,
            "opencode-go::future/model:v1",
        ),
        (
            ModelRef::openai("gpt-future").unwrap(),
            ProviderId::OpenAi,
            ModelKind::OpenAi,
            "openai::gpt-future",
        ),
        (
            ModelRef::deepseek("deepseek-future").unwrap(),
            ProviderId::DeepSeek,
            ModelKind::DeepSeek,
            "deepseek::deepseek-future",
        ),
        (
            ModelRef::openrouter("openrouter::vendor/model").unwrap(),
            ProviderId::OpenRouter,
            ModelKind::OpenRouter,
            "openrouter::openrouter::vendor/model",
        ),
    ] {
        let json = serde_json::to_string(&reference).unwrap();
        let decoded: ModelRef = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, reference);
        assert_eq!(decoded.provider(), provider);
        assert_eq!(decoded.kind(), kind);
        assert_eq!(decoded.to_string(), wire);
    }
}

#[test]
fn model_ref_rejects_ambiguous_or_empty_dynamic_ids() {
    assert!(serde_json::from_str::<ModelRef>("\"unknown/model\"").is_err());
    for prefix in ["opencode-go::", "openai::", "deepseek::", "openrouter::"] {
        assert!(serde_json::from_str::<ModelRef>(&format!("\"{prefix}\"")).is_err());
    }
    assert!(
        serde_json::from_str::<ModelRef>("\"MiniMax M3\"").is_err(),
        "persistence accepts exact built-in wire ids only"
    );
}

#[test]
fn namespaced_legacy_models_canonicalize_without_cross_provider_rewrites() {
    assert_eq!(
        ModelRef::open_code_go("minimax-m3").unwrap(),
        ModelRef::BuiltIn(ModelId::MiniMaxM3)
    );
    assert_eq!(
        ModelRef::openai("gpt-4.1").unwrap(),
        ModelRef::BuiltIn(ModelId::Gpt41)
    );
    assert_eq!(
        "opencode-go::minimax-m3".parse::<ModelRef>().unwrap(),
        ModelRef::BuiltIn(ModelId::MiniMaxM3)
    );
    assert_eq!(
        "openai::gpt-4.1".parse::<ModelRef>().unwrap(),
        ModelRef::BuiltIn(ModelId::Gpt41)
    );
    assert_eq!(
        "deepseek::gpt-4o".parse::<ModelRef>().unwrap(),
        ModelRef::deepseek("gpt-4o").unwrap()
    );
    assert_eq!(
        "openai::Vendor/Future:v1".parse::<ModelRef>().unwrap(),
        ModelRef::openai("Vendor/Future:v1").unwrap()
    );
}

#[test]
fn registry_rows_bind_identity_to_their_provider() {
    let mismatched = ModelEntry {
        id: ModelId::Gpt4o.as_str().into(),
        display_name: "metadata is not identity".into(),
        provider: ProviderId::DeepSeek,
        kind: ModelKind::DeepSeek,
        context_length: None,
        is_free: false,
    };
    assert_eq!(
        mismatched.model_ref().unwrap(),
        ModelRef::deepseek(ModelId::Gpt4o.as_str()).unwrap()
    );

    let openrouter = ModelEntry {
        id: "openai/gpt-4o".into(),
        display_name: "arbitrary label".into(),
        provider: ProviderId::OpenRouter,
        kind: ModelKind::OpenRouter,
        context_length: Some(128_000),
        is_free: false,
    };
    assert_eq!(
        openrouter.model_ref().unwrap(),
        ModelRef::openrouter("openai/gpt-4o").unwrap()
    );

    let dynamic = ModelEntry {
        id: "gpt-future".into(),
        display_name: "Future".into(),
        provider: ProviderId::OpenAi,
        kind: ModelKind::OpenAi,
        context_length: None,
        is_free: false,
    };
    assert_eq!(
        dynamic.model_ref().unwrap(),
        ModelRef::openai("gpt-future").unwrap()
    );
}

#[test]
fn openrouter_provider_has_stable_wire_and_display_names() {
    let provider = mewcode_protocol::ProviderId::OpenRouter;
    assert_eq!(serde_json::to_string(&provider).unwrap(), "\"open-router\"");
    assert_eq!(provider.to_string(), "OpenRouter");
}

#[test]
fn zen_and_anthropic_identities_and_transport_wires_are_stable() {
    for (model, provider, wire) in [
        (
            ModelRef::open_code_zen("Vendor/Exact:v1").unwrap(),
            ProviderId::OpenCodeZen,
            "opencode-zen::Vendor/Exact:v1",
        ),
        (
            ModelRef::anthropic("claude-exact").unwrap(),
            ProviderId::Anthropic,
            "anthropic::claude-exact",
        ),
    ] {
        assert_eq!(model.provider(), provider);
        assert_eq!(model.raw_id(), wire.split_once("::").unwrap().1);
        assert_eq!(model.to_string(), wire);
        assert_eq!(wire.parse::<ModelRef>().unwrap(), model);
    }

    assert_eq!(
        serde_json::to_string(&ProviderId::OpenCodeZen).unwrap(),
        "\"open-code-zen\""
    );
    assert_eq!(
        serde_json::to_string(&ProviderId::Anthropic).unwrap(),
        "\"anthropic\""
    );
    assert_eq!(
        serde_json::to_string(&ModelKind::OpenAiResponses).unwrap(),
        "\"open-ai-responses\""
    );
    assert_eq!(
        serde_json::to_string(&ModelKind::OpenCodeZen).unwrap(),
        "\"open-code-zen\""
    );
}

#[test]
fn zen_and_anthropic_reject_empty_ids_and_preserve_legacy_wires() {
    for value in ["opencode-zen::", "anthropic::"] {
        assert!(value.parse::<ModelRef>().is_err());
    }
    assert_eq!(
        serde_json::to_string(&ModelKind::OpenAi).unwrap(),
        "\"open-ai\""
    );
    assert_eq!(
        serde_json::to_string(&ModelKind::AnthropicMessages).unwrap(),
        "\"anthropic-messages\""
    );
    assert_eq!(
        serde_json::to_string(&ModelId::MiniMaxM3).unwrap(),
        "\"minimax-m3\""
    );
}

#[test]
fn provider_transport_compatibility_is_explicit() {
    assert!(mewcode_protocol::model::provider_supports_kind(
        ProviderId::OpenCodeZen,
        ModelKind::AnthropicMessages,
    ));
    assert!(mewcode_protocol::model::provider_supports_kind(
        ProviderId::OpenCodeZen,
        ModelKind::OpenAiResponses,
    ));
    assert!(mewcode_protocol::model::provider_supports_kind(
        ProviderId::OpenCodeZen,
        ModelKind::OpenCodeZen,
    ));
    assert!(mewcode_protocol::model::provider_supports_kind(
        ProviderId::Anthropic,
        ModelKind::AnthropicMessages,
    ));
    assert!(!mewcode_protocol::model::provider_supports_kind(
        ProviderId::Anthropic,
        ModelKind::OpenAiResponses,
    ));
    assert!(!mewcode_protocol::model::provider_supports_kind(
        ProviderId::OpenCodeZen,
        ModelKind::OpenAi,
    ));
}
