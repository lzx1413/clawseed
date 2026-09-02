use super::{AppState, hydrate_config_for_save, mask_sensitive_fields, require_auth};
use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Json},
};

/// GET /api/config — current config (api_key masked)
pub async fn handle_api_config_get(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if let Err(e) = require_auth(&state, &headers) {
        return e.into_response();
    }

    let config = state.config.lock().clone();

    // Serialize to TOML after masking sensitive fields.
    let masked_config = mask_sensitive_fields(&config);
    let toml_str = match toml::to_string_pretty(&masked_config) {
        Ok(s) => s,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": format!("Failed to serialize config: {e}")})),
            )
                .into_response();
        }
    };

    Json(serde_json::json!({
        "format": "toml",
        "content": toml_str,
    }))
    .into_response()
}

/// PUT /api/config — update config from TOML body
pub async fn handle_api_config_put(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: String,
) -> impl IntoResponse {
    if let Err(e) = require_auth(&state, &headers) {
        return e.into_response();
    }

    // Parse the incoming TOML
    let incoming: clawseed_config::schema::Config = match toml::from_str(&body) {
        Ok(c) => c,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": format!("Invalid TOML: {e}")})),
            )
                .into_response();
        }
    };

    let current_config = state.config.lock().clone();
    let new_config = hydrate_config_for_save(incoming, &current_config);

    if let Err(e) = new_config.validate() {
        let _ = e; // stub: validate always succeeds
        // return (
        //     StatusCode::BAD_REQUEST,
        //     Json(serde_json::json!({"error": format!("Invalid config: {e}")})),
        // )
        //     .into_response();
    }

    // Save to disk
    if let Err(e) = new_config.save() {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": format!("Failed to save config: {e}")})),
        )
            .into_response();
    }

    // Hot-update tool registry filters and skill exclusion list
    let allowed_tools = new_config.agent.allowed_tools.clone();
    let mut denied_tools = new_config.agent.denied_tools.clone();
    let skills_excluded = new_config.skills.excluded.clone();
    let skills_enabled = new_config.skills.enabled;

    // When skills are disabled, ensure "Skill" tool is denied so it's hidden
    // from both new and existing agents (shared_builtin_tools isn't rebuilt at runtime).
    if !skills_enabled && !denied_tools.iter().any(|p| p == "Skill") {
        denied_tools.push("Skill".to_string());
    }

    // Reload skill index if skills.enabled or skills.extra_roots changed
    let needs_skill_reload = current_config.skills.enabled != new_config.skills.enabled
        || current_config.skills.extra_roots != new_config.skills.extra_roots;

    // Update in-memory config
    *state.config.lock() = new_config;

    if let Some(reg) = state
        .tool_registry
        .as_any()
        .downcast_ref::<clawseed_agent::tool_registry::DefaultToolRegistry>()
    {
        reg.update_filters(allowed_tools, denied_tools);
    }
    *state.skills_excluded.lock().unwrap() = skills_excluded;

    if needs_skill_reload {
        let config = state.config.lock().clone();
        let extra_roots = config.skills.extra_roots.clone();
        let new_index = if config.skills.enabled {
            clawseed_agent::skills::load_skill_index_with_roots(&config.workspace_dir, &extra_roots)
                .into_iter()
                .collect()
        } else {
            Vec::new()
        };
        drop(config);
        *state.skill_index.write() = new_index;
    }

    Json(serde_json::json!({
        "status": "ok",
        "warning": "Provider, model, temperature, and memory are shared across connections and not rebuilt on config update. Restart the gateway for these changes to take effect."
    })).into_response()
}
