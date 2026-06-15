//! OpenAPI 3.1 spec for the mewcode server.
//!
//! Generated at compile time from handler `#[utoipa::path]` annotations and
//! the `#[derive(utoipa::ToSchema)]` types in the `routes/` and `store/`
//! modules. The spec is exposed as JSON at `/api-docs/openapi.json`; a
//! Swagger UI is mounted at `/swagger-ui` by [`mount_openapi`].

use serde::Serialize;
use utoipa::OpenApi;

use crate::routes;
use crate::store::{NewSession, Session, SessionSummary};

/// Wire shape of every error response.
///
/// [`crate::AppError`] implements `IntoResponse` and serialises to
/// `{"error": "<message>"}`; this struct is the OpenAPI-visible form of that
/// body so handlers can reference it in `responses(...)` clauses.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ErrorResponse {
    /// Human-readable error message (mirrors `Display` of the originating
    /// [`crate::AppError`]).
    pub error: String,
}

/// OpenAPI 3.1 spec for every public route. Aggregated by
/// [`utoipa::OpenApi`] from the `#[utoipa::path]` annotations on the
/// handler functions.
#[derive(OpenApi)]
#[openapi(
    info(
        title = "mewcode server",
        version = env!("CARGO_PKG_VERSION"),
        description = "HTTP API for the mewcode TUI session backend.",
    ),
    paths(
        routes::health::health,
        routes::models::list_models,
        routes::storage::status,
        routes::sessions::list,
        routes::sessions::get_one,
        routes::sessions::create,
        routes::sessions::delete,
        routes::chat::chat_stream,
    ),
    components(
        schemas(
            ErrorResponse,
            NewSession,
            Session,
            SessionSummary,
            crate::routes::health::HealthResponse,
            crate::routes::models::ModelEntry,
            crate::routes::sessions::CreateSessionRequest,
            crate::routes::storage::StorageStatus,
            mewcode_protocol::event::ChatRequest,
            mewcode_protocol::event::StreamEvent,
            mewcode_protocol::Message,
            mewcode_protocol::MessagePart,
            mewcode_protocol::Role,
            mewcode_protocol::ToolCall,
            mewcode_protocol::ToolResult,
            mewcode_protocol::Mode,
            mewcode_protocol::ModelId,
            mewcode_protocol::ModelKind,
        )
    ),
    tags(
        (name = "meta", description = "Liveness, model registry, storage status."),
        (name = "sessions", description = "Session CRUD."),
        (name = "chat", description = "SSE chat streaming."),
    ),
)]
pub struct ApiDoc;
