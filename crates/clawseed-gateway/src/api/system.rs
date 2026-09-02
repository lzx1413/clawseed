use super::{AppState, require_auth};
use axum::{
    extract::State,
    http::HeaderMap,
    response::{IntoResponse, Json},
};

/// GET /api/cost — cost summary
pub async fn handle_api_cost(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if let Err(e) = require_auth(&state, &headers) {
        return e.into_response();
    }

    if let Some(ref tracker) = state.cost_tracker {
        let summary = tracker.get_summary();
        Json(serde_json::json!({"cost": summary})).into_response()
    } else {
        Json(serde_json::json!({
            "cost": {
                "session_cost_usd": 0.0,
                "daily_cost_usd": 0.0,
                "monthly_cost_usd": 0.0,
                "total_tokens": 0,
                "request_count": 0,
                "by_model": {},
            }
        }))
        .into_response()
    }
}

/// GET /api/cli-tools — discovered CLI tools
pub async fn handle_api_cli_tools(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if let Err(e) = require_auth(&state, &headers) {
        return e.into_response();
    }

    let tools = clawseed_agent::tools::cli_discovery::discover_cli_tools(&[], &[]);

    Json(serde_json::json!({"cli_tools": tools})).into_response()
}

/// GET /api/channels — list configured channels with status
pub async fn handle_api_channels(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if let Err(e) = require_auth(&state, &headers) {
        return e.into_response();
    }

    let config = state.config.lock().clone();
    let channels: Vec<serde_json::Value> = config
        .channels
        .channels()
        .into_iter()
        .filter(|(_, present)| *present)
        .map(|(ch, _)| {
            serde_json::json!({
                "name": ch.name(),
                "type": ch.name(),
                "enabled": true,
                "status": "active",
                "message_count": 0,
                "last_message_at": null,
                "health": "healthy",
            })
        })
        .collect();

    Json(serde_json::json!({ "channels": channels })).into_response()
}

/// GET /api/health — component health snapshot
pub async fn handle_api_health(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if let Err(e) = require_auth(&state, &headers) {
        return e.into_response();
    }

    let health = clawseed_agent::health::snapshot_json();
    Json(serde_json::json!({"health": health})).into_response()
}
