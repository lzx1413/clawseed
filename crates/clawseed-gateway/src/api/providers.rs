use super::{AppState, require_auth};
use axum::{
    extract::{Query, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Json},
};

#[derive(Default, serde::Deserialize)]
pub struct ProviderModelsQuery {
    /// Configured provider profile to query. When omitted, uses the default
    /// provider for backward compatibility.
    pub provider: Option<String>,
}

/// GET /api/providers — list every configured LLM provider profile.
pub async fn handle_api_providers(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if let Err(e) = require_auth(&state, &headers) {
        return e.into_response();
    }
    let config = state.config.lock().clone();
    let mut providers: Vec<serde_json::Value> = config
        .providers
        .models
        .iter()
        .map(|(id, entry)| {
            serde_json::json!({
                "id": id,
                "name": entry.name.as_deref().unwrap_or(id),
                "base_url": entry.base_url,
                "model": entry.model,
                "models": if entry.models.is_empty() {
                    entry.model.iter().cloned().collect::<Vec<_>>()
                } else {
                    entry.models.clone()
                },
                "active": config.providers.fallback.as_deref() == Some(id),
            })
        })
        .collect();
    providers.sort_by(|a, b| a["id"].as_str().cmp(&b["id"].as_str()));
    Json(serde_json::json!({"providers": providers})).into_response()
}

/// GET /api/provider/models — proxy model list fetch using configured API key
pub async fn handle_api_provider_models(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    handle_api_provider_models_query(State(state), headers, Query(ProviderModelsQuery::default()))
        .await
}

/// GET /api/provider/models?provider=:id — fetch and cache a configured
/// provider's model catalog.
pub async fn handle_api_provider_models_query(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ProviderModelsQuery>,
) -> impl IntoResponse {
    if let Err(e) = require_auth(&state, &headers) {
        return e.into_response();
    }

    let config = state.config.lock().clone();
    let provider_key = query.provider.or_else(|| config.providers.fallback.clone());
    let entry = provider_key
        .as_ref()
        .and_then(|key| config.providers.models.get(key));

    let Some(entry) = entry else {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "No provider configured"})),
        )
            .into_response();
    };

    let base_url = match entry.base_url.as_deref() {
        Some(u) if !u.is_empty() => u.trim_end_matches('/'),
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": "No base_url configured"})),
            )
                .into_response();
        }
    };

    let url = format!("{base_url}/models");
    let client = clawseed_config::schema::build_runtime_proxy_client_with_timeouts(
        "provider.models",
        15,
        10,
    );
    let mut req = client.get(&url);
    if let Some(ref key) = entry.api_key {
        req = req.header("Authorization", format!("Bearer {key}"));
    }

    match req.send().await {
        Ok(resp) if resp.status().is_success() => {
            let body = resp.text().await.unwrap_or_default();
            if let (Some(key), Some(models)) = (
                provider_key,
                parse_model_ids(&body).filter(|models| !models.is_empty()),
            ) {
                let mut updated = state.config.lock().clone();
                if let Some(entry) = updated.providers.models.get_mut(&key) {
                    entry.models = models;
                    if updated.save().is_ok() {
                        *state.config.lock() = updated;
                    }
                }
            }
            (
                StatusCode::OK,
                [(header::CONTENT_TYPE, "application/json")],
                body,
            )
                .into_response()
        }
        Ok(resp) => {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            (
                StatusCode::BAD_GATEWAY,
                Json(serde_json::json!({"error": format!("{status}: {}", body.chars().take(200).collect::<String>())})),
            )
                .into_response()
        }
        Err(e) => (
            StatusCode::BAD_GATEWAY,
            Json(serde_json::json!({"error": format!("连接失败: {e}")})),
        )
            .into_response(),
    }
}

fn parse_model_ids(body: &str) -> Option<Vec<String>> {
    let value: serde_json::Value = serde_json::from_str(body).ok()?;
    let items = value
        .get("data")
        .or_else(|| value.get("models"))
        .and_then(serde_json::Value::as_array)?;
    let mut models: Vec<String> = items
        .iter()
        .filter_map(|item| {
            item.as_str().map(str::to_owned).or_else(|| {
                item.get("id")
                    .or_else(|| item.get("name"))
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_owned)
            })
        })
        .filter(|model| !model.trim().is_empty())
        .collect();
    models.sort();
    models.dedup();
    Some(models)
}

#[cfg(test)]
mod model_catalog_tests {
    use super::parse_model_ids;

    #[test]
    fn parses_and_normalizes_common_model_catalog_shapes() {
        assert_eq!(
            parse_model_ids(r#"{"data":[{"id":"z-model"},{"id":"a-model"},{"id":"a-model"}]}"#),
            Some(vec!["a-model".to_string(), "z-model".to_string()])
        );
        assert_eq!(
            parse_model_ids(r#"{"models":[{"name":"gemini-pro"},"custom-model"]}"#),
            Some(vec!["custom-model".to_string(), "gemini-pro".to_string()])
        );
    }
}

/// GET /api/tools — list all registered tool specs (including disabled)
pub async fn handle_api_tools(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if let Err(e) = require_auth(&state, &headers) {
        return e.into_response();
    }

    let registry = &state.tool_registry;
    let tools: Vec<serde_json::Value> = registry
        .all_tool_names()
        .iter()
        .map(|name| {
            let spec = registry.get_tool_unfiltered(name).map(|t| t.spec());
            let entry = registry.get_entry(name);
            let source_type = entry.as_ref().map(|e| match &e.source {
                clawseed_api::tool_registry::ToolSource::BuiltIn => "builtin",
                clawseed_api::tool_registry::ToolSource::Mcp { .. } => "mcp",
                clawseed_api::tool_registry::ToolSource::Remote { .. } => "remote",
            });
            let source = entry.as_ref().map(|e| match &e.source {
                clawseed_api::tool_registry::ToolSource::BuiltIn => "builtin".to_string(),
                clawseed_api::tool_registry::ToolSource::Mcp { server } => server.clone(),
                clawseed_api::tool_registry::ToolSource::Remote { session } => session.clone(),
            });
            let enabled = registry.is_tool_enabled(name);
            serde_json::json!({
                "name": name,
                "description": spec.as_ref().map(|s| s.description.clone()).unwrap_or_default(),
                "parameters": spec.as_ref().map(|s| s.parameters.clone()).unwrap_or(serde_json::json!({})),
                "enabled": enabled,
                "source_type": source_type,
                "source": source,
            })
        })
        .collect();

    Json(serde_json::json!({"tools": tools})).into_response()
}

#[derive(serde::Deserialize)]
pub struct ProviderBalanceRequest {
    pub base_url: String,
    /// None reuses a saved credential only for the exact same provider URL.
    pub api_key: Option<String>,
}

/// POST /api/provider/balance — query a draft or saved provider's optional balance.
pub async fn handle_api_provider_balance(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<ProviderBalanceRequest>,
) -> impl IntoResponse {
    if let Err(e) = require_auth(&state, &headers) {
        return e.into_response();
    }
    let saved_key = if request.api_key.is_none() {
        let config = state.config.lock();
        let matches_url = |entry: &&clawseed_config::schema::ModelProviderConfig| {
            entry.base_url.as_deref().is_some_and(|url| {
                url.trim_end_matches('/') == request.base_url.trim_end_matches('/')
            })
        };
        config
            .providers
            .fallback
            .as_ref()
            .and_then(|name| config.providers.models.get(name))
            .filter(matches_url)
            .or_else(|| config.providers.models.values().find(matches_url))
            .and_then(|entry| entry.api_key.clone())
    } else {
        None
    };
    let balance = clawseed_providers::balance::query_balance(
        &request.base_url,
        request.api_key.as_deref().or(saved_key.as_deref()),
    )
    .await;
    ([(header::CACHE_CONTROL, "no-store")], Json(balance)).into_response()
}
