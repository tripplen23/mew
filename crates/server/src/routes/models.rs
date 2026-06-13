use axum::Json;
use mewcode_protocol::{ModelId, ModelKind};
use serde::Serialize;

/// `GET /models` — returns the model registry.
#[derive(Serialize)]
pub struct ModelEntry {
    pub id: String,
    pub display_name: &'static str,
    pub kind: ModelKind,
}

pub async fn list_models() -> Json<Vec<ModelEntry>> {
    let entries = ModelId::ALL
        .iter()
        .map(|m| ModelEntry {
            id: m.provider_id().to_string(),
            display_name: m.display_name(),
            kind: m.kind(),
        })
        .collect();
    Json(entries)
}
