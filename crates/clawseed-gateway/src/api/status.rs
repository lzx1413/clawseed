use super::{AppState, require_auth};
use axum::{
    extract::State,
    http::HeaderMap,
    response::{IntoResponse, Json},
};

/// GET /api/status — system status overview
pub async fn handle_api_status(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if let Err(e) = require_auth(&state, &headers) {
        return e.into_response();
    }

    let config = state.config.lock().clone();
    let health = serde_json::Map::new();

    let mut channels = serde_json::Map::new();

    for (channel, present) in config.channels.channels() {
        channels.insert(channel.name().to_string(), serde_json::Value::Bool(present));
    }

    let locale = config
        .locale
        .as_deref()
        .filter(|s| !s.is_empty())
        .map(String::from)
        .unwrap_or_else(|| "en".to_string());

    let fallback_entry = config
        .providers
        .fallback
        .as_ref()
        .and_then(|key| config.providers.models.get(key));
    let model = fallback_entry
        .and_then(|e| e.model.clone())
        .unwrap_or_else(|| state.model.clone());
    let temperature = fallback_entry
        .and_then(|e| e.temperature)
        .unwrap_or(state.temperature);

    let mem_count = state.mem.count().await.unwrap_or(0);

    let memory_info = serde_json::json!({
        "backend": state.mem.name(),
        "embedding_provider": config.memory.embedding_provider.as_deref().unwrap_or("none"),
        "embedding_model": config.memory.embedding_model.as_deref().unwrap_or(""),
        "embedding_dims": match state.mem.name() {
            "sqlite" => {
                // Since we can't downcast Arc<dyn Memory> to SqliteMemory, use config dims
                config.memory.embedding_dims.unwrap_or(
                    match config.memory.embedding_provider.as_deref() {
                        Some("local") => 768,
                        Some("openai") | Some("openrouter") => 1536,
                        _ => 0,
                    }
                )
            }
            _ => 0,
        },
        "search_mode": config.memory.effective_search_mode().to_string(),
        "count": mem_count,
    });

    let body = serde_json::json!({
        "provider": config.providers.fallback,
        "model": model,
        "image_attachments": {
            "supported": state.session_backend.is_some(),
            "max_images_per_message": crate::session_attachments::MAX_MESSAGE_IMAGES,
            "max_image_bytes": crate::session_attachments::MAX_IMAGE_BYTES,
            "max_dimension": crate::session_attachments::MAX_IMAGE_DIMENSION,
        },
        "temperature": temperature,
        "uptime_seconds": health.get("uptime_seconds").and_then(|v| v.as_f64()).unwrap_or(0.0),
        "gateway_port": config.gateway.port,
        "locale": locale,
        "memory": memory_info,
        "paired": state.pairing.is_paired(),
        "channels": channels,
        "health": health,
    });

    Json(body).into_response()
}
