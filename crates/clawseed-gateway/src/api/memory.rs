use super::{AppState, require_auth};
use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Json},
};
use clawseed_api::memory_traits::{
    MemoryCategory, MemoryEntry, MemoryQuery as ScopedMemoryQuery, MemoryScope,
};
use clawseed_api::user_profile::{
    ProfileCategory, ProfileChangeAction, ProfileImportStrategy, ProfileItemInput,
    ProfileSearchQuery, ProfileSource, ProfileStatus,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Deserialize)]
pub struct MemoryQuery {
    pub query: Option<String>,
    pub category: Option<String>,
    /// Filter memories created at or after (RFC 3339 / ISO 8601)
    pub since: Option<String>,
    /// Filter memories created at or before (RFC 3339 / ISO 8601)
    pub until: Option<String>,
    /// An authorized memory namespace. Omit to read default + public.
    pub namespace: Option<String>,
    /// Persona name whose configured namespace should be used.
    pub persona: Option<String>,
}

#[derive(Deserialize)]
pub struct MemoryStoreBody {
    pub key: String,
    pub content: String,
    pub category: Option<String>,
    pub namespace: Option<String>,
    pub persona: Option<String>,
}

#[derive(Default, Deserialize)]
pub struct MemoryScopeQuery {
    pub namespace: Option<String>,
    pub persona: Option<String>,
}

#[derive(Deserialize)]
pub struct KnowledgeForgetPlanBody {
    pub query: String,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct KnowledgeForgetPlan {
    pub plan_id: String,
    pub expected_profile_version: u64,
    pub exact_profile_items: Vec<clawseed_api::user_profile::ProfileItem>,
    pub possible_profile_items: Vec<clawseed_api::user_profile::ProfileItem>,
    pub exact_memories: Vec<MemoryEntry>,
    pub possible_memories: Vec<MemoryEntry>,
    pub requires_confirmation: bool,
    pub expires_at: String,
}

#[derive(Serialize)]
pub struct KnowledgeForgetResult {
    pub operation_id: String,
    pub profile_version: u64,
    pub profile_affected: usize,
    pub memories_deleted: usize,
    pub skipped: usize,
}

fn memory_category(value: Option<&str>) -> Option<MemoryCategory> {
    value.map(|category| match category {
        "core" => MemoryCategory::Core,
        "daily" => MemoryCategory::Daily,
        "conversation" => MemoryCategory::Conversation,
        other => MemoryCategory::Custom(other.to_string()),
    })
}

enum MemoryScopeError {
    BadRequest(&'static str),
    NotFound(&'static str),
    Forbidden(&'static str),
}

impl IntoResponse for MemoryScopeError {
    fn into_response(self) -> axum::response::Response {
        let (status, message) = match self {
            Self::BadRequest(message) => (StatusCode::BAD_REQUEST, message),
            Self::NotFound(message) => (StatusCode::NOT_FOUND, message),
            Self::Forbidden(message) => (StatusCode::FORBIDDEN, message),
        };
        (status, Json(serde_json::json!({"error": message}))).into_response()
    }
}

fn authorized_memory_scopes(
    state: &AppState,
    namespace: Option<&str>,
    persona: Option<&str>,
    include_public_default: bool,
) -> Result<Vec<String>, MemoryScopeError> {
    if namespace.is_some() && persona.is_some() {
        return Err(MemoryScopeError::BadRequest(
            "Specify namespace or persona, not both",
        ));
    }
    let config = state.config.lock();
    let default_namespace = config.memory.namespace.as_deref().unwrap_or("default");
    let selected = if let Some(persona) = persona {
        let Some(entry) = config.agents.get(persona) else {
            return Err(MemoryScopeError::NotFound("Persona not found"));
        };
        entry
            .memory_namespace
            .as_deref()
            .unwrap_or(default_namespace)
            .to_string()
    } else if let Some(namespace) = namespace {
        let authorized = namespace == default_namespace
            || namespace == clawseed_memory::namespaced::PUBLIC_NAMESPACE
            || config
                .agents
                .values()
                .any(|entry| entry.memory_namespace.as_deref() == Some(namespace));
        if !authorized {
            return Err(MemoryScopeError::Forbidden(
                "Memory namespace is not authorized",
            ));
        }
        namespace.to_string()
    } else {
        default_namespace.to_string()
    };
    let mut scopes = vec![selected];
    if namespace.is_none() && persona.is_none() && include_public_default {
        let public = clawseed_memory::namespaced::PUBLIC_NAMESPACE.to_string();
        if scopes[0] != public {
            scopes.push(public);
        }
    }
    Ok(scopes)
}

#[derive(Deserialize)]
pub struct UserProfileCreateBody {
    pub key: String,
    pub value: serde_json::Value,
    pub category: ProfileCategory,
    pub expires_at: Option<String>,
}

#[derive(Deserialize)]
pub struct UserProfilePatchBody {
    pub value: Option<serde_json::Value>,
    pub category: Option<ProfileCategory>,
    pub status: Option<ProfileStatus>,
    pub expires_at: Option<String>,
    #[serde(default)]
    pub clear_expires_at: bool,
}

#[derive(Deserialize)]
pub struct UserProfileImportItemBody {
    pub key: String,
    pub value: serde_json::Value,
    pub category: ProfileCategory,
    pub confidence: f64,
    pub status: ProfileStatus,
    pub evidence_session_id: Option<String>,
    pub expires_at: Option<String>,
}

#[derive(Deserialize)]
pub struct UserProfileImportBody {
    pub strategy: ProfileImportStrategy,
    pub items: Vec<UserProfileImportItemBody>,
}

#[derive(Deserialize)]
pub struct UserProfileChangePlanBody {
    pub actions: Vec<ProfileChangeAction>,
}

fn profile_plan_error(error: anyhow::Error) -> axum::response::Response {
    let message = error.to_string();
    let status = if message.contains("expired") {
        StatusCode::GONE
    } else if message.contains("version conflict") || message.contains("already") {
        StatusCode::CONFLICT
    } else if message.contains("not found") {
        StatusCode::NOT_FOUND
    } else if message.contains("maximum affected") {
        StatusCode::UNPROCESSABLE_ENTITY
    } else {
        StatusCode::BAD_REQUEST
    };
    (status, Json(serde_json::json!({"error": message}))).into_response()
}

/// GET /api/memory — list or search memory entries
pub async fn handle_api_memory_list(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(params): Query<MemoryQuery>,
) -> impl IntoResponse {
    if let Err(e) = require_auth(&state, &headers) {
        return e.into_response();
    }

    let scopes = match authorized_memory_scopes(
        &state,
        params.namespace.as_deref(),
        params.persona.as_deref(),
        true,
    ) {
        Ok(scopes) => scopes,
        Err(error) => return error.into_response(),
    };
    let category = memory_category(params.category.as_deref());

    // Use recall when query or time range is provided
    if params.query.is_some() || params.since.is_some() || params.until.is_some() {
        let query = params.query.as_deref().unwrap_or("");
        let since = params.since.as_deref();
        let until = params.until.as_deref();
        let mut entries = Vec::new();
        for namespace in &scopes {
            match state
                .mem
                .recall_scoped(ScopedMemoryQuery {
                    query,
                    scope: MemoryScope {
                        namespace,
                        session_id: None,
                    },
                    category: category.as_ref(),
                    since,
                    until,
                    limit: 50,
                    min_relevance_score: None,
                    search_mode: None,
                    exclude_ids: &[],
                    exclude_keys: &[],
                })
                .await
            {
                Ok(found) => entries.extend(found),
                Err(error) => {
                    return (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(
                            serde_json::json!({"error": format!("Memory recall failed: {error}")}),
                        ),
                    )
                        .into_response();
                }
            }
        }
        entries.sort_by(|left, right| {
            right
                .score
                .unwrap_or_default()
                .partial_cmp(&left.score.unwrap_or_default())
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| right.timestamp.cmp(&left.timestamp))
                .then_with(|| left.key.cmp(&right.key))
        });
        entries.truncate(50);
        Json(serde_json::json!({"entries": entries})).into_response()
    } else {
        let mut entries = Vec::new();
        for namespace in &scopes {
            match state
                .mem
                .list_scoped(
                    MemoryScope {
                        namespace,
                        session_id: None,
                    },
                    category.as_ref(),
                )
                .await
            {
                Ok(found) => entries.extend(found),
                Err(error) => {
                    return (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(serde_json::json!({"error": format!("Memory list failed: {error}")})),
                    )
                        .into_response();
                }
            }
        }
        entries.sort_by(|left, right| {
            right
                .timestamp
                .cmp(&left.timestamp)
                .then_with(|| left.key.cmp(&right.key))
        });
        Json(serde_json::json!({"entries": entries})).into_response()
    }
}

/// POST /api/memory — store a memory entry
pub async fn handle_api_memory_store(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<MemoryStoreBody>,
) -> impl IntoResponse {
    if let Err(e) = require_auth(&state, &headers) {
        return e.into_response();
    }

    let category = memory_category(body.category.as_deref()).unwrap_or(MemoryCategory::Core);
    let namespace = match authorized_memory_scopes(
        &state,
        body.namespace.as_deref(),
        body.persona.as_deref(),
        false,
    ) {
        Ok(mut scopes) => scopes.remove(0),
        Err(error) => return error.into_response(),
    };

    match state
        .mem
        .store_with_metadata(
            &body.key,
            &body.content,
            category,
            None,
            Some(&namespace),
            None,
        )
        .await
    {
        Ok(()) => Json(serde_json::json!({"status": "ok"})).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": format!("Memory store failed: {e}")})),
        )
            .into_response(),
    }
}

/// DELETE /api/memory/:key — delete a memory entry
pub async fn handle_api_memory_delete(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(key): Path<String>,
    Query(scope): Query<MemoryScopeQuery>,
) -> impl IntoResponse {
    if let Err(e) = require_auth(&state, &headers) {
        return e.into_response();
    }

    let namespace = match authorized_memory_scopes(
        &state,
        scope.namespace.as_deref(),
        scope.persona.as_deref(),
        false,
    ) {
        Ok(mut scopes) => scopes.remove(0),
        Err(error) => return error.into_response(),
    };
    match state
        .mem
        .forget_scoped(
            MemoryScope {
                namespace: &namespace,
                session_id: None,
            },
            &key,
        )
        .await
    {
        Ok(deleted) => {
            Json(serde_json::json!({"status": "ok", "deleted": deleted})).into_response()
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": format!("Memory forget failed: {e}")})),
        )
            .into_response(),
    }
}

fn profile_store_disabled_response() -> axum::response::Response {
    (
        StatusCode::NOT_FOUND,
        Json(serde_json::json!({"error": "User modeling is disabled"})),
    )
        .into_response()
}

/// GET /api/users/me/profile — load the authenticated local user's profile.
pub async fn handle_api_user_profile_get(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if let Err(error) = require_auth(&state, &headers) {
        return error.into_response();
    }
    let Some(store) = state.user_profile_store.as_ref() else {
        return profile_store_disabled_response();
    };
    match store.load(crate::LOCAL_OWNER_USER_ID).await {
        Ok(profile) => Json(profile).into_response(),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": format!("Profile load failed: {error}")})),
        )
            .into_response(),
    }
}

/// POST /api/users/me/profile — create or replace a profile key.
pub async fn handle_api_user_profile_upsert(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<UserProfileCreateBody>,
) -> impl IntoResponse {
    if let Err(error) = require_auth(&state, &headers) {
        return error.into_response();
    }
    let Some(store) = state.user_profile_store.as_ref() else {
        return profile_store_disabled_response();
    };
    let input = ProfileItemInput {
        key: body.key,
        value: body.value,
        category: body.category,
        confidence: 1.0,
        source: ProfileSource::Explicit,
        status: ProfileStatus::Active,
        evidence_session_id: None,
        expires_at: body.expires_at,
    };
    match store.upsert(crate::LOCAL_OWNER_USER_ID, input).await {
        Ok(item) => (StatusCode::CREATED, Json(item)).into_response(),
        Err(error) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": error.to_string()})),
        )
            .into_response(),
    }
}

/// PATCH /api/users/me/profile/items/{id} — update a profile item.
pub async fn handle_api_user_profile_patch(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(body): Json<UserProfilePatchBody>,
) -> impl IntoResponse {
    if let Err(error) = require_auth(&state, &headers) {
        return error.into_response();
    }
    let Some(store) = state.user_profile_store.as_ref() else {
        return profile_store_disabled_response();
    };
    let profile = match store.load(crate::LOCAL_OWNER_USER_ID).await {
        Ok(profile) => profile,
        Err(error) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": format!("Profile load failed: {error}")})),
            )
                .into_response();
        }
    };
    let Some(current) = profile.items.into_iter().find(|item| item.id == id) else {
        return (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "Profile item not found"})),
        )
            .into_response();
    };
    let expires_at = if body.clear_expires_at {
        None
    } else {
        body.expires_at.clone().or(current.expires_at.clone())
    };
    if body.status == Some(ProfileStatus::Superseded) {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "Profile status may only be active or rejected"})),
        )
            .into_response();
    }
    let has_manual_edit = body.value.is_some()
        || body.category.is_some()
        || body.expires_at.is_some()
        || body.clear_expires_at;
    let status = body.status.unwrap_or(if has_manual_edit {
        ProfileStatus::Active
    } else {
        current.status
    });
    let (confidence, source) = if has_manual_edit || status == ProfileStatus::Active {
        (1.0, ProfileSource::Explicit)
    } else {
        (current.confidence, current.source)
    };
    let input = ProfileItemInput {
        key: current.key,
        value: body.value.unwrap_or(current.value),
        category: body.category.unwrap_or(current.category),
        confidence,
        source,
        status,
        evidence_session_id: current.evidence_session_id,
        expires_at,
    };
    match store.upsert(crate::LOCAL_OWNER_USER_ID, input).await {
        Ok(item) => Json(item).into_response(),
        Err(error) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": error.to_string()})),
        )
            .into_response(),
    }
}

/// DELETE /api/users/me/profile/items/{id} — delete one profile item.
pub async fn handle_api_user_profile_delete(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> impl IntoResponse {
    if let Err(error) = require_auth(&state, &headers) {
        return error.into_response();
    }
    let Some(store) = state.user_profile_store.as_ref() else {
        return profile_store_disabled_response();
    };
    match store.delete_item(crate::LOCAL_OWNER_USER_ID, &id).await {
        Ok(true) => Json(serde_json::json!({"deleted": true})).into_response(),
        Ok(false) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "Profile item not found"})),
        )
            .into_response(),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": format!("Profile delete failed: {error}")})),
        )
            .into_response(),
    }
}

/// DELETE /api/users/me/profile — remove all profile items for the user.
pub async fn handle_api_user_profile_clear(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if let Err(error) = require_auth(&state, &headers) {
        return error.into_response();
    }
    let Some(store) = state.user_profile_store.as_ref() else {
        return profile_store_disabled_response();
    };
    match store.clear(crate::LOCAL_OWNER_USER_ID).await {
        Ok(deleted) => Json(serde_json::json!({"deleted": deleted})).into_response(),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": format!("Profile clear failed: {error}")})),
        )
            .into_response(),
    }
}

/// PUT /api/users/me/profile/import — atomically restore a profile backup.
pub async fn handle_api_user_profile_import(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<UserProfileImportBody>,
) -> impl IntoResponse {
    if let Err(error) = require_auth(&state, &headers) {
        return error.into_response();
    }
    let Some(store) = state.user_profile_store.as_ref() else {
        return profile_store_disabled_response();
    };
    if body.items.len() > 512 {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "Profile import exceeds 512 items"})),
        )
            .into_response();
    }
    let items = body
        .items
        .into_iter()
        .map(|item| ProfileItemInput {
            key: item.key,
            value: item.value,
            category: item.category,
            confidence: item.confidence,
            source: ProfileSource::Imported,
            status: item.status,
            evidence_session_id: item.evidence_session_id,
            expires_at: item.expires_at,
        })
        .collect();
    match store
        .import_items(crate::LOCAL_OWNER_USER_ID, items, body.strategy)
        .await
    {
        Ok(result) => Json(result).into_response(),
        Err(error) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": error.to_string()})),
        )
            .into_response(),
    }
}

/// POST /api/users/me/profile/search — deterministic profile filtering.
pub async fn handle_api_user_profile_search(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(query): Json<ProfileSearchQuery>,
) -> impl IntoResponse {
    if let Err(error) = require_auth(&state, &headers) {
        return error.into_response();
    }
    let Some(store) = state.user_profile_store.as_ref() else {
        return profile_store_disabled_response();
    };
    match store.search(crate::LOCAL_OWNER_USER_ID, query).await {
        Ok(items) => Json(serde_json::json!({"items": items})).into_response(),
        Err(error) => profile_plan_error(error),
    }
}

/// POST /api/users/me/profile/change-plans — persist a bounded preview.
pub async fn handle_api_user_profile_change_plan(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<UserProfileChangePlanBody>,
) -> impl IntoResponse {
    if let Err(error) = require_auth(&state, &headers) {
        return error.into_response();
    }
    let Some(store) = state.user_profile_store.as_ref() else {
        return profile_store_disabled_response();
    };
    let ttl = state.config.lock().user_model.change_plan_ttl_minutes;
    match store
        .create_change_plan(crate::LOCAL_OWNER_USER_ID, body.actions, ttl, 100)
        .await
    {
        Ok(plan) => Json(plan).into_response(),
        Err(error) => profile_plan_error(error),
    }
}

/// POST /api/users/me/profile/change-plans/{plan_id}/apply — apply once.
pub async fn handle_api_user_profile_apply_plan(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(plan_id): Path<String>,
) -> impl IntoResponse {
    if let Err(error) = require_auth(&state, &headers) {
        return error.into_response();
    }
    let Some(store) = state.user_profile_store.as_ref() else {
        return profile_store_disabled_response();
    };
    match store
        .apply_change_plan(crate::LOCAL_OWNER_USER_ID, &plan_id, 100)
        .await
    {
        Ok(result) => Json(result).into_response(),
        Err(error) => profile_plan_error(error),
    }
}

/// POST /api/users/me/profile/operations/{operation_id}/undo — restore atomically.
pub async fn handle_api_user_profile_undo(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(operation_id): Path<String>,
) -> impl IntoResponse {
    if let Err(error) = require_auth(&state, &headers) {
        return error.into_response();
    }
    let Some(store) = state.user_profile_store.as_ref() else {
        return profile_store_disabled_response();
    };
    let retention = state.config.lock().user_model.undo_retention_hours;
    match store
        .undo_operation(crate::LOCAL_OWNER_USER_ID, &operation_id, retention)
        .await
    {
        Ok(result) => Json(result).into_response(),
        Err(error) => profile_plan_error(error),
    }
}

fn all_user_memory_namespaces(state: &AppState) -> Vec<String> {
    let config = state.config.lock();
    let mut namespaces = BTreeSet::new();
    namespaces.insert(
        config
            .memory
            .namespace
            .clone()
            .unwrap_or_else(|| "default".into()),
    );
    namespaces.insert(clawseed_memory::namespaced::PUBLIC_NAMESPACE.into());
    namespaces.extend(
        config
            .agents
            .values()
            .filter_map(|entry| entry.memory_namespace.clone()),
    );
    namespaces.into_iter().collect()
}

async fn save_forget_plan(
    workspace: std::path::PathBuf,
    plan: KnowledgeForgetPlan,
    profile_plan_id: Option<String>,
) -> anyhow::Result<()> {
    tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
        let dir = workspace.join("user_model");
        std::fs::create_dir_all(&dir)?;
        let conn = rusqlite::Connection::open(dir.join("knowledge_operations.db"))?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS knowledge_forget_plans (
                 plan_id TEXT PRIMARY KEY,
                 user_id TEXT NOT NULL,
                 plan_json TEXT NOT NULL,
                 profile_plan_id TEXT,
                 status TEXT NOT NULL,
                 error TEXT,
                 expires_at TEXT NOT NULL,
                 updated_at TEXT NOT NULL
             );",
        )?;
        conn.execute(
            "DELETE FROM knowledge_forget_plans WHERE expires_at <= ?1 AND status = 'pending'",
            rusqlite::params![chrono::Utc::now().to_rfc3339()],
        )?;
        conn.execute(
            "INSERT INTO knowledge_forget_plans(
                 plan_id, user_id, plan_json, profile_plan_id, status, error, expires_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, 'pending', NULL, ?5, ?6)",
            rusqlite::params![
                plan.plan_id,
                crate::LOCAL_OWNER_USER_ID,
                serde_json::to_string(&plan)?,
                profile_plan_id,
                plan.expires_at,
                chrono::Utc::now().to_rfc3339(),
            ],
        )?;
        Ok(())
    })
    .await?
}

async fn load_forget_plan(
    workspace: std::path::PathBuf,
    plan_id: String,
) -> anyhow::Result<(KnowledgeForgetPlan, Option<String>, String)> {
    tokio::task::spawn_blocking(move || -> anyhow::Result<_> {
        let conn = rusqlite::Connection::open(
            workspace.join("user_model").join("knowledge_operations.db"),
        )?;
        use rusqlite::OptionalExtension;
        let row: Option<(String, Option<String>, String)> = conn
            .query_row(
                "SELECT plan_json, profile_plan_id, status
                 FROM knowledge_forget_plans WHERE plan_id = ?1 AND user_id = ?2",
                rusqlite::params![plan_id, crate::LOCAL_OWNER_USER_ID],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()?;
        let (json, profile_plan_id, status) =
            row.ok_or_else(|| anyhow::anyhow!("knowledge forget plan not found"))?;
        Ok((serde_json::from_str(&json)?, profile_plan_id, status))
    })
    .await?
}

async fn mark_forget_plan(
    workspace: std::path::PathBuf,
    plan_id: String,
    status: &'static str,
    error: Option<String>,
) -> anyhow::Result<()> {
    tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
        let conn = rusqlite::Connection::open(
            workspace.join("user_model").join("knowledge_operations.db"),
        )?;
        conn.execute(
            "UPDATE knowledge_forget_plans SET status = ?1, error = ?2, updated_at = ?3
             WHERE plan_id = ?4 AND user_id = ?5",
            rusqlite::params![
                status,
                error,
                chrono::Utc::now().to_rfc3339(),
                plan_id,
                crate::LOCAL_OWNER_USER_ID,
            ],
        )?;
        Ok(())
    })
    .await?
}

async fn restore_memory_snapshots(
    memory: &dyn clawseed_api::memory_traits::Memory,
    entries: &[MemoryEntry],
) -> Vec<String> {
    let mut errors = Vec::new();
    for entry in entries {
        let scope = MemoryScope {
            namespace: &entry.namespace,
            session_id: entry.session_id.as_deref(),
        };
        match memory.get_scoped(scope, &entry.key).await {
            Ok(Some(_)) => continue,
            Ok(None) => {}
            Err(error) => {
                errors.push(error.to_string());
                continue;
            }
        }
        if let Err(error) = memory
            .store_with_metadata(
                &entry.key,
                &entry.content,
                entry.category.clone(),
                entry.session_id.as_deref(),
                Some(&entry.namespace),
                entry.importance,
            )
            .await
        {
            errors.push(error.to_string());
        }
    }
    errors
}

async fn forget_targets_are_absent(
    memory: &dyn clawseed_api::memory_traits::Memory,
    profile: &clawseed_api::user_profile::UserProfile,
    plan: &KnowledgeForgetPlan,
) -> anyhow::Result<bool> {
    if plan
        .exact_profile_items
        .iter()
        .any(|target| profile.items.iter().any(|item| item.id == target.id))
    {
        return Ok(false);
    }
    for entry in &plan.exact_memories {
        if memory
            .get_scoped(
                MemoryScope {
                    namespace: &entry.namespace,
                    session_id: entry.session_id.as_deref(),
                },
                &entry.key,
            )
            .await?
            .is_some()
        {
            return Ok(false);
        }
    }
    Ok(true)
}

/// POST /api/users/me/knowledge/forget-plans — preview exact and possible matches.
pub async fn handle_api_knowledge_forget_plan(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<KnowledgeForgetPlanBody>,
) -> impl IntoResponse {
    if let Err(error) = require_auth(&state, &headers) {
        return error.into_response();
    }
    let query = body.query.trim();
    if query.is_empty() || query.len() > 1_024 {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "query must contain 1 to 1024 bytes"})),
        )
            .into_response();
    }
    let Some(store) = state.user_profile_store.as_ref() else {
        return profile_store_disabled_response();
    };
    let profile = match store.load(crate::LOCAL_OWNER_USER_ID).await {
        Ok(profile) => profile,
        Err(error) => return profile_plan_error(error),
    };
    let profile_matches = match store
        .search(
            crate::LOCAL_OWNER_USER_ID,
            ProfileSearchQuery {
                text: Some(query.to_string()),
                ..Default::default()
            },
        )
        .await
    {
        Ok(items) => items,
        Err(error) => return profile_plan_error(error),
    };
    let normalized = query.to_lowercase();
    let (exact_profile_items, possible_profile_items): (Vec<_>, Vec<_>) =
        profile_matches.into_iter().partition(|item| {
            item.key.to_lowercase().contains(&normalized)
                || item.value.to_string().to_lowercase().contains(&normalized)
        });

    let mut memory_matches = Vec::new();
    for namespace in all_user_memory_namespaces(&state) {
        match state
            .mem
            .recall_scoped(ScopedMemoryQuery {
                query,
                scope: MemoryScope {
                    namespace: &namespace,
                    session_id: None,
                },
                category: None,
                since: None,
                until: None,
                limit: 100,
                min_relevance_score: None,
                search_mode: None,
                exclude_ids: &[],
                exclude_keys: &[],
            })
            .await
        {
            Ok(entries) => memory_matches.extend(entries),
            Err(error) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({"error": format!("Memory search failed: {error}")})),
                )
                    .into_response();
            }
        }
    }
    memory_matches.sort_by(|left, right| left.id.cmp(&right.id));
    memory_matches.dedup_by(|left, right| left.id == right.id);
    let (mut exact_memories, mut possible_memories): (Vec<_>, Vec<_>) =
        memory_matches.into_iter().partition(|entry| {
            entry.key.to_lowercase().contains(&normalized)
                || entry.content.to_lowercase().contains(&normalized)
        });
    exact_memories.truncate(100);
    possible_memories.truncate(100);
    if exact_profile_items.len() + exact_memories.len() > 100 {
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(serde_json::json!({"error": "forget plan exceeds maximum affected items"})),
        )
            .into_response();
    }

    let ttl = state.config.lock().user_model.change_plan_ttl_minutes;
    let profile_plan_id = if exact_profile_items.is_empty() {
        None
    } else {
        let actions = exact_profile_items
            .iter()
            .map(|item| ProfileChangeAction::Delete {
                item_id: item.id.clone(),
            })
            .collect();
        match store
            .create_change_plan(crate::LOCAL_OWNER_USER_ID, actions, ttl, 100)
            .await
        {
            Ok(plan) => Some(plan.plan_id),
            Err(error) => return profile_plan_error(error),
        }
    };
    let plan = KnowledgeForgetPlan {
        plan_id: uuid::Uuid::new_v4().to_string(),
        expected_profile_version: profile.version,
        exact_profile_items,
        possible_profile_items,
        exact_memories,
        possible_memories,
        requires_confirmation: true,
        expires_at: (chrono::Utc::now() + chrono::Duration::minutes(ttl.clamp(1, 60) as i64))
            .to_rfc3339(),
    };
    let workspace = state.config.lock().workspace_dir.clone();
    match save_forget_plan(workspace, plan.clone(), profile_plan_id).await {
        Ok(()) => Json(plan).into_response(),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": format!("Forget plan save failed: {error}")})),
        )
            .into_response(),
    }
}

/// POST /api/users/me/knowledge/forget-plans/{plan_id}/apply — compensated cross-store delete.
pub async fn handle_api_knowledge_forget_apply(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(plan_id): Path<String>,
) -> impl IntoResponse {
    if let Err(error) = require_auth(&state, &headers) {
        return error.into_response();
    }
    let Some(store) = state.user_profile_store.as_ref() else {
        return profile_store_disabled_response();
    };
    let workspace = state.config.lock().workspace_dir.clone();
    let (plan, profile_plan_id, status) =
        match load_forget_plan(workspace.clone(), plan_id.clone()).await {
            Ok(value) => value,
            Err(error) => return profile_plan_error(error),
        };
    if status != "pending" && status != "applying" {
        return profile_plan_error(anyhow::anyhow!("knowledge forget plan was already applied"));
    }
    if status == "pending" && plan.expires_at.as_str() <= chrono::Utc::now().to_rfc3339().as_str() {
        return profile_plan_error(anyhow::anyhow!("knowledge forget plan expired"));
    }
    let current = match store.load(crate::LOCAL_OWNER_USER_ID).await {
        Ok(profile) => profile,
        Err(error) => return profile_plan_error(error),
    };
    if status == "applying" && current.version != plan.expected_profile_version {
        match forget_targets_are_absent(state.mem.as_ref(), &current, &plan).await {
            Ok(true) => {
                if let Err(error) =
                    mark_forget_plan(workspace, plan_id.clone(), "completed", None).await
                {
                    return profile_plan_error(error);
                }
                return Json(KnowledgeForgetResult {
                    operation_id: plan_id,
                    profile_version: current.version,
                    profile_affected: plan.exact_profile_items.len(),
                    memories_deleted: plan.exact_memories.len(),
                    skipped: 0,
                })
                .into_response();
            }
            Ok(false) => {
                let compensation_errors =
                    restore_memory_snapshots(state.mem.as_ref(), &plan.exact_memories).await;
                let journal_status = if compensation_errors.is_empty() {
                    "failed_compensated"
                } else {
                    "compensation_failed"
                };
                let _ = mark_forget_plan(
                    workspace,
                    plan_id,
                    journal_status,
                    Some("profile changed while recovering knowledge forget plan".into()),
                )
                .await;
                return profile_plan_error(anyhow::anyhow!(
                    "profile version conflict: expected {}, current {}",
                    plan.expected_profile_version,
                    current.version
                ));
            }
            Err(error) => return profile_plan_error(error),
        }
    }
    if current.version != plan.expected_profile_version {
        return profile_plan_error(anyhow::anyhow!(
            "profile version conflict: expected {}, current {}",
            plan.expected_profile_version,
            current.version
        ));
    }
    if status == "pending"
        && let Err(error) =
            mark_forget_plan(workspace.clone(), plan_id.clone(), "applying", None).await
    {
        return profile_plan_error(error);
    }

    let mut deleted = Vec::new();
    let mut skipped = 0;
    for entry in &plan.exact_memories {
        match state
            .mem
            .forget_scoped(
                MemoryScope {
                    namespace: &entry.namespace,
                    session_id: entry.session_id.as_deref(),
                },
                &entry.key,
            )
            .await
        {
            Ok(true) => deleted.push(entry.clone()),
            Ok(false) => skipped += 1,
            Err(error) => {
                let compensation_errors =
                    restore_memory_snapshots(state.mem.as_ref(), &plan.exact_memories).await;
                let journal_status = if compensation_errors.is_empty() {
                    "failed_compensated"
                } else {
                    "compensation_failed"
                };
                let message = format!(
                    "memory delete failed: {error}; compensation: {}",
                    compensation_errors.join("; ")
                );
                let _ = mark_forget_plan(workspace, plan_id, journal_status, Some(message.clone()))
                    .await;
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({"error": message})),
                )
                    .into_response();
            }
        }
    }

    let mut profile_result = None;
    if let Some(profile_plan_id) = profile_plan_id {
        match store
            .apply_change_plan(crate::LOCAL_OWNER_USER_ID, &profile_plan_id, 100)
            .await
        {
            Ok(result) => profile_result = Some(result),
            Err(error) => {
                let compensation_errors =
                    restore_memory_snapshots(state.mem.as_ref(), &plan.exact_memories).await;
                let journal_status = if compensation_errors.is_empty() {
                    "failed_compensated"
                } else {
                    "compensation_failed"
                };
                let message = format!(
                    "profile delete failed: {error}; compensation: {}",
                    compensation_errors.join("; ")
                );
                let _ = mark_forget_plan(workspace, plan_id, journal_status, Some(message.clone()))
                    .await;
                return profile_plan_error(anyhow::anyhow!(message));
            }
        }
    }
    let profile_version = profile_result
        .as_ref()
        .map_or(current.version, |result| result.profile_version);
    let profile_affected = profile_result.as_ref().map_or(0, |result| result.affected);
    let operation_id = profile_result
        .as_ref()
        .map(|result| result.operation_id.clone())
        .unwrap_or_else(|| plan.plan_id.clone());
    if let Err(error) = mark_forget_plan(workspace, plan_id, "completed", None).await {
        return profile_plan_error(error);
    }
    Json(KnowledgeForgetResult {
        operation_id,
        profile_version,
        profile_affected,
        memories_deleted: deleted.len(),
        skipped,
    })
    .into_response()
}

/// GET /api/users/me/knowledge/legacy-report — read-only profile/Core duplicate scan.
pub async fn handle_api_knowledge_legacy_report(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if let Err(error) = require_auth(&state, &headers) {
        return error.into_response();
    }
    let Some(store) = state.user_profile_store.as_ref() else {
        return profile_store_disabled_response();
    };
    let profile = match store.load(crate::LOCAL_OWNER_USER_ID).await {
        Ok(profile) => profile,
        Err(error) => return profile_plan_error(error),
    };
    let mut definite = Vec::new();
    let mut possible = Vec::new();
    let mut retained = Vec::new();
    for namespace in all_user_memory_namespaces(&state) {
        let entries = match state
            .mem
            .list_scoped(
                MemoryScope {
                    namespace: &namespace,
                    session_id: None,
                },
                Some(&MemoryCategory::Core),
            )
            .await
        {
            Ok(entries) => entries,
            Err(error) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({"error": error.to_string()})),
                )
                    .into_response();
            }
        };
        for entry in entries {
            let key_match = profile.items.iter().any(|item| item.key == entry.key);
            let value_match = profile.items.iter().any(|item| {
                item.value.as_str().map(str::trim).unwrap_or_default() == entry.content.trim()
            });
            if key_match || value_match {
                definite.push(entry);
            } else if profile.items.iter().any(|item| {
                let value = item.value.to_string().to_lowercase();
                let content = entry.content.to_lowercase();
                value.len() >= 4 && content.contains(value.trim_matches('"'))
            }) {
                possible.push(entry);
            } else {
                retained.push(entry);
            }
        }
    }
    Json(serde_json::json!({
        "generated_at": chrono::Utc::now().to_rfc3339(),
        "definite_profile_duplicates": definite,
        "possible_profile_duplicates": possible,
        "retain_as_memory": retained,
        "applied": false,
    }))
    .into_response()
}
