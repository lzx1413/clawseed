use super::{AppState, require_auth};
use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Json},
};
use serde::Deserialize;

/// GET /api/personality — read personality files from workspace
pub async fn handle_api_personality_get(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if let Err(e) = require_auth(&state, &headers) {
        return e.into_response();
    }

    let config = state.config.lock().clone();
    let workspace_dir = clawseed_config::resolve_workspace_dir(&config);
    let allowed = clawseed_agent::personality::allowed_personality_files();

    let mut files = serde_json::Map::new();
    for &name in allowed {
        let path = workspace_dir.join(name);
        if let Ok(content) = std::fs::read_to_string(&path) {
            files.insert(name.to_string(), serde_json::Value::String(content));
        }
    }

    Json(serde_json::json!({ "files": files })).into_response()
}

/// PUT /api/personality — write personality files to workspace
#[derive(Deserialize)]
pub struct PersonalityPutBody {
    pub files: std::collections::HashMap<String, String>,
}

pub async fn handle_api_personality_put(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<PersonalityPutBody>,
) -> impl IntoResponse {
    if let Err(e) = require_auth(&state, &headers) {
        return e.into_response();
    }

    let config = state.config.lock().clone();
    let workspace_dir = clawseed_config::resolve_workspace_dir(&config);
    let allowed = clawseed_agent::personality::allowed_personality_files();

    if let Err(e) = std::fs::create_dir_all(&workspace_dir) {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": format!("Failed to create workspace dir: {e}")})),
        )
            .into_response();
    }

    for (name, content) in &body.files {
        if !allowed.contains(&name.as_str()) {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": format!("Unknown personality file: {name}")})),
            )
                .into_response();
        }
        let path = workspace_dir.join(name);
        if let Err(e) = std::fs::write(&path, content) {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": format!("Failed to write {name}: {e}")})),
            )
                .into_response();
        }
    }

    Json(serde_json::json!({"status": "ok"})).into_response()
}

// ── /api/personas — named persona (分身) management ─────────────────────────
//
// Personas live in `config.agents` as `AgentEntryConfig` entries. An entry is
// a "persona" (vs a plain named API key) when `has_persona_overrides()` is
// true — i.e. it sets identity / system_prompt / memory_namespace /
// allowed_tools / denied_tools. These endpoints manage only the persona fields
// and preserve any existing `api_key`. Persistence goes through the standard
// `Config::save()` path (no second source of truth).

/// GET /api/personas — list personas with their configured overrides.
///
/// Never exposes `api_key`. Returns each entry's name, which override fields
/// are set, and the memory namespace (if any).
pub async fn handle_api_personas_list(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if let Err(e) = require_auth(&state, &headers) {
        return e.into_response();
    }

    let config = state.config.lock().clone();
    let personas: Vec<serde_json::Value> = config
        .agents
        .iter()
        .map(|(name, entry)| {
            serde_json::json!({
                "name": name,
                "is_persona": entry.has_persona_overrides(),
                "has_identity": entry.identity.is_some(),
                "has_system_prompt": entry.system_prompt.is_some(),
                "memory_namespace": entry.memory_namespace,
                "provider": entry.provider,
                "allowed_tools": entry.allowed_tools,
                "denied_tools": entry.denied_tools,
                "denied_skills": entry.denied_skills,
                "model": entry.model,
                "thinking_enabled": entry.thinking_enabled,
                "vision": entry.vision,
                "avatar": entry.avatar,
                "color": entry.color,
            })
        })
        .collect();

    Json(serde_json::json!({"personas": personas})).into_response()
}

/// GET /api/personas/:name — read full editable persona detail.
pub async fn handle_api_persona_get(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(name): Path<String>,
) -> impl IntoResponse {
    if let Err(e) = require_auth(&state, &headers) {
        return e.into_response();
    }

    let config = state.config.lock().clone();
    let Some(entry) = config.agents.get(&name) else {
        return (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": format!("Persona '{name}' not found")})),
        )
            .into_response();
    };

    Json(serde_json::json!({
        "name": name,
        "is_persona": entry.has_persona_overrides(),
        "identity": entry.identity,
        "has_identity": entry.identity.is_some(),
        "system_prompt": entry.system_prompt,
        "has_system_prompt": entry.system_prompt.is_some(),
        "memory_namespace": entry.memory_namespace,
        "provider": entry.provider,
        "allowed_tools": entry.allowed_tools,
        "denied_tools": entry.denied_tools,
        "denied_skills": entry.denied_skills,
        "model": entry.model,
        "thinking_enabled": entry.thinking_enabled,
                "vision": entry.vision,
        "avatar": entry.avatar,
        "color": entry.color,
    }))
    .into_response()
}

/// PUT /api/personas/:name — upsert a persona's override fields.
///
/// Body fields are optional; only provided fields are overwritten. The entry's
/// `api_key` is preserved if it already exists. Setting all override fields to
/// null/empty effectively demotes the entry back to a plain named API key.
#[derive(Deserialize)]
pub struct PersonaPutBody {
    #[serde(default)]
    pub provider: Option<String>,
    #[serde(default)]
    pub identity: Option<clawseed_config::schema::IdentityConfig>,
    #[serde(default)]
    pub system_prompt: Option<String>,
    #[serde(default)]
    pub memory_namespace: Option<String>,
    #[serde(default)]
    pub allowed_tools: Vec<String>,
    #[serde(default)]
    pub denied_tools: Vec<String>,
    #[serde(default)]
    pub denied_skills: Vec<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub thinking_enabled: Option<bool>,
    #[serde(default)]
    pub vision: Option<clawseed_config::schema::VisionMode>,
    #[serde(default)]
    pub avatar: Option<String>,
    #[serde(default)]
    pub color: Option<String>,
}

pub async fn handle_api_persona_put(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(name): Path<String>,
    Json(body): Json<PersonaPutBody>,
) -> impl IntoResponse {
    if let Err(e) = require_auth(&state, &headers) {
        return e.into_response();
    }

    let mut config = state.config.lock().clone();

    // Upsert, preserving any existing api_key.
    let existing = config.agents.get(&name).cloned();
    let entry = clawseed_config::schema::AgentEntryConfig {
        provider: body.provider,
        api_key: existing.and_then(|e| e.api_key),
        identity: body.identity,
        system_prompt: body.system_prompt,
        memory_namespace: body.memory_namespace,
        allowed_tools: body.allowed_tools,
        denied_tools: body.denied_tools,
        denied_skills: body.denied_skills,
        model: body.model,
        thinking_enabled: body.thinking_enabled,
        vision: body.vision,
        avatar: body.avatar,
        color: body.color,
    };
    config.agents.insert(name.clone(), entry);

    if let Err(e) = config.save() {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": format!("Failed to save config: {e}")})),
        )
            .into_response();
    }
    *state.config.lock() = config;

    Json(serde_json::json!({"status": "ok", "name": name})).into_response()
}

/// DELETE /api/personas/:name — remove a persona entry entirely.
pub async fn handle_api_persona_delete(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(name): Path<String>,
) -> impl IntoResponse {
    if let Err(e) = require_auth(&state, &headers) {
        return e.into_response();
    }

    let mut config = state.config.lock().clone();
    if config.agents.remove(&name).is_none() {
        return (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": format!("Persona '{name}' not found")})),
        )
            .into_response();
    }
    if let Err(e) = config.save() {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": format!("Failed to save config: {e}")})),
        )
            .into_response();
    }
    *state.config.lock() = config;

    Json(serde_json::json!({"status": "ok", "deleted": name})).into_response()
}
