use super::{AppState, require_auth};
use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Json},
};
use serde::Deserialize;

/// GET /api/skills — list all skill index entries (including excluded)
pub async fn handle_api_skills(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if let Err(e) = require_auth(&state, &headers) {
        return e.into_response();
    }

    let excluded = state.skills_excluded.lock().unwrap().clone();
    let index = state.skill_index.read();
    let skills: Vec<serde_json::Value> = index
        .iter()
        .map(|entry| {
            let enabled = !excluded.contains(&entry.name);
            serde_json::json!({
                "name": entry.name,
                "description": entry.description,
                "version": entry.version,
                "triggers": entry.trigger_phrases,
                "permissions": entry.permissions,
                "enabled": enabled,
            })
        })
        .collect();

    Json(serde_json::json!({"skills": skills})).into_response()
}

/// GET /api/skills/{name} — read full skill content (SKILL.md + manifest metadata)
pub async fn handle_api_skill_get(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(name): Path<String>,
) -> impl IntoResponse {
    if let Err(e) = require_auth(&state, &headers) {
        return e.into_response();
    }

    let config = state.config.lock().clone();
    let workspace_dir = clawseed_config::resolve_workspace_dir(&config);
    let extra_roots: Vec<String> = config.skills.extra_roots.clone();
    drop(config);

    match clawseed_agent::skills::load_skill_by_name_with_roots(&name, &workspace_dir, &extra_roots)
    {
        Ok(skill) => Json(serde_json::json!({
            "name": skill.name,
            "description": skill.description,
            "version": skill.version,
            "author": skill.author,
            "tags": skill.tags,
            "permissions": skill.permissions,
            "content": skill.content,
        }))
        .into_response(),
        Err(e) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": format!("Skill '{}' not found: {e}", name)})),
        )
            .into_response(),
    }
}

#[derive(Deserialize)]
pub struct SkillPutBody {
    pub content: String,
}

/// PUT /api/skills/{name} — write SKILL.md content for a skill
pub async fn handle_api_skill_put(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(name): Path<String>,
    Json(body): Json<SkillPutBody>,
) -> impl IntoResponse {
    if let Err(e) = require_auth(&state, &headers) {
        return e.into_response();
    }

    let config = state.config.lock().clone();
    let workspace_dir = clawseed_config::resolve_workspace_dir(&config);
    let extra_roots: Vec<String> = config.skills.extra_roots.clone();
    drop(config);

    // Find the skill to get its directory location
    let skill = match clawseed_agent::skills::load_skill_by_name_with_roots(
        &name,
        &workspace_dir,
        &extra_roots,
    ) {
        Ok(s) => s,
        Err(e) => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": format!("Skill '{}' not found: {e}", name)})),
            )
                .into_response();
        }
    };

    let skill_md_path = skill.location.join("SKILL.md");
    if let Err(e) = std::fs::write(&skill_md_path, &body.content) {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": format!("Failed to write SKILL.md: {e}")})),
        )
            .into_response();
    }

    // Reload skill index so changes are reflected
    let config = state.config.lock().clone();
    let new_index = if config.skills.enabled {
        clawseed_agent::skills::load_skill_index_with_roots(&workspace_dir, &extra_roots)
            .into_iter()
            .collect()
    } else {
        Vec::new()
    };
    drop(config);
    *state.skill_index.write() = new_index;

    Json(serde_json::json!({"status": "ok"})).into_response()
}

/// POST /api/skills/reload — hot-reload skill index from disk
pub async fn handle_api_skills_reload(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if let Err(e) = require_auth(&state, &headers) {
        return e.into_response();
    }

    let config = state.config.lock().clone();
    let extra_roots: Vec<String> = config.skills.extra_roots.clone();
    let new_excluded: Vec<String> = config.skills.excluded.clone();

    let new_index = if config.skills.enabled {
        clawseed_agent::skills::load_skill_index_with_roots(&config.workspace_dir, &extra_roots)
            .into_iter()
            .collect()
    } else {
        Vec::new()
    };
    drop(config);

    let count = new_index.len();
    {
        let mut index = state.skill_index.write();
        *index = new_index;
    }
    {
        let mut excluded = state.skills_excluded.lock().unwrap();
        *excluded = new_excluded;
    }

    Json(serde_json::json!({
        "ok": true,
        "skills_count": count,
    }))
    .into_response()
}
