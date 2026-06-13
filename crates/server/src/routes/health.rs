use axum::Json;
use serde_json::{json, Value};

/// `GET /health` — returns `{"ok":true}`.
pub async fn health() -> Json<Value> {
    Json(json!({ "ok": true, "service": "mewcode-server", "version": env!("CARGO_PKG_VERSION") }))
}
