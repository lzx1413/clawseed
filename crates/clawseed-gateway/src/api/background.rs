use super::{AppState, require_auth};
use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Json},
};

/// Read-only summary of the same manager used by agent tool dispatch. Output
/// bodies are deliberately omitted so a one-second keepalive poll stays small.
pub async fn handle_api_background_jobs(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if let Err(error) = require_auth(&state, &headers) {
        return error.into_response();
    }
    let Some(tool) = state
        .shared_builtin_tools
        .iter()
        .find(|tool| tool.name() == "background_status")
    else {
        return Json(serde_json::json!({"active_count":0,"jobs":[]})).into_response();
    };
    struct Context(std::path::PathBuf);
    impl clawseed_api::tool_context::ToolContext for Context {
        fn workspace_dir(&self) -> &std::path::Path {
            &self.0
        }
    }
    let context = Context(state.config.lock().workspace_dir.clone());
    match tool.execute(serde_json::json!({}), &context).await {
        Ok(result) if result.success => {
            match serde_json::from_str::<serde_json::Value>(&result.output) {
                Ok(summary) => Json(summary).into_response(),
                Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
            }
        }
        _ => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}
