use super::{AppState, require_auth};
use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Json},
};
use clawseed_api::user_profile::{
    ProfileCategory, ProfileImportStrategy, ProfileItemInput, ProfileSource, ProfileStatus,
};
use serde::Deserialize;

#[derive(Deserialize)]
pub struct MemoryQuery {
    pub query: Option<String>,
    pub category: Option<String>,
    /// Filter memories created at or after (RFC 3339 / ISO 8601)
    pub since: Option<String>,
    /// Filter memories created at or before (RFC 3339 / ISO 8601)
    pub until: Option<String>,
}

#[derive(Deserialize)]
pub struct MemoryStoreBody {
    pub key: String,
    pub content: String,
    pub category: Option<String>,
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

/// GET /api/memory — list or search memory entries
pub async fn handle_api_memory_list(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(params): Query<MemoryQuery>,
) -> impl IntoResponse {
    if let Err(e) = require_auth(&state, &headers) {
        return e.into_response();
    }

    // Use recall when query or time range is provided
    if params.query.is_some() || params.since.is_some() || params.until.is_some() {
        let query = params.query.as_deref().unwrap_or("");
        let since = params.since.as_deref();
        let until = params.until.as_deref();
        match state.mem.recall(query, 50, None, since, until, None).await {
            Ok(entries) => Json(serde_json::json!({"entries": entries})).into_response(),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": format!("Memory recall failed: {e}")})),
            )
                .into_response(),
        }
    } else {
        // List mode
        let category = params.category.as_deref().map(|cat| match cat {
            "core" => clawseed_api::memory_traits::MemoryCategory::Core,
            "daily" => clawseed_api::memory_traits::MemoryCategory::Daily,
            "conversation" => clawseed_api::memory_traits::MemoryCategory::Conversation,
            other => clawseed_api::memory_traits::MemoryCategory::Custom(other.to_string()),
        });

        match state.mem.list(category.as_ref(), None).await {
            Ok(entries) => Json(serde_json::json!({"entries": entries})).into_response(),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": format!("Memory list failed: {e}")})),
            )
                .into_response(),
        }
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

    let category = body
        .category
        .as_deref()
        .map(|cat| match cat {
            "core" => clawseed_api::memory_traits::MemoryCategory::Core,
            "daily" => clawseed_api::memory_traits::MemoryCategory::Daily,
            "conversation" => clawseed_api::memory_traits::MemoryCategory::Conversation,
            other => clawseed_api::memory_traits::MemoryCategory::Custom(other.to_string()),
        })
        .unwrap_or(clawseed_api::memory_traits::MemoryCategory::Core);

    match state
        .mem
        .store(&body.key, &body.content, category, None)
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
) -> impl IntoResponse {
    if let Err(e) = require_auth(&state, &headers) {
        return e.into_response();
    }

    match state.mem.forget(&key).await {
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
