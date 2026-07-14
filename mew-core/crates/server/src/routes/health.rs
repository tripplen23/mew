use axum::Json;
use serde::Serialize;

/// Response body for `GET /health`.
#[derive(Serialize, utoipa::ToSchema)]
pub struct HealthResponse {
    /// Always `true` for a live server.
    pub ok: bool,
    /// Service name (`"mewcode-server"`).
    pub service: &'static str,
    /// Crate version (from `CARGO_PKG_VERSION`).
    pub version: &'static str,
}

/// `GET /health` — liveness probe.
#[utoipa::path(
    get,
    path = "/health",
    tag = "meta",
    responses(
        (status = 200, description = "Server is alive", body = HealthResponse),
    ),
)]
pub async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        ok: true,
        service: "mewcode-server",
        version: env!("CARGO_PKG_VERSION"),
    })
}
