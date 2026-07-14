// Feature: session-flow-and-engine-v0, provider routing matches the model kind
//
// For any `ModelId`, `Provider::for_model` builds the `Anthropic` variant
// exactly when `model.kind()` is `AnthropicMessages`, and the `OpenAi` variant
// exactly when it is `OpenAiChatCompletions`.

use mewcode_engine::Provider;
use mewcode_protocol::{ModelId, ModelKind};
use proptest::prelude::*;

/// Strategy over every supported model.
fn any_model() -> impl Strategy<Value = ModelId> {
    prop::sample::select(ModelId::ALL.to_vec())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    #[test]
    fn provider_variant_matches_model_kind(model in any_model()) {
        // Dummy credentials/base: provider construction is offline (no request).
        let provider = Provider::for_model(model, "test-key", "https://example.invalid")
            .expect("for_model is infallible");

        match model.kind() {
            ModelKind::AnthropicMessages => {
                prop_assert!(
                    matches!(provider, Provider::Anthropic(_)),
                    "expected Anthropic variant for {model:?}"
                );
            }
            ModelKind::OpenAiChatCompletions => {
                prop_assert!(
                    matches!(provider, Provider::OpenAi(_)),
                    "expected OpenAi variant for {model:?}"
                );
            }
        }
    }
}
