use super::{AppState, require_auth};
use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Json},
};
use clawseed_config::schema::PluginsConfig;
use clawseed_tools::plugins::{PluginManifest, valid_plugin_id};
use serde::Deserialize;
use std::collections::HashSet;
use std::path::PathBuf;

#[derive(Debug, Deserialize)]
pub struct PluginConfigPatch {
    pub enabled: Option<bool>,
    pub extra_roots: Option<Vec<String>>,
    pub max_loaded: Option<usize>,
    pub allowed: Option<Vec<String>>,
    pub denied: Option<Vec<String>>,
}

fn plugin_roots(config: &clawseed_config::schema::Config) -> Vec<PathBuf> {
    let mut roots = vec![config.workspace_dir.join(".clawseed/plugins")];
    roots.extend(config.plugins.extra_roots.iter().map(PathBuf::from));
    roots
}

fn plugin_allowed(config: &PluginsConfig, plugin_id: &str) -> bool {
    config.enabled
        && (config.allowed.is_empty() || config.allowed.iter().any(|id| id == plugin_id))
        && !config.denied.iter().any(|id| id == plugin_id)
}

fn validate_id_list(
    field: &str,
    ids: &[String],
) -> Result<(), (StatusCode, Json<serde_json::Value>)> {
    if let Some(id) = ids.iter().find(|id| !valid_plugin_id(id)) {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": format!("{field} contains invalid plugin id: {id}")
            })),
        ));
    }
    Ok(())
}

/// GET /api/plugins — list discovered plugin manifests and their load policy.
pub async fn handle_api_plugins(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if let Err(e) = require_auth(&state, &headers) {
        return e.into_response();
    }

    let config = state.config.lock().clone();
    let mut seen = HashSet::new();
    let mut plugins = Vec::new();
    for root in plugin_roots(&config) {
        let Ok(entries) = std::fs::read_dir(&root) else {
            continue;
        };
        let mut dirs: Vec<PathBuf> = entries
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| path.is_dir())
            .collect();
        dirs.sort();
        for plugin_dir in dirs {
            let manifest_path = plugin_dir.join("manifest.json");
            let bytes = match std::fs::read(&manifest_path) {
                Ok(bytes) => bytes,
                Err(error) => {
                    plugins.push(serde_json::json!({
                        "plugin_id": plugin_dir.file_name().and_then(|v| v.to_str()).unwrap_or(""),
                        "path": plugin_dir.display().to_string(),
                        "status": "invalid",
                        "manifest_valid": false,
                        "validation_error": format!("could not read manifest: {error}"),
                    }));
                    continue;
                }
            };
            let manifest: PluginManifest = match serde_json::from_slice(&bytes) {
                Ok(manifest) => manifest,
                Err(error) => {
                    plugins.push(serde_json::json!({
                        "plugin_id": plugin_dir.file_name().and_then(|v| v.to_str()).unwrap_or(""),
                        "path": plugin_dir.display().to_string(),
                        "status": "invalid",
                        "manifest_valid": false,
                        "validation_error": format!("invalid manifest: {error}"),
                    }));
                    continue;
                }
            };
            if !seen.insert(manifest.plugin_id.clone()) {
                continue;
            }
            let validation_error = manifest
                .validate(&plugin_dir)
                .err()
                .map(|error| error.to_string());
            let manifest_valid = validation_error.is_none();
            let enabled = manifest_valid && plugin_allowed(&config.plugins, &manifest.plugin_id);
            plugins.push(serde_json::json!({
                "plugin_id": manifest.plugin_id,
                "name": manifest.name,
                "version": manifest.version,
                "description": manifest.description,
                "language": manifest.runtime.language,
                "entry": manifest.entry,
                "path": plugin_dir.display().to_string(),
                "enabled": enabled,
                "status": if !manifest_valid { "invalid" } else if enabled { "enabled" } else { "disabled" },
                "manifest_valid": manifest_valid,
                "validation_error": validation_error,
            }));
        }
    }

    Json(serde_json::json!({
        "enabled": config.plugins.enabled,
        "max_loaded": config.plugins.max_loaded,
        "allowed": config.plugins.allowed,
        "denied": config.plugins.denied,
        "plugins": plugins,
    }))
    .into_response()
}

/// PATCH /api/plugins/config — update only the plugin configuration section.
pub async fn handle_api_plugins_config(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(patch): Json<PluginConfigPatch>,
) -> impl IntoResponse {
    if let Err(e) = require_auth(&state, &headers) {
        return e.into_response();
    }
    if let Some(ids) = patch.allowed.as_deref() {
        if let Err(error) = validate_id_list("allowed", ids) {
            return error.into_response();
        }
    }
    if let Some(ids) = patch.denied.as_deref() {
        if let Err(error) = validate_id_list("denied", ids) {
            return error.into_response();
        }
    }

    let mut config = state.config.lock().clone();
    if let Some(enabled) = patch.enabled {
        config.plugins.enabled = enabled;
    }
    if let Some(extra_roots) = patch.extra_roots {
        config.plugins.extra_roots = extra_roots;
    }
    if let Some(max_loaded) = patch.max_loaded {
        config.plugins.max_loaded = max_loaded;
    }
    if let Some(allowed) = patch.allowed {
        config.plugins.allowed = allowed;
    }
    if let Some(denied) = patch.denied {
        config.plugins.denied = denied;
    }

    if let Err(error) = config.save() {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": format!("Failed to save plugin config: {error}")})),
        )
            .into_response();
    }
    let response = serde_json::json!({
        "status": "ok",
        "enabled": config.plugins.enabled,
        "extra_roots": config.plugins.extra_roots,
        "max_loaded": config.plugins.max_loaded,
        "allowed": config.plugins.allowed,
        "denied": config.plugins.denied,
        "reload_required": true,
    });
    *state.config.lock() = config;
    Json(response).into_response()
}
