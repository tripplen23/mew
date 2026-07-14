//! Storage status. Reports the active [`crate::store::SessionStore`] backend
//! and, for the filesystem backend, the resolved data directory — without
//! reading any session or message data.

use axum::Json;
use axum::extract::State;
use serde::Serialize;

use crate::AppState;

/// Response body for `GET /storage/status`.
///
/// `backend` is always exactly the active store's wire label (`"memory"` or
/// `"filesystem"`); `data_dir` is the resolved path string for the filesystem
/// backend and `null` for the in-memory backend.
#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct StorageStatus {
    /// Active backend label, one of `"memory"` or `"filesystem"`.
    pub backend: String,
    /// Resolved data directory path, or `null` for non-persistent backends.
    pub data_dir: Option<String>,
}

/// `GET /storage/status` — report the active backend and data dir. Reads only
/// store metadata, never session or message content.
#[utoipa::path(
    get,
    path = "/storage/status",
    tag = "meta",
    responses(
        (status = 200, description = "Active storage backend and resolved data dir", body = StorageStatus),
    ),
)]
pub async fn status(State(state): State<AppState>) -> Json<StorageStatus> {
    Json(StorageStatus {
        backend: state.store.backend().as_str().to_owned(),
        data_dir: state
            .store
            .data_dir_path()
            .map(|p| p.to_string_lossy().into_owned()),
    })
}
