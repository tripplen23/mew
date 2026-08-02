//! Per-model token pricing as a fallback cost source.
//!
//! Rates live in `pricing.json` (USD per 1M tokens). The primary source of
//! truth is the provider itself: OpenCode Go reports per-request cost in its
//! `inference-cost` stream chunk, which rig surfaces as `Usage::cost` and the
//! harness prefers. This table only fills the gap for providers that do not
//! report cost (e.g. native OpenAI).

use std::collections::HashMap;

use mewcode_protocol::ModelId;

use crate::agent::TurnUsage;

const PRICING_JSON: &str = include_str!("../../pricing.json");

/// Per-model rates, USD per 1M tokens. `None` means the vendor charges no
/// separate rate for that bucket.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct ModelPrice {
    pub input: f64,
    pub output: f64,
    #[serde(default)]
    pub cache_read: Option<f64>,
    #[serde(default)]
    pub cache_creation: Option<f64>,
}

#[derive(Debug, Default, serde::Deserialize)]
struct PricingTable {
    models: HashMap<String, ModelPrice>,
}

/// Total cost in USD for a turn, or `None` when the model has no known price.
pub fn turn_cost_usd(model: ModelId, usage: TurnUsage) -> Option<f64> {
    let table: PricingTable = serde_json::from_str(PRICING_JSON)
        .expect("pricing.json must parse; corrupt asset");
    let price = table.models.get(model.as_str())?;

    // Charged input excludes tokens billed at the (cheaper) cache rate, so
    // cached reads aren't double-counted against `input`.
    let fresh_input = usage.input_tokens.saturating_sub(usage.cached_input_tokens);
    let mut cost = fresh_input as f64 * price.input
        + usage.output_tokens as f64 * price.output;

    if let Some(rate) = price.cache_read {
        cost += usage.cached_input_tokens as f64 * rate;
    }
    if let Some(rate) = price.cache_creation {
        cost += usage.cache_creation_input_tokens as f64 * rate;
    }

    Some(cost / 1_000_000.0)
}
