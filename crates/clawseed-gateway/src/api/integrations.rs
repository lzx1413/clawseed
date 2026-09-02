use super::{AppState, require_auth};
use axum::{
    extract::State,
    http::HeaderMap,
    response::{IntoResponse, Json},
};

/// GET /api/integrations — list all integrations with status
pub async fn handle_api_integrations(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if let Err(e) = require_auth(&state, &headers) {
        return e.into_response();
    }

    let _ = &state;
    let integrations: Vec<serde_json::Value> = Vec::new();

    Json(serde_json::json!({"integrations": integrations})).into_response()
}

/// GET /api/integrations/settings — return per-integration settings (enabled + category)
pub async fn handle_api_integrations_settings(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if let Err(e) = require_auth(&state, &headers) {
        return e.into_response();
    }

    let _ = state;
    let settings = serde_json::Map::new();

    Json(serde_json::json!({"settings": settings})).into_response()
}

/// POST /api/doctor — run diagnostics
pub async fn handle_api_doctor(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if let Err(e) = require_auth(&state, &headers) {
        return e.into_response();
    }

    let mut results = Vec::new();

    // Provider health
    let config = state.config.lock().clone();
    let provider_status = if config.providers.fallback.is_some() {
        "ok"
    } else {
        "not_configured"
    };
    results.push(serde_json::json!({
        "check": "provider",
        "status": provider_status,
        "provider": config.providers.fallback,
        "model": state.model,
    }));

    // Memory health
    let memory_ok = state.mem.health_check().await;
    results.push(serde_json::json!({
        "check": "memory",
        "status": if memory_ok { "ok" } else { "error" },
        "backend": state.mem.name(),
    }));

    // Tools health
    let tool_count = state.tool_registry.len();
    results.push(serde_json::json!({
        "check": "tools",
        "status": "ok",
        "count": tool_count,
    }));

    let errors = results.iter().filter(|r| r["status"] == "error").count();
    let warnings = results.iter().filter(|r| r["status"] == "warning").count();
    let ok = results.len() - errors - warnings;

    Json(serde_json::json!({
        "results": results,
        "summary": {
            "ok": ok,
            "warnings": warnings,
            "errors": errors,
        }
    }))
    .into_response()
}
