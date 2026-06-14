//! HTTP route paths shared by the server (which mounts them) and the
//! client (which builds URLs against them).
//!
//! Treat the values here as part of the public API: if a route moves,
//! every consumer moves with it.

/// `GET /health` — liveness probe.
pub const HEALTH: &str = "/health";

/// `GET /models` — model registry.
pub const MODELS: &str = "/models";

/// `GET/POST /sessions` — list and create sessions.
pub const SESSIONS: &str = "/sessions";

/// `GET /sessions/{id}` — single-session detail.
pub const SESSION_BY_ID: &str = "/sessions/{id}";

/// `POST /chat` — SSE chat stream.
pub const CHAT: &str = "/chat";

/// `GET /storage/status` — active storage backend and resolved data dir.
pub const STORAGE_STATUS: &str = "/storage/status";
