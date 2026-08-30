//! Pricing-table fallback: USD cost from token usage × per-model rates.

use mewcode_engine::agent::TurnUsage;
use mewcode_engine::helpers::pricing::turn_cost_usd;
use mewcode_protocol::ModelId;

fn usage(input: u64, output: u64, cached: u64, cache_creation: u64) -> TurnUsage {
    TurnUsage {
        input_tokens: input,
        output_tokens: output,
        cached_input_tokens: cached,
        cache_creation_input_tokens: cache_creation,
        ..TurnUsage::default()
    }
}

fn assert_cost_close(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() < 1e-9,
        "cost {actual} != {expected}"
    );
}

#[test]
fn known_model_costs_fresh_and_cached_input() {
    let model = ModelId::Gpt41Mini.into();
    let fresh = turn_cost_usd(&model, usage(1_000_000, 0, 0, 0)).unwrap();
    assert_cost_close(fresh, 0.40);

    let cached = turn_cost_usd(&model, usage(1_000_000, 0, 1_000_000, 0)).unwrap();
    assert_cost_close(cached, 0.10);
}

#[test]
fn output_is_billed() {
    let cost = turn_cost_usd(&ModelId::DeepSeekV4Pro.into(), usage(0, 1_000_000, 0, 0)).unwrap();
    assert_cost_close(cost, 0.87);
}

#[test]
fn every_model_has_a_price() {
    for model in ModelId::ALL {
        assert!(
            turn_cost_usd(&(*model).into(), usage(1, 1, 0, 0)).is_some(),
            "missing price for {model:?}"
        );
    }
}

#[test]
fn dynamic_provider_models_do_not_use_builtin_pricing_fallback() {
    for model in [
        mewcode_protocol::ModelRef::open_code_go("future-code-model").unwrap(),
        mewcode_protocol::ModelRef::openai("gpt-future").unwrap(),
        mewcode_protocol::ModelRef::deepseek("deepseek-future").unwrap(),
        mewcode_protocol::ModelRef::openrouter("openai/gpt-4.1-mini").unwrap(),
    ] {
        assert_eq!(turn_cost_usd(&model, usage(1_000_000, 1, 0, 0)), None);
    }
}
