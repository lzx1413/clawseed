use super::{AppState, require_auth};
use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Json},
};

/// GET /api/sessions — list gateway sessions
pub async fn handle_api_sessions_list(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if let Err(e) = require_auth(&state, &headers) {
        return e.into_response();
    }

    let Some(ref backend) = state.session_backend else {
        return Json(serde_json::json!({
            "sessions": [],
            "message": "Session persistence is disabled"
        }))
        .into_response();
    };

    let all_metadata = backend.list_sessions_with_metadata();
    let gw_sessions: Vec<serde_json::Value> = all_metadata
        .into_iter()
        .filter_map(|meta| {
            let session_id = meta.key.strip_prefix("gw_")?;
            let mut entry = serde_json::json!({
                "session_id": session_id,
                "created_at": meta.created_at.to_rfc3339(),
                "last_activity": meta.last_activity.to_rfc3339(),
                "message_count": meta.message_count,
            });
            if let Some(name) = meta.name {
                entry["name"] = serde_json::Value::String(name);
            }
            if let Some(persona) = meta.persona {
                entry["persona"] = serde_json::Value::String(persona);
            }
            Some(entry)
        })
        .collect();

    Json(serde_json::json!({ "sessions": gw_sessions })).into_response()
}

/// GET /api/sessions/{id}/messages — load persisted gateway WebSocket chat transcript
pub async fn handle_api_session_messages(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> impl IntoResponse {
    if let Err(e) = require_auth(&state, &headers) {
        return e.into_response();
    }

    let Some(ref backend) = state.session_backend else {
        return Json(serde_json::json!({
            "session_id": id,
            "messages": [],
            "session_persistence": false,
        }))
        .into_response();
    };

    let session_key = format!("gw_{id}");
    if backend
        .get_session_user(&session_key)
        .ok()
        .flatten()
        .is_some_and(|owner| owner != crate::LOCAL_OWNER_USER_ID)
    {
        return (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({"error":"Session belongs to another user"})),
        )
            .into_response();
    }
    let msgs = backend.load_with_presentations(&session_key);
    let messages: Vec<serde_json::Value> = msgs
        .into_iter()
        .map(|m| {
            serde_json::json!({
                "role": m.role,
                "content": m.content,
                "presentation": m.presentation,
                "metrics": m.metrics,
                "attachments": m.attachments,
            })
        })
        .collect();

    Json(serde_json::json!({
        "session_id": id,
        "messages": messages,
        "session_persistence": true,
    }))
    .into_response()
}

/// DELETE /api/sessions/{id} — delete a gateway session
pub async fn handle_api_session_delete(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> impl IntoResponse {
    if let Err(e) = require_auth(&state, &headers) {
        return e.into_response();
    }

    let Some(ref backend) = state.session_backend else {
        return (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "Session persistence is disabled"})),
        )
            .into_response();
    };

    let session_key = format!("gw_{id}");
    match backend.delete_session(&session_key) {
        Ok(true) => {
            if let Err(error) = backend.cleanup_images() {
                tracing::warn!(%error, "Image cleanup failed");
            }
            Json(serde_json::json!({"deleted": true, "session_id": id})).into_response()
        }
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "Session not found"})),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": format!("Failed to delete session: {e}")})),
        )
            .into_response(),
    }
}

/// PUT /api/sessions/{id} — rename a gateway session
pub async fn handle_api_session_rename(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(body): Json<serde_json::Value>,
) -> impl IntoResponse {
    if let Err(e) = require_auth(&state, &headers) {
        return e.into_response();
    }

    let Some(ref backend) = state.session_backend else {
        return (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "Session persistence is disabled"})),
        )
            .into_response();
    };

    let name = body["name"].as_str().unwrap_or("").trim();
    if name.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "name is required"})),
        )
            .into_response();
    }

    let session_key = format!("gw_{id}");

    // Verify the session exists before renaming
    let sessions = backend.list_sessions();
    if !sessions.contains(&session_key) {
        return (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "Session not found"})),
        )
            .into_response();
    }

    match backend.set_session_name(&session_key, name) {
        Ok(()) => Json(serde_json::json!({"session_id": id, "name": name})).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": format!("Failed to rename session: {e}")})),
        )
            .into_response(),
    }
}

/// GET /api/sessions/running — list sessions currently in "running" state
pub async fn handle_api_sessions_running(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if let Err(e) = require_auth(&state, &headers) {
        return e.into_response();
    }

    let Some(ref backend) = state.session_backend else {
        return Json(serde_json::json!({
            "sessions": [],
            "message": "Session persistence is disabled"
        }))
        .into_response();
    };

    let running = backend.list_running_sessions();
    let sessions: Vec<serde_json::Value> = running
        .into_iter()
        .filter_map(|meta| {
            let session_id = meta.key.strip_prefix("gw_")?;
            Some(serde_json::json!({
                "session_id": session_id,
                "created_at": meta.created_at.to_rfc3339(),
                "last_activity": meta.last_activity.to_rfc3339(),
                "message_count": meta.message_count,
            }))
        })
        .collect();

    Json(serde_json::json!({ "sessions": sessions })).into_response()
}

/// GET /api/sessions/{id}/state — get session state
pub async fn handle_api_session_state(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> impl IntoResponse {
    if let Err(e) = require_auth(&state, &headers) {
        return e.into_response();
    }

    let Some(ref backend) = state.session_backend else {
        return (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "Session persistence is disabled"})),
        )
            .into_response();
    };

    let session_key = format!("gw_{id}");
    match backend.get_session_state(&session_key) {
        Ok(Some(ss)) => {
            let mut resp = serde_json::json!({
                "session_id": id,
                "state": ss.state,
            });
            if let Some(turn_id) = ss.turn_id {
                resp["turn_id"] = serde_json::Value::String(turn_id);
            }
            if let Some(started) = ss.turn_started_at {
                resp["turn_started_at"] = serde_json::Value::String(started.to_rfc3339());
            }
            Json(resp).into_response()
        }
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "Session not found"})),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": format!("Failed to get session state: {e}")})),
        )
            .into_response(),
    }
}

// ── Session abort endpoint ────────────────────────────────────────

/// POST /api/sessions/{id}/abort — cancel an in-flight agent response.
///
/// Looks up the cancellation token for the given session. If a turn is
/// currently running the token is cancelled, which causes the agent's
/// streaming loop and tool-call loop to exit early. The WebSocket handler
/// is responsible for cleaning up partial state and sending the abort
/// frame to the client.
///
/// Returns 200 with `{"status": "aborted"}` if a running turn was found,
/// or `{"status": "no_active_response"}` if the session was idle (no
/// token present). Both are success — abort is idempotent.
pub async fn handle_api_session_abort(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> impl IntoResponse {
    if let Err(e) = require_auth(&state, &headers) {
        return e.into_response();
    }

    let session_key = format!("gw_{id}");

    // Look up and cancel the token. Hold the lock only long enough to
    // clone the token — cancellation itself does not need the lock.
    let token = state
        .cancel_tokens
        .lock()
        .expect("cancel_tokens lock poisoned")
        .get(&session_key)
        .cloned();

    if let Some(token) = token {
        token.cancel();
        tracing::info!(session_key, "session abort requested");
        Json(serde_json::json!({ "status": "aborted" })).into_response()
    } else {
        Json(serde_json::json!({ "status": "no_active_response" })).into_response()
    }
}
