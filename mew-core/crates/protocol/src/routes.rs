//! HTTP route paths shared by the server (which mounts them) and the
//! client (which builds URLs against them).
//!
//! Treat the values here as part of the public API: if a route moves,
//! every consumer moves with it.

/// `GET /health` — liveness probe.
pub const HEALTH: &str = "/health";

/// `GET /providers` — provider registry with their models.
pub const PROVIDERS: &str = "/providers";

/// `GET /skills` — loaded skill catalog.
pub const SKILLS: &str = "/skills";

/// `GET/POST /sessions` — list and create sessions.
pub const SESSIONS: &str = "/sessions";

/// `GET/PATCH/DELETE /sessions/{id}` — single-session detail, partial update, delete.
pub const SESSION_BY_ID: &str = "/sessions/{id}";

/// `POST /chat` — SSE chat stream.
pub const CHAT: &str = "/chat";

/// `POST /choices` — answer a pending interactive choice.
pub const CHOICES: &str = "/choices";

/// `GET /storage/status` — active storage backend and resolved data dir.
pub const STORAGE_STATUS: &str = "/storage/status";

/// `GET /memory` — read the active memory profile.
pub const MEMORY_GET: &str = "/memory";

/// `POST /memory` — write to the active memory profile.
pub const MEMORY_POST: &str = "/memory";
