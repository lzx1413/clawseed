//! Live Canvas (A2UI) API handlers.
//!
//! Stubs — full implementation deferred.

use axum::{
    extract::{Path, State},
    response::{IntoResponse, Json},
};
use super::AppState;

/// GET /api/canvas — list canvas IDs
pub async fn handle_canvas_list(State(_state): State<AppState>) -> impl IntoResponse {
    Json(serde_json::json!({"canvases": []}))
}

/// GET /api/canvas/{id} — get current canvas content
pub async fn handle_canvas_get(
    Path(_id): Path<String>,
    State(_state): State<AppState>,
) -> impl IntoResponse {
    Json(serde_json::json!({"canvas_id": "default", "content": null}))
}

/// POST /api/canvas/{id} — render content to canvas
pub async fn handle_canvas_post(
    Path(_id): Path<String>,
    State(_state): State<AppState>,
) -> impl IntoResponse {
    Json(serde_json::json!({"status": "ok"}))
}

/// DELETE /api/canvas/{id} — clear canvas
pub async fn handle_canvas_clear(
    Path(_id): Path<String>,
    State(_state): State<AppState>,
) -> impl IntoResponse {
    Json(serde_json::json!({"status": "ok"}))
}

/// GET /api/canvas/{id}/history — get canvas frame history
pub async fn handle_canvas_history(
    Path(_id): Path<String>,
    State(_state): State<AppState>,
) -> impl IntoResponse {
    Json(serde_json::json!({"history": []}))
}

/// GET /ws/canvas/{id} — WebSocket canvas updates
pub async fn handle_ws_canvas(
    Path(_id): Path<String>,
    State(_state): State<AppState>,
) -> impl IntoResponse {
    (axum::http::StatusCode::NOT_IMPLEMENTED, "Canvas WebSocket not yet implemented")
}
