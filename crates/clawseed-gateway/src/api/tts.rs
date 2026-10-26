use super::{AppState, require_auth};
use axum::{
    extract::State,
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use clawseed_config::schema::{Config, build_runtime_proxy_client_with_timeouts};
use serde::Deserialize;
use serde_json::{Value, json};

const DEFAULT_BASE_URL: &str = "https://api.xiaomimimo.com/v1";
const DEFAULT_MODEL: &str = "mimo-v2.5-tts";
const DEFAULT_VOICE: &str = "Chloe";
const MAX_TEXT_CHARS: usize = 8_000;

#[derive(Debug, Deserialize)]
pub struct TtsRequest {
    pub text: String,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub voice: Option<String>,
    #[serde(default)]
    pub instructions: Option<String>,
    #[serde(default)]
    pub format: Option<String>,
}

/// Generate one MiMo TTS segment and return decoded audio bytes.
pub async fn handle_api_tts(
    State(state): State<AppState>,
    headers: HeaderMap,
    axum::Json(request): axum::Json<TtsRequest>,
) -> Response {
    if let Err(error) = require_auth(&state, &headers) {
        return error.into_response();
    }

    let text = request.text.trim();
    if text.is_empty() {
        return error_response(StatusCode::BAD_REQUEST, "text must not be empty");
    }
    if text.chars().count() > MAX_TEXT_CHARS {
        return error_response(
            StatusCode::BAD_REQUEST,
            &format!("text exceeds the {MAX_TEXT_CHARS}-character limit"),
        );
    }

    let config = state.config.lock().clone();
    let Some(api_key) = mimo_api_key(&config) else {
        return error_response(
            StatusCode::PRECONDITION_FAILED,
            "MiMo API key is not configured; save a key for the mimo provider first",
        );
    };

    let model = request
        .model
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(DEFAULT_MODEL);
    let voice = request
        .voice
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(DEFAULT_VOICE);
    let format = request
        .format
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("wav");
    if !matches!(format, "wav" | "mp3" | "pcm16") {
        return error_response(StatusCode::BAD_REQUEST, "format must be wav, mp3, or pcm16");
    }
    if model.ends_with("voiceclone") {
        return error_response(
            StatusCode::BAD_REQUEST,
            "MiMo voice cloning is not enabled in this client yet",
        );
    }

    let endpoint = format!(
        "{}/chat/completions",
        mimo_base_url(&config).trim_end_matches('/')
    );
    let style = request.instructions.as_deref().unwrap_or(
        "Speak naturally and clearly. Preserve the language, punctuation, and intent of the text.",
    );
    let mut audio = json!({"format": format});
    if !model.ends_with("voicedesign") {
        audio["voice"] = json!(voice);
    }
    let payload = json!({
        "model": model,
        "messages": [
            {"role": "user", "content": style},
            {"role": "assistant", "content": text},
        ],
        "audio": audio,
    });

    let client = build_runtime_proxy_client_with_timeouts("tts.mimo", 60, 10);
    let response = match client
        .post(endpoint)
        .bearer_auth(api_key)
        .json(&payload)
        .send()
        .await
    {
        Ok(response) => response,
        Err(error) => {
            tracing::warn!(%error, "MiMo TTS request failed");
            return error_response(StatusCode::BAD_GATEWAY, "MiMo TTS request failed");
        }
    };

    let status = response.status();
    let body = match response.json::<Value>().await {
        Ok(body) => body,
        Err(error) => {
            tracing::warn!(%error, "MiMo TTS returned invalid JSON");
            return error_response(
                StatusCode::BAD_GATEWAY,
                "MiMo TTS returned an invalid response",
            );
        }
    };
    if !status.is_success() {
        let message = body
            .get("error")
            .and_then(|value| value.get("message"))
            .and_then(Value::as_str)
            .unwrap_or("MiMo TTS rejected the request");
        tracing::warn!(status = %status, message, "MiMo TTS rejected request");
        return error_response(StatusCode::BAD_GATEWAY, message);
    }

    let Some(encoded) = body
        .pointer("/choices/0/message/audio/data")
        .and_then(Value::as_str)
    else {
        return error_response(
            StatusCode::BAD_GATEWAY,
            "MiMo TTS response did not contain audio",
        );
    };
    let audio = match STANDARD.decode(encoded) {
        Ok(audio) if !audio.is_empty() => audio,
        Ok(_) => return error_response(StatusCode::BAD_GATEWAY, "MiMo TTS returned empty audio"),
        Err(error) => {
            tracing::warn!(%error, "MiMo TTS returned invalid base64 audio");
            return error_response(StatusCode::BAD_GATEWAY, "MiMo TTS returned invalid audio");
        }
    };

    let content_type = match format {
        "mp3" => "audio/mpeg",
        "pcm16" => "audio/pcm",
        _ => "audio/wav",
    };
    (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, content_type),
            (header::CACHE_CONTROL, "no-store"),
        ],
        audio,
    )
        .into_response()
}

fn mimo_api_key(config: &Config) -> Option<String> {
    config
        .providers
        .models
        .get("mimo")
        .and_then(|provider| provider.api_key.clone())
        .or_else(|| config.providers.default_api_key.clone())
        .or_else(|| std::env::var("MIMO_API_KEY").ok())
        .filter(|key| !key.trim().is_empty() && key != "***MASKED***")
}

fn mimo_base_url(config: &Config) -> String {
    config
        .providers
        .models
        .get("mimo")
        .and_then(|provider| provider.base_url.clone())
        .filter(|url| !url.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_BASE_URL.to_string())
}

fn error_response(status: StatusCode, message: &str) -> Response {
    (status, axum::Json(json!({"error": message}))).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use clawseed_config::schema::Config;

    #[test]
    fn resolves_mimo_provider_key() {
        let mut config = Config::default();
        config.providers.models.insert(
            "mimo".to_string(),
            clawseed_config::schema::ModelProviderConfig {
                api_key: Some("mimo-key".to_string()),
                ..Default::default()
            },
        );
        assert_eq!(mimo_api_key(&config).as_deref(), Some("mimo-key"));
    }

    #[test]
    fn uses_mimo_default_endpoint() {
        assert_eq!(mimo_base_url(&Config::default()), DEFAULT_BASE_URL);
    }
}
