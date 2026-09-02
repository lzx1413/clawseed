use super::{AppState, require_auth};
use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Json},
};
use serde::Deserialize;

#[derive(Deserialize)]
pub struct CronRunsQuery {
    pub limit: Option<u32>,
}

#[derive(Deserialize)]
pub struct CronAddBody {
    pub name: Option<String>,
    pub schedule: String,
    pub command: Option<String>,
    pub job_type: Option<String>,
    pub prompt: Option<String>,
    pub delivery: Option<clawseed_agent::cron::DeliveryConfig>,
    pub session_target: Option<String>,
    pub model: Option<String>,
    pub allowed_tools: Option<Vec<String>>,
    pub delete_after_run: Option<bool>,
}

#[derive(Deserialize)]
pub struct CronPatchBody {
    pub name: Option<String>,
    pub schedule: Option<String>,
    pub command: Option<String>,
    pub prompt: Option<String>,
}

/// GET /api/cron — list cron jobs
pub async fn handle_api_cron_list(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if let Err(e) = require_auth(&state, &headers) {
        return e.into_response();
    }

    let config = state.config.lock().clone();
    match clawseed_agent::cron::list_jobs(&config) {
        Ok(jobs) => Json(serde_json::json!({"jobs": jobs})).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": format!("Failed to list cron jobs: {e}")})),
        )
            .into_response(),
    }
}

/// POST /api/cron — add a new cron job
pub async fn handle_api_cron_add(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<CronAddBody>,
) -> impl IntoResponse {
    if let Err(e) = require_auth(&state, &headers) {
        return e.into_response();
    }

    let CronAddBody {
        name,
        schedule,
        command,
        job_type,
        prompt,
        delivery,
        session_target,
        model,
        allowed_tools,
        delete_after_run,
    } = body;

    let config = state.config.lock().clone();
    let schedule = clawseed_agent::cron::Schedule::Cron {
        expr: schedule,
        tz: None,
    };
    if let Err(e) = clawseed_agent::cron::validate_delivery_config(delivery.as_ref()) {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": format!("Failed to add cron job: {e}")})),
        )
            .into_response();
    }

    // Determine job type: explicit field, or infer "agent" when prompt is provided.
    let is_agent =
        matches!(job_type.as_deref(), Some("agent")) || (job_type.is_none() && prompt.is_some());

    let result = if is_agent {
        let prompt = match prompt.as_deref() {
            Some(p) if !p.trim().is_empty() => p,
            _ => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({"error": "Missing 'prompt' for agent job"})),
                )
                    .into_response();
            }
        };

        let session_target = session_target
            .as_deref()
            .map(clawseed_agent::cron::SessionTarget::parse)
            .unwrap_or_default();

        let default_delete = matches!(schedule, clawseed_agent::cron::Schedule::At { .. });
        let delete_after_run = delete_after_run.unwrap_or(default_delete);

        clawseed_agent::cron::add_agent_job(
            &config,
            name,
            schedule,
            prompt,
            session_target,
            model,
            delivery,
            delete_after_run,
            allowed_tools,
        )
    } else {
        let command = match command.as_deref() {
            Some(c) if !c.trim().is_empty() => c,
            _ => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({"error": "Missing 'command' for shell job"})),
                )
                    .into_response();
            }
        };

        clawseed_agent::cron::add_shell_job_with_approval(
            &config, name, schedule, command, delivery, false,
        )
    };

    match result {
        Ok(job) => Json(serde_json::json!({"status": "ok", "job": job})).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": format!("Failed to add cron job: {e}")})),
        )
            .into_response(),
    }
}

/// GET /api/cron/:id/runs — list recent runs for a cron job
pub async fn handle_api_cron_runs(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Query(params): Query<CronRunsQuery>,
) -> impl IntoResponse {
    if let Err(e) = require_auth(&state, &headers) {
        return e.into_response();
    }

    let limit = params.limit.unwrap_or(20).clamp(1, 100) as usize;
    let config = state.config.lock().clone();

    // Verify the job exists before listing runs.
    if let Err(e) = clawseed_agent::cron::get_job(&config, &id) {
        return (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": format!("Cron job not found: {e}")})),
        )
            .into_response();
    }

    match clawseed_agent::cron::list_runs(&config, &id, limit) {
        Ok(runs) => {
            let runs_json: Vec<serde_json::Value> = runs
                .iter()
                .map(|r| {
                    serde_json::json!({
                        "id": r.id,
                        "job_id": r.job_id,
                        "started_at": r.started_at.to_rfc3339(),
                        "finished_at": r.finished_at.to_rfc3339(),
                        "status": r.status,
                        "output": r.output,
                        "duration_ms": r.duration_ms,
                    })
                })
                .collect();
            Json(serde_json::json!({"runs": runs_json})).into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": format!("Failed to list cron runs: {e}")})),
        )
            .into_response(),
    }
}

/// PATCH /api/cron/:id — update an existing cron job
pub async fn handle_api_cron_patch(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(body): Json<CronPatchBody>,
) -> impl IntoResponse {
    if let Err(e) = require_auth(&state, &headers) {
        return e.into_response();
    }

    let config = state.config.lock().clone();

    // Build the schedule from the provided expression string (if any).
    let schedule = match body.schedule {
        Some(expr) if !expr.trim().is_empty() => Some(clawseed_agent::cron::Schedule::Cron {
            expr: expr.trim().to_string(),
            tz: None,
        }),
        _ => None,
    };

    // Route the edited text to the correct field based on the job's stored type.
    // The frontend sends a single textarea value; for agent jobs it is the prompt,
    // for shell jobs it is the command.
    let existing = match clawseed_agent::cron::get_job(&config, &id) {
        Ok(j) => j,
        Err(e) => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": format!("Cron job not found: {e}")})),
            )
                .into_response();
        }
    };
    let is_agent = matches!(existing.job_type, clawseed_agent::cron::JobType::Agent);
    let (patch_command, patch_prompt) = if is_agent {
        (None, body.command.or(body.prompt))
    } else {
        (body.command.or(body.prompt), None)
    };

    let patch = clawseed_agent::cron::CronJobPatch {
        name: body.name,
        schedule,
        command: patch_command,
        prompt: patch_prompt,
        ..clawseed_agent::cron::CronJobPatch::default()
    };

    match clawseed_agent::cron::update_shell_job_with_approval(&config, &id, patch, false) {
        Ok(job) => Json(serde_json::json!({"status": "ok", "job": job})).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": format!("Failed to update cron job: {e}")})),
        )
            .into_response(),
    }
}

/// DELETE /api/cron/:id — remove a cron job
pub async fn handle_api_cron_delete(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> impl IntoResponse {
    if let Err(e) = require_auth(&state, &headers) {
        return e.into_response();
    }

    let config = state.config.lock().clone();
    match clawseed_agent::cron::remove_job(&config, &id) {
        Ok(()) => Json(serde_json::json!({"status": "ok"})).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": format!("Failed to remove cron job: {e}")})),
        )
            .into_response(),
    }
}

/// GET /api/cron/settings — return cron subsystem settings
pub async fn handle_api_cron_settings_get(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if let Err(e) = require_auth(&state, &headers) {
        return e.into_response();
    }

    let config = state.config.lock().clone();
    Json(serde_json::json!({
        "enabled": config.cron.enabled,
        "catch_up_on_startup": config.cron.catch_up_on_startup,
        "max_run_history": config.cron.max_run_history,
    }))
    .into_response()
}

/// PATCH /api/cron/settings — update cron subsystem settings
pub async fn handle_api_cron_settings_patch(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<serde_json::Value>,
) -> impl IntoResponse {
    if let Err(e) = require_auth(&state, &headers) {
        return e.into_response();
    }

    let mut config = state.config.lock().clone();

    if let Some(v) = body.get("enabled").and_then(|v| v.as_bool()) {
        config.cron.enabled = v;
    }
    if let Some(v) = body.get("catch_up_on_startup").and_then(|v| v.as_bool()) {
        config.cron.catch_up_on_startup = v;
    }
    if let Some(v) = body.get("max_run_history").and_then(|v| v.as_u64()) {
        config.cron.max_run_history = u32::try_from(v).unwrap_or(u32::MAX);
    }

    if let Err(e) = config.save() {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": format!("Failed to save config: {e}")})),
        )
            .into_response();
    }

    *state.config.lock() = config.clone();

    Json(serde_json::json!({
        "status": "ok",
        "enabled": config.cron.enabled,
        "catch_up_on_startup": config.cron.catch_up_on_startup,
        "max_run_history": config.cron.max_run_history,
    }))
    .into_response()
}
