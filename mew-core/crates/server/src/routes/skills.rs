//! `GET /skills` — returns the skill catalog loaded from the server's
//! configured locations (defaults + `external_dirs`), mirroring how
//! `POST /chat` builds its registry.

use axum::Json;
use axum::extract::State;
use mewcode_engine::skills::{SkillLoadConfig, SkillRegistry};
use serde::Serialize;

use crate::AppState;

/// One entry in the skill catalog returned by `GET /skills`.
#[derive(Serialize, utoipa::ToSchema)]
pub struct SkillEntry {
    /// Skill name.
    pub name: String,
    /// When to use the skill.
    pub description: String,
    /// Where it was loaded from (`bundled`, `project`, `external`, …).
    pub source: String,
    /// Sub-files inside the skill bundle, relative to its root.
    pub assets: Vec<String>,
}

/// `GET /skills` — list every loaded skill. Loads the registry on demand
/// with the same config as the chat route, so the catalog reflects the
/// server's configured skill directories.
#[utoipa::path(
    get,
    path = "/skills",
    tag = "meta",
    responses(
        (status = 200, description = "Skill catalog", body = [SkillEntry]),
    ),
)]
pub async fn list_skills(State(state): State<AppState>) -> Json<Vec<SkillEntry>> {
    let cfg = SkillLoadConfig {
        bundled_dir: None,
        external_dirs: state.config.skills.resolved_dirs(),
        project_search_start: std::env::current_dir().ok(),
        include_dev_dir: true,
    };
    let entries = SkillRegistry::load(&cfg)
        .list_for_tool()
        .into_iter()
        .map(|e| SkillEntry {
            name: e.name,
            description: e.description,
            source: e.source.to_string(),
            assets: e.assets,
        })
        .collect();
    Json(entries)
}
