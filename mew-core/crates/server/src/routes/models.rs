use axum::Json;
use mewcode_protocol::{ModelId, ModelKind};
use serde::Serialize;

/// `GET /models` — returns the model registry.
#[derive(Serialize, utoipa::ToSchema)]
pub struct ModelEntry {
    /// Provider-id string used on the wire (e.g. `"minimax-m3"`).
    pub id: String,
    /// Human-friendly display name for the model picker.
    pub display_name: &'static str,
    /// Which OpenCode Go endpoint serves this model.
    pub kind: ModelKind,
}

/// `GET /models` — list every model reachable through an OpenCode Go
/// subscription.
#[utoipa::path(
    get,
    path = "/models",
    tag = "meta",
    responses(
        (status = 200, description = "Model registry", body = [ModelEntry]),
    ),
)]
pub async fn list_models() -> Json<Vec<ModelEntry>> {
    let entries = ModelId::ALL
        .iter()
        .map(|m| ModelEntry {
            id: m.as_str().to_string(),
            display_name: m.display_name(),
            kind: m.kind(),
        })
        .collect();
    Json(entries)
}
