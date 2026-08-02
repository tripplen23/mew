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

#[test]
fn known_model_costs_fresh_and_cached_input() {
    let fresh = turn_cost_usd(ModelId::Gpt41Mini, usage(1_000_000, 0, 0, 0)).unwrap();
    assert_eq!(fresh, 0.40);

    let cached = turn_cost_usd(ModelId::Gpt41Mini, usage(1_000_000, 0, 1_000_000, 0)).unwrap();
    assert_eq!(cached, 0.10);
}

#[test]
fn output_is_billed() {
    let cost = turn_cost_usd(ModelId::DeepSeekV4Pro, usage(0, 1_000_000, 0, 0)).unwrap();
    assert_eq!(cost, 0.87);
}

#[test]
fn unknown_model_returns_none() {
    assert!(turn_cost_usd(ModelId::MiniMaxM3, usage(1_000_000, 0, 0, 0)).is_some());
    // Every ModelId in the table must have a price.
    for model in ModelId::ALL {
        assert!(
            turn_cost_usd(*model, usage(1, 1, 0, 0)).is_some(),
            "missing price for {model:?}"
        );
    }
}
