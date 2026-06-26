//! Canvas read endpoints. Surface the project's `.mewcode/canvas/`
//! graph and layout as JSON, mirroring the `LoadSessions` HTTP
//! pattern so the client crate does not need a direct dependency
//! on the engine.

use axum::Json;
use axum::extract::State;
use mewcode_engine::canvas::io;
use mewcode_protocol::canvas::{Graph, Layout};
use serde::Serialize;

use crate::AppError;
use crate::AppState;

/// `GET /canvas/graph` — return the current graph (or empty default
/// for a first-run project with no `.mewcode/canvas/graph.json`).
#[utoipa::path(
    get,
    path = "/canvas/graph",
    tag = "canvas",
    responses(
        (status = 200, description = "Current canvas graph", body = Graph),
        (status = 500, description = "Internal error", body = crate::openapi::ErrorResponse),
    ),
)]
pub async fn get_graph(State(state): State<AppState>) -> Result<Json<Graph>, AppError> {
    let graph = io::read_graph(state.config.canvas_project_root())
        .map_err(|e| AppError::Internal(format!("canvas graph load failed: {e}")))?;
    Ok(Json(graph))
}

/// `GET /canvas/layout` — return the current layout (positions +
/// theme; auto-layout is *not* applied here — the client decides
/// when to resolve missing positions, keeping the server stateless
/// over the wire).
#[utoipa::path(
    get,
    path = "/canvas/layout",
    tag = "canvas",
    responses(
        (status = 200, description = "Current canvas layout", body = Layout),
        (status = 500, description = "Internal error", body = crate::openapi::ErrorResponse),
    ),
)]
pub async fn get_layout(State(state): State<AppState>) -> Result<Json<Layout>, AppError> {
    let layout = io::read_layout(state.config.canvas_project_root())
        .map_err(|e| AppError::Internal(format!("canvas layout load failed: {e}")))?;
    Ok(Json(layout))
}

/// Convenience wrapper so `AppError` carries both fields when both
/// are useful (e.g. an OpenAPI batch endpoint later). Today each
/// handler returns just the field it advertises.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct CanvasSnapshot {
    /// Current graph.
    pub graph: Graph,
    /// Current layout.
    pub layout: Layout,
}
