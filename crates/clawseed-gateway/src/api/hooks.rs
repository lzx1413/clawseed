use super::AppState;
use axum::{
    extract::State,
    response::{IntoResponse, Json},
};

/// POST /hooks/claude-code — receives HTTP hook events from Claude Code
/// sessions spawned by `ClaudeCodeRunnerTool`.
///
/// Claude Code posts structured JSON describing tool executions, completions,
/// and errors. This handler logs the event and (when a Slack channel is
/// configured) could be wired to update a Slack message in-place.
pub async fn handle_claude_code_hook(
    State(state): State<AppState>,
    Json(payload): Json<clawseed_agent::tools::claude_code_runner::ClaudeCodeHookEvent>,
) -> impl IntoResponse {
    // Do not require bearer-token auth: Claude Code subprocesses cannot easily
    // obtain a pairing token, and the hook carries a session_id that ties it
    // back to a session we spawned.
    let _ = &state; // retained for future Slack update wiring

    tracing::info!(
        session_id = %payload.session_id,
        event_type = %payload.event_type,
        tool_name = ?payload.tool_name,
        summary = ?payload.summary,
        "Claude Code hook event received"
    );

    Json(serde_json::json!({ "ok": true }))
}
