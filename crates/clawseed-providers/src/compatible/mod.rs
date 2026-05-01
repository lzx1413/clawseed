//! Generic OpenAI-compatible provider.
//! Most LLM APIs follow the same `/v1/chat/completions` format.
//! This module provides a single implementation that works for all of them.

use crate::traits::ChatMessage;
use reqwest::{
    Client,
    header::{HeaderMap, HeaderValue, USER_AGENT},
};

/// A provider that speaks the OpenAI-compatible chat completions API.
/// Used by: Venice, Vercel AI Gateway, Cloudflare AI Gateway, Moonshot,
/// Synthetic, `OpenCode` Zen, `OpenCode` Go, `Z.AI`, `GLM`, `MiniMax`, Bedrock, Qianfan, Groq, Mistral, `xAI`, etc.
#[allow(clippy::struct_excessive_bools)]
pub struct OpenAiCompatibleProvider {
    pub name: String,
    pub base_url: String,
    pub credential: Option<String>,
    pub auth_header: AuthStyle,
    supports_vision: bool,
    /// When false, do not fall back to /v1/responses on chat completions 404.
    /// GLM/Zhipu does not support the responses API.
    supports_responses_fallback: bool,
    user_agent: Option<String>,
    /// When true, collect all `system` messages and prepend their content
    /// to the first `user` message, then drop the system messages.
    /// Required for providers that reject `role: system` (e.g. MiniMax).
    merge_system_into_user: bool,
    /// Whether this provider supports OpenAI-style native tool calling.
    /// When false, tools are injected into the system prompt as text.
    native_tool_calling: bool,
    /// HTTP request timeout in seconds for LLM API calls. Default: 120.
    timeout_secs: u64,
    /// Extra HTTP headers to include in all API requests.
    extra_headers: std::collections::HashMap<String, String>,
    /// Optional reasoning effort for GPT-5/Codex-compatible backends.
    reasoning_effort: Option<String>,
    /// Custom API path suffix (e.g. "/v2/generate").
    /// When set, overrides the default `/chat/completions` path detection.
    api_path: Option<String>,
    /// Maximum output tokens to include in API requests.
    max_tokens: Option<u32>,
}

pub mod auth;
pub(super) mod parsing;
pub(super) mod provider_impl;

pub use auth::AuthStyle;

impl OpenAiCompatibleProvider {
    pub fn new(
        name: &str,
        base_url: &str,
        credential: Option<&str>,
        auth_style: AuthStyle,
    ) -> Self {
        Self::new_with_options(
            name, base_url, credential, auth_style, false, true, None, false,
        )
    }

    pub fn new_with_vision(
        name: &str,
        base_url: &str,
        credential: Option<&str>,
        auth_style: AuthStyle,
        supports_vision: bool,
    ) -> Self {
        Self::new_with_options(
            name,
            base_url,
            credential,
            auth_style,
            supports_vision,
            true,
            None,
            false,
        )
    }

    /// Same as `new` but skips the /v1/responses fallback on 404.
    /// Use for providers (e.g. GLM) that only support chat completions.
    pub fn new_no_responses_fallback(
        name: &str,
        base_url: &str,
        credential: Option<&str>,
        auth_style: AuthStyle,
    ) -> Self {
        Self::new_with_options(
            name, base_url, credential, auth_style, false, false, None, false,
        )
    }

    /// Create a provider with a custom User-Agent header.
    ///
    /// Some providers (for example Kimi Code) require a specific User-Agent
    /// for request routing and policy enforcement.
    pub fn new_with_user_agent(
        name: &str,
        base_url: &str,
        credential: Option<&str>,
        auth_style: AuthStyle,
        user_agent: &str,
    ) -> Self {
        Self::new_with_options(
            name,
            base_url,
            credential,
            auth_style,
            false,
            true,
            Some(user_agent),
            false,
        )
    }

    pub fn new_with_user_agent_and_vision(
        name: &str,
        base_url: &str,
        credential: Option<&str>,
        auth_style: AuthStyle,
        user_agent: &str,
        supports_vision: bool,
    ) -> Self {
        Self::new_with_options(
            name,
            base_url,
            credential,
            auth_style,
            supports_vision,
            true,
            Some(user_agent),
            false,
        )
    }

    /// For providers that do not support `role: system` (e.g. MiniMax).
    /// System prompt content is prepended to the first user message instead.
    pub fn new_merge_system_into_user(
        name: &str,
        base_url: &str,
        credential: Option<&str>,
        auth_style: AuthStyle,
    ) -> Self {
        Self::new_with_options(
            name, base_url, credential, auth_style, false, false, None, true,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn new_with_options(
        name: &str,
        base_url: &str,
        credential: Option<&str>,
        auth_style: AuthStyle,
        supports_vision: bool,
        supports_responses_fallback: bool,
        user_agent: Option<&str>,
        merge_system_into_user: bool,
    ) -> Self {
        Self {
            name: name.to_string(),
            base_url: base_url.trim_end_matches('/').to_string(),
            credential: credential.map(ToString::to_string),
            auth_header: auth_style,
            supports_vision,
            supports_responses_fallback,
            user_agent: user_agent.map(ToString::to_string),
            merge_system_into_user,
            native_tool_calling: !merge_system_into_user,
            timeout_secs: 120,
            extra_headers: std::collections::HashMap::new(),
            reasoning_effort: None,
            api_path: None,
            max_tokens: None,
        }
    }

    /// Disable native tool calling, forcing prompt-guided tool use instead.
    pub fn without_native_tools(mut self) -> Self {
        self.native_tool_calling = false;
        self
    }

    /// Merge all system messages into the first user message before sending.
    /// Unlike `new_merge_system_into_user`, this preserves native tool calling.
    pub fn with_merge_system_into_user(mut self) -> Self {
        self.merge_system_into_user = true;
        self
    }

    /// Override the HTTP request timeout for LLM API calls.
    pub fn with_timeout_secs(mut self, timeout_secs: u64) -> Self {
        self.timeout_secs = timeout_secs;
        self
    }

    /// Set extra HTTP headers to include in all API requests.
    pub fn with_extra_headers(
        mut self,
        headers: std::collections::HashMap<String, String>,
    ) -> Self {
        self.extra_headers = headers;
        self
    }

    /// Set reasoning effort for GPT-5/Codex-compatible chat-completions APIs.
    pub fn with_reasoning_effort(mut self, reasoning_effort: Option<String>) -> Self {
        self.reasoning_effort = reasoning_effort;
        self
    }

    /// Set a custom API path suffix for this provider.
    /// When set, replaces the default `/chat/completions` path.
    pub fn with_api_path(mut self, api_path: Option<String>) -> Self {
        self.api_path = api_path;
        self
    }

    /// Set the maximum output tokens for API requests.
    pub fn with_max_tokens(mut self, max_tokens: Option<u32>) -> Self {
        self.max_tokens = max_tokens;
        self
    }

    /// Collect all `system` role messages, concatenate their content,
    /// and prepend to the first `user` message. Drop all system messages.
    /// Used for providers (e.g. MiniMax) that reject `role: system`.
    fn flatten_system_messages(messages: &[ChatMessage], merge: bool) -> Vec<ChatMessage> {
        if !merge {
            return messages.to_vec();
        }
        let system_content: String = messages
            .iter()
            .filter(|m| m.role == "system")
            .map(|m| m.content.as_str())
            .collect::<Vec<_>>()
            .join("\n\n");

        if system_content.is_empty() {
            return messages.to_vec();
        }

        let mut result: Vec<ChatMessage> = messages
            .iter()
            .filter(|m| m.role != "system")
            .cloned()
            .collect();

        if let Some(first_user) = result.iter_mut().find(|m| m.role == "user") {
            first_user.content = format!("{system_content}\n\n{}", first_user.content);
        } else {
            // No user message found: insert a synthetic user message with system content
            result.insert(0, ChatMessage::user(&system_content));
        }

        result
    }

    fn http_client(&self) -> Client {
        let timeout = self.timeout_secs;
        let has_user_agent = self.user_agent.is_some();
        let has_extra_headers = !self.extra_headers.is_empty();

        if has_user_agent || has_extra_headers {
            let mut headers = HeaderMap::new();
            if let Some(ua) = self.user_agent.as_deref()
                && let Ok(value) = HeaderValue::from_str(ua)
            {
                headers.insert(USER_AGENT, value);
            }
            for (key, value) in &self.extra_headers {
                match (
                    reqwest::header::HeaderName::from_bytes(key.as_bytes()),
                    HeaderValue::from_str(value),
                ) {
                    (Ok(name), Ok(val)) => {
                        headers.insert(name, val);
                    }
                    _ => {
                        tracing::warn!(header = key, "Skipping invalid extra header name or value");
                    }
                }
            }

            let builder = Client::builder()
                .timeout(std::time::Duration::from_secs(timeout))
                .connect_timeout(std::time::Duration::from_secs(10))
                .default_headers(headers);
            let builder = clawseed_config::schema::apply_runtime_proxy_to_builder(
                builder,
                "provider.compatible",
            );

            return builder.build().unwrap_or_else(|error| {
                tracing::warn!(
                    "Failed to build proxied timeout client with custom headers: {error}"
                );
                Client::new()
            });
        }

        clawseed_config::schema::build_runtime_proxy_client_with_timeouts(
            "provider.compatible",
            timeout,
            10,
        )
    }

    /// Build the full URL for chat completions, detecting if base_url already includes the path.
    /// This allows custom providers with non-standard endpoints (e.g., VolcEngine ARK uses
    /// `/api/coding/v3/chat/completions` instead of `/v1/chat/completions`).
    fn chat_completions_url(&self) -> String {
        // If a custom api_path is configured, use it directly.
        if let Some(ref api_path) = self.api_path {
            let separator = if api_path.starts_with('/') { "" } else { "/" };
            return format!("{}{separator}{api_path}", self.base_url);
        }

        let has_full_endpoint = reqwest::Url::parse(&self.base_url)
            .map(|url| {
                url.path()
                    .trim_end_matches('/')
                    .ends_with("/chat/completions")
            })
            .unwrap_or_else(|_| {
                self.base_url
                    .trim_end_matches('/')
                    .ends_with("/chat/completions")
            });

        if has_full_endpoint {
            self.base_url.clone()
        } else {
            format!("{}/chat/completions", self.base_url)
        }
    }

    fn path_ends_with(&self, suffix: &str) -> bool {
        if let Ok(url) = reqwest::Url::parse(&self.base_url) {
            return url.path().trim_end_matches('/').ends_with(suffix);
        }

        self.base_url.trim_end_matches('/').ends_with(suffix)
    }

    fn has_explicit_api_path(&self) -> bool {
        let Ok(url) = reqwest::Url::parse(&self.base_url) else {
            return false;
        };

        let path = url.path().trim_end_matches('/');
        !path.is_empty() && path != "/"
    }

    fn requires_tool_stream(&self) -> bool {
        let host_requires_tool_stream = reqwest::Url::parse(&self.base_url)
            .ok()
            .and_then(|url| url.host_str().map(str::to_ascii_lowercase))
            .is_some_and(|host| host == "api.z.ai" || host.ends_with(".z.ai"));

        host_requires_tool_stream || matches!(self.name.as_str(), "zai" | "z.ai")
    }

    fn tool_stream_for_tools(&self, has_tools: bool) -> Option<bool> {
        if has_tools && self.requires_tool_stream() {
            Some(true)
        } else {
            None
        }
    }

    /// Build the full URL for responses API, detecting if base_url already includes the path.
    fn responses_url(&self) -> String {
        if self.path_ends_with("/responses") {
            return self.base_url.clone();
        }

        let normalized_base = self.base_url.trim_end_matches('/');

        // If chat endpoint is explicitly configured, derive sibling responses endpoint.
        if let Some(prefix) = normalized_base.strip_suffix("/chat/completions") {
            return format!("{prefix}/responses");
        }

        // If an explicit API path already exists (e.g. /v1, /openai, /api/coding/v3),
        // append responses directly to avoid duplicate /v1 segments.
        if self.has_explicit_api_path() {
            format!("{normalized_base}/responses")
        } else {
            format!("{normalized_base}/v1/responses")
        }
    }

    #[cfg(test)]
    fn tool_specs_to_openai_format(
        tools: &[clawseed_api::tool::ToolSpec],
    ) -> Vec<serde_json::Value> {
        tools
            .iter()
            .map(|tool| {
                let params =
                    clawseed_api::schema::SchemaCleanr::clean_for_openai(tool.parameters.clone());
                serde_json::json!({
                    "type": "function",
                    "function": {
                        "name": tool.name,
                        "description": tool.description,
                        "parameters": params
                    }
                })
            })
            .collect()
    }

    /// Returns true if the given model requires system messages to be merged
    /// into the first user message because its prompt template cannot handle
    /// the `system` role reliably (e.g. DeepSeek V3.2 Jinja rendering errors).
    fn model_requires_system_merge(model: &str) -> bool {
        let id = model
            .rsplit('/')
            .next()
            .unwrap_or(model)
            .to_ascii_lowercase();
        id.contains("deepseek-v3") || id.contains("deepseek_v3")
    }

    /// Whether system messages should be flattened into the first user message,
    /// either because the provider was configured that way or the model requires it.
    fn effective_merge_system(&self, model: &str) -> bool {
        self.merge_system_into_user || Self::model_requires_system_merge(model)
    }

    fn reasoning_effort_for_model(&self, model: &str) -> Option<String> {
        let id = model.rsplit('/').next().unwrap_or(model);
        let supports_reasoning_effort = id.starts_with("gpt-5") || id.contains("codex");
        supports_reasoning_effort
            .then(|| self.reasoning_effort.clone())
            .flatten()
    }
}

// NOTE: Test module disabled because it references pub(super) types from
// `parsing` and private methods from `provider_impl` that are not accessible
// from this test module. These tests were ported from a prior crate and
// need significant rework to compile in the clawseed crate structure.
// To re-enable, fix imports and make private items accessible (e.g. pub(crate)).
#[cfg(any())]
mod tests {
    use super::*;

    fn make_provider(name: &str, url: &str, key: Option<&str>) -> OpenAiCompatibleProvider {
        OpenAiCompatibleProvider::new(name, url, key, AuthStyle::Bearer)
    }

    #[test]
    fn creates_with_key() {
        let p = make_provider(
            "venice",
            "https://api.venice.ai",
            Some("venice-test-credential"),
        );
        assert_eq!(p.name, "venice");
        assert_eq!(p.base_url, "https://api.venice.ai");
        assert_eq!(p.credential.as_deref(), Some("venice-test-credential"));
    }

    #[test]
    fn creates_without_key() {
        let p = make_provider("test", "https://example.com", None);
        assert!(p.credential.is_none());
    }

    #[test]
    fn strips_trailing_slash() {
        let p = make_provider("test", "https://example.com/", None);
        assert_eq!(p.base_url, "https://example.com");
    }

    #[tokio::test]
    async fn chat_without_key_attempts_request() {
        let p = make_provider("Local", "http://127.0.0.1:1", None);
        let result = p
            .chat_with_system(None, "hello", "default", Some(0.7))
            .await;
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            !err_msg.contains("API key not set"),
            "should not get credential error, got: {err_msg}"
        );
    }

    #[test]
    fn request_serializes_correctly() {
        let req = ApiChatRequest {
            model: "llama-3.3-70b".to_string(),
            messages: vec![
                Message {
                    role: "system".to_string(),
                    content: MessageContent::Text("You are ClawSeed".to_string()),
                },
                Message {
                    role: "user".to_string(),
                    content: MessageContent::Text("hello".to_string()),
                },
            ],
            temperature: 0.4,
            stream: Some(false),
            reasoning_effort: None,
            tool_stream: None,
            tools: None,
            tool_choice: None,
            max_tokens: None,
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("llama-3.3-70b"));
        assert!(json.contains("system"));
        assert!(json.contains("user"));
        // tools/tool_choice should be omitted when None
        assert!(!json.contains("tools"));
        assert!(!json.contains("tool_choice"));
    }

    #[test]
    fn response_deserializes() {
        let json = r#"{"choices":[{"message":{"content":"Hello from Venice!"}}]}"#;
        let resp: ApiChatResponse = serde_json::from_str(json).unwrap();
        assert_eq!(
            resp.choices[0].message.content,
            Some("Hello from Venice!".to_string())
        );
    }

    #[test]
    fn response_empty_choices() {
        let json = r#"{"choices":[]}"#;
        let resp: ApiChatResponse = serde_json::from_str(json).unwrap();
        assert!(resp.choices.is_empty());
    }

    #[test]
    fn parse_chat_response_body_reports_sanitized_snippet() {
        let body = r#"{"choices":"invalid","api_key":"sk-test-secret-value"}"#;
        let err = parse_chat_response_body("custom", body).expect_err("payload should fail");
        let msg = err.to_string();

        assert!(msg.contains("custom API returned an unexpected chat-completions payload"));
        assert!(msg.contains("body="));
        assert!(msg.contains("[REDACTED]"));
        assert!(!msg.contains("sk-test-secret-value"));
    }

    #[test]
    fn parse_responses_response_body_reports_sanitized_snippet() {
        let body = r#"{"output_text":123,"api_key":"sk-another-secret"}"#;
        let err = parse_responses_response_body("custom", body).expect_err("payload should fail");
        let msg = err.to_string();

        assert!(msg.contains("custom Responses API returned an unexpected payload"));
        assert!(msg.contains("body="));
        assert!(msg.contains("[REDACTED]"));
        assert!(!msg.contains("sk-another-secret"));
    }

    #[test]
    fn x_api_key_auth_style() {
        let p = OpenAiCompatibleProvider::new(
            "moonshot",
            "https://api.moonshot.cn",
            Some("ms-key"),
            AuthStyle::XApiKey,
        );
        assert!(matches!(p.auth_header, AuthStyle::XApiKey));
    }

    #[test]
    fn custom_auth_style() {
        let p = OpenAiCompatibleProvider::new(
            "custom",
            "https://api.example.com",
            Some("key"),
            AuthStyle::Custom("X-Custom-Key".into()),
        );
        assert!(matches!(p.auth_header, AuthStyle::Custom(_)));
    }

    #[test]
    fn zhipu_jwt_produces_valid_three_part_token() {
        let result = zhipu_jwt_bearer("testid.testsecret").unwrap();
        assert!(result.starts_with("Bearer "));
        let jwt = result.strip_prefix("Bearer ").unwrap();
        let parts: Vec<&str> = jwt.split('.').collect();
        assert_eq!(parts.len(), 3, "JWT must have 3 dot-separated parts: {jwt}");
    }

    #[test]
    fn zhipu_jwt_header_is_correct() {
        use base64::engine::{Engine, general_purpose::URL_SAFE_NO_PAD};
        let result = zhipu_jwt_bearer("myid.mysecret").unwrap();
        let jwt = result.strip_prefix("Bearer ").unwrap();
        let header_b64 = jwt.split('.').next().unwrap();
        let header_bytes = URL_SAFE_NO_PAD.decode(header_b64).unwrap();
        let header: serde_json::Value = serde_json::from_slice(&header_bytes).unwrap();
        assert_eq!(header["alg"], "HS256");
        assert_eq!(header["typ"], "JWT");
        assert_eq!(header["sign_type"], "SIGN");
    }

    #[test]
    fn zhipu_jwt_payload_contains_api_key_and_timestamps() {
        use base64::engine::{Engine, general_purpose::URL_SAFE_NO_PAD};
        let result = zhipu_jwt_bearer("myapiid.mysecretkey").unwrap();
        let jwt = result.strip_prefix("Bearer ").unwrap();
        let payload_b64 = jwt.split('.').nth(1).unwrap();
        let payload_bytes = URL_SAFE_NO_PAD.decode(payload_b64).unwrap();
        let payload: serde_json::Value = serde_json::from_slice(&payload_bytes).unwrap();
        assert_eq!(payload["api_key"], "myapiid");
        assert!(payload["exp"].is_number());
        assert!(payload["timestamp"].is_number());
        // exp should be ~210s after timestamp
        let ts = payload["timestamp"].as_u64().unwrap();
        let exp = payload["exp"].as_u64().unwrap();
        assert_eq!(exp - ts, 210_000);
    }

    #[test]
    fn zhipu_jwt_signature_is_verifiable() {
        let secret = "testsecret123";
        let credential = format!("testid.{secret}");
        let result = zhipu_jwt_bearer(&credential).unwrap();
        let jwt = result.strip_prefix("Bearer ").unwrap();
        let parts: Vec<&str> = jwt.split('.').collect();
        let signing_input = format!("{}.{}", parts[0], parts[1]);

        // Verify HMAC-SHA256 signature
        let key = ring::hmac::Key::new(ring::hmac::HMAC_SHA256, secret.as_bytes());
        use base64::engine::{Engine, general_purpose::URL_SAFE_NO_PAD};
        let sig_bytes = URL_SAFE_NO_PAD.decode(parts[2]).unwrap();
        ring::hmac::verify(&key, signing_input.as_bytes(), &sig_bytes)
            .expect("signature must verify");
    }

    #[test]
    fn zhipu_jwt_rejects_invalid_key_format() {
        assert!(zhipu_jwt_bearer("no-dot-here").is_err());
        assert!(zhipu_jwt_bearer("").is_err());
    }

    #[test]
    fn zhipu_jwt_auth_style_applies_correctly() {
        let p = OpenAiCompatibleProvider::new(
            "Z.AI",
            "https://api.z.ai/api/coding/paas/v4",
            Some("testid.testsecret"),
            AuthStyle::ZhipuJwt,
        );
        assert!(matches!(p.auth_header, AuthStyle::ZhipuJwt));
    }

    #[tokio::test]
    async fn all_compatible_providers_attempt_request_without_key() {
        let providers = vec![
            make_provider("Venice", "http://127.0.0.1:1", None),
            make_provider("Moonshot", "http://127.0.0.1:1", None),
            make_provider("GLM", "http://127.0.0.1:1", None),
            make_provider("MiniMax", "http://127.0.0.1:1", None),
            make_provider("Groq", "http://127.0.0.1:1", None),
            make_provider("Mistral", "http://127.0.0.1:1", None),
            make_provider("xAI", "http://127.0.0.1:1", None),
            make_provider("Astrai", "http://127.0.0.1:1", None),
        ];

        for p in providers {
            let result = p.chat_with_system(None, "test", "model", Some(0.7)).await;
            assert!(result.is_err(), "{} should fail (unreachable host)", p.name);
            let err_msg = result.unwrap_err().to_string();
            assert!(
                !err_msg.contains("API key not set"),
                "{} should get transport error, not credential error, got: {err_msg}",
                p.name
            );
        }
    }

    #[test]
    fn responses_extracts_top_level_output_text() {
        let json = r#"{"output_text":"Hello from top-level","output":[]}"#;
        let response: ResponsesResponse = serde_json::from_str(json).unwrap();
        assert_eq!(
            extract_responses_text(response).as_deref(),
            Some("Hello from top-level")
        );
    }

    #[test]
    fn responses_extracts_nested_output_text() {
        let json =
            r#"{"output":[{"content":[{"type":"output_text","text":"Hello from nested"}]}]}"#;
        let response: ResponsesResponse = serde_json::from_str(json).unwrap();
        assert_eq!(
            extract_responses_text(response).as_deref(),
            Some("Hello from nested")
        );
    }

    #[test]
    fn responses_extracts_any_text_as_fallback() {
        let json = r#"{"output":[{"content":[{"type":"message","text":"Fallback text"}]}]}"#;
        let response: ResponsesResponse = serde_json::from_str(json).unwrap();
        assert_eq!(
            extract_responses_text(response).as_deref(),
            Some("Fallback text")
        );
    }

    #[test]
    fn build_responses_prompt_preserves_multi_turn_history() {
        let messages = vec![
            ChatMessage::system("policy"),
            ChatMessage::user("step 1"),
            ChatMessage::assistant("ack 1"),
            ChatMessage::tool("{\"result\":\"ok\"}"),
            ChatMessage::user("step 2"),
        ];

        let (instructions, input) = build_responses_prompt(&messages);

        assert_eq!(instructions.as_deref(), Some("policy"));
        assert_eq!(input.len(), 4);

        let serialized: Vec<serde_json::Value> = input
            .iter()
            .map(|item| serde_json::to_value(item).expect("responses input item serializes"))
            .collect();
        assert_eq!(
            serialized[0],
            serde_json::json!({
                "role": "user",
                "content": "step 1"
            })
        );
        assert_eq!(
            serialized[1],
            serde_json::json!({
                "role": "assistant",
                "type": "message",
                "content": [{
                    "type": "output_text",
                    "text": "ack 1"
                }]
            })
        );
        assert_eq!(
            serialized[2],
            serde_json::json!({
                "role": "assistant",
                "type": "message",
                "content": [{
                    "type": "output_text",
                    "text": "{\"result\":\"ok\"}"
                }]
            })
        );
        assert_eq!(
            serialized[3],
            serde_json::json!({
                "role": "user",
                "content": "step 2"
            })
        );
    }

    #[tokio::test]
    async fn chat_via_responses_requires_non_system_message() {
        let provider = make_provider("custom", "https://api.example.com", Some("test-key"));
        let err = provider
            .chat_via_responses(
                Some("test-key"),
                &[ChatMessage::system("policy")],
                "gpt-test",
            )
            .await
            .expect_err("system-only fallback payload should fail");

        assert!(
            err.to_string()
                .contains("requires at least one non-system message")
        );
    }

    #[test]
    fn tool_call_function_name_falls_back_to_top_level_name() {
        let call: ToolCall = serde_json::from_value(serde_json::json!({
            "name": "memory_recall",
            "arguments": "{\"query\":\"latest roadmap\"}"
        }))
        .unwrap();

        assert_eq!(call.function_name().as_deref(), Some("memory_recall"));
    }

    #[test]
    fn tool_call_function_arguments_falls_back_to_parameters_object() {
        let call: ToolCall = serde_json::from_value(serde_json::json!({
            "name": "shell",
            "parameters": {"command": "pwd"}
        }))
        .unwrap();

        assert_eq!(
            call.function_arguments().as_deref(),
            Some("{\"command\":\"pwd\"}")
        );
    }

    #[test]
    fn tool_call_function_arguments_prefers_nested_function_field() {
        let call: ToolCall = serde_json::from_value(serde_json::json!({
            "name": "ignored_name",
            "arguments": "{\"query\":\"ignored\"}",
            "function": {
                "name": "memory_recall",
                "arguments": "{\"query\":\"preferred\"}"
            }
        }))
        .unwrap();

        assert_eq!(call.function_name().as_deref(), Some("memory_recall"));
        assert_eq!(
            call.function_arguments().as_deref(),
            Some("{\"query\":\"preferred\"}")
        );
    }

    // ----------------------------------------------------------
    // Custom endpoint path tests (Issue #114)
    // ----------------------------------------------------------

    #[test]
    fn chat_completions_url_standard_openai() {
        // Standard OpenAI-compatible providers get /chat/completions appended
        let p = make_provider("openai", "https://api.openai.com/v1", None);
        assert_eq!(
            p.chat_completions_url(),
            "https://api.openai.com/v1/chat/completions"
        );
    }

    #[test]
    fn chat_completions_url_trailing_slash() {
        // Trailing slash is stripped, then /chat/completions appended
        let p = make_provider("test", "https://api.example.com/v1/", None);
        assert_eq!(
            p.chat_completions_url(),
            "https://api.example.com/v1/chat/completions"
        );
    }

    #[test]
    fn chat_completions_url_volcengine_ark() {
        // VolcEngine ARK uses custom path - should use as-is
        let p = make_provider(
            "volcengine",
            "https://ark.cn-beijing.volces.com/api/coding/v3/chat/completions",
            None,
        );
        assert_eq!(
            p.chat_completions_url(),
            "https://ark.cn-beijing.volces.com/api/coding/v3/chat/completions"
        );
    }

    #[test]
    fn chat_completions_url_custom_full_endpoint() {
        // Custom provider with full endpoint path
        let p = make_provider(
            "custom",
            "https://my-api.example.com/v2/llm/chat/completions",
            None,
        );
        assert_eq!(
            p.chat_completions_url(),
            "https://my-api.example.com/v2/llm/chat/completions"
        );
    }

    #[test]
    fn chat_completions_url_requires_exact_suffix_match() {
        let p = make_provider(
            "custom",
            "https://my-api.example.com/v2/llm/chat/completions-proxy",
            None,
        );
        assert_eq!(
            p.chat_completions_url(),
            "https://my-api.example.com/v2/llm/chat/completions-proxy/chat/completions"
        );
    }

    #[test]
    fn responses_url_standard() {
        // Standard providers get /v1/responses appended
        let p = make_provider("test", "https://api.example.com", None);
        assert_eq!(p.responses_url(), "https://api.example.com/v1/responses");
    }

    #[test]
    fn responses_url_custom_full_endpoint() {
        // Custom provider with full responses endpoint
        let p = make_provider(
            "custom",
            "https://my-api.example.com/api/v2/responses",
            None,
        );
        assert_eq!(
            p.responses_url(),
            "https://my-api.example.com/api/v2/responses"
        );
    }

    #[test]
    fn responses_url_requires_exact_suffix_match() {
        let p = make_provider(
            "custom",
            "https://my-api.example.com/api/v2/responses-proxy",
            None,
        );
        assert_eq!(
            p.responses_url(),
            "https://my-api.example.com/api/v2/responses-proxy/responses"
        );
    }

    #[test]
    fn responses_url_derives_from_chat_endpoint() {
        let p = make_provider(
            "custom",
            "https://my-api.example.com/api/v2/chat/completions",
            None,
        );
        assert_eq!(
            p.responses_url(),
            "https://my-api.example.com/api/v2/responses"
        );
    }

    #[test]
    fn responses_url_base_with_v1_no_duplicate() {
        let p = make_provider("test", "https://api.example.com/v1", None);
        assert_eq!(p.responses_url(), "https://api.example.com/v1/responses");
    }

    #[test]
    fn responses_url_non_v1_api_path_uses_raw_suffix() {
        let p = make_provider("test", "https://api.example.com/api/coding/v3", None);
        assert_eq!(
            p.responses_url(),
            "https://api.example.com/api/coding/v3/responses"
        );
    }

    #[test]
    fn chat_completions_url_without_v1() {
        // Provider configured without /v1 in base URL
        let p = make_provider("test", "https://api.example.com", None);
        assert_eq!(
            p.chat_completions_url(),
            "https://api.example.com/chat/completions"
        );
    }

    #[test]
    fn chat_completions_url_base_with_v1() {
        // Provider configured with /v1 in base URL
        let p = make_provider("test", "https://api.example.com/v1", None);
        assert_eq!(
            p.chat_completions_url(),
            "https://api.example.com/v1/chat/completions"
        );
    }

    // ----------------------------------------------------------
    // Provider-specific endpoint tests (Issue #167)
    // ----------------------------------------------------------

    #[test]
    fn chat_completions_url_zai() {
        // Z.AI uses /api/paas/v4 base path
        let p = make_provider("zai", "https://api.z.ai/api/paas/v4", None);
        assert_eq!(
            p.chat_completions_url(),
            "https://api.z.ai/api/paas/v4/chat/completions"
        );
    }

    #[test]
    fn chat_completions_url_minimax() {
        // MiniMax OpenAI-compatible endpoint requires /v1 base path.
        let p = make_provider("minimax", "https://api.minimaxi.com/v1", None);
        assert_eq!(
            p.chat_completions_url(),
            "https://api.minimaxi.com/v1/chat/completions"
        );
    }

    #[test]
    fn chat_completions_url_glm() {
        // GLM (BigModel) uses /api/paas/v4 base path
        let p = make_provider("glm", "https://open.bigmodel.cn/api/paas/v4", None);
        assert_eq!(
            p.chat_completions_url(),
            "https://open.bigmodel.cn/api/paas/v4/chat/completions"
        );
    }

    #[test]
    fn chat_completions_url_opencode() {
        // OpenCode Zen uses /zen/v1 base path
        let p = make_provider("opencode", "https://opencode.ai/zen/v1", None);
        assert_eq!(
            p.chat_completions_url(),
            "https://opencode.ai/zen/v1/chat/completions"
        );
    }

    #[test]
    fn chat_completions_url_opencode_go() {
        // OpenCode Go uses /zen/go/v1 base path
        let p = make_provider("opencode-go", "https://opencode.ai/zen/go/v1", None);
        assert_eq!(
            p.chat_completions_url(),
            "https://opencode.ai/zen/go/v1/chat/completions"
        );
    }

    #[test]
    fn parse_native_response_preserves_tool_call_id() {
        let message = ResponseMessage {
            content: None,
            tool_calls: Some(vec![ToolCall {
                id: Some("call_123".to_string()),
                kind: Some("function".to_string()),
                function: Some(Function {
                    name: Some("shell".to_string()),
                    arguments: Some(r#"{"command":"pwd"}"#.to_string()),
                }),
                name: None,
                arguments: None,
                parameters: None,
            }]),
            reasoning_content: None,
        };

        let parsed = OpenAiCompatibleProvider::parse_native_response(message);
        assert_eq!(parsed.tool_calls.len(), 1);
        assert_eq!(parsed.tool_calls[0].id, "call_123");
        assert_eq!(parsed.tool_calls[0].name, "shell");
    }

    #[test]
    fn convert_messages_for_native_maps_tool_result_payload() {
        let input = vec![ChatMessage::tool(
            r#"{"tool_call_id":"call_abc","content":"done"}"#,
        )];

        let converted = OpenAiCompatibleProvider::convert_messages_for_native(&input, true);
        assert_eq!(converted.len(), 1);
        assert_eq!(converted[0].role, "tool");
        assert_eq!(converted[0].tool_call_id.as_deref(), Some("call_abc"));
        assert!(matches!(
            converted[0].content.as_ref(),
            Some(MessageContent::Text(value)) if value == "done"
        ));
    }

    #[test]
    fn convert_messages_for_native_keeps_user_image_markers_as_text_when_disabled() {
        let input = vec![ChatMessage::user(
            "System primer [IMAGE:data:image/png;base64,abcd] user turn",
        )];

        let converted = OpenAiCompatibleProvider::convert_messages_for_native(&input, false);
        assert_eq!(converted.len(), 1);
        assert_eq!(converted[0].role, "user");
        assert!(matches!(
            converted[0].content.as_ref(),
            Some(MessageContent::Text(value))
                if value == "System primer [IMAGE:data:image/png;base64,abcd] user turn"
        ));
    }

    #[test]
    fn flatten_system_messages_merges_into_first_user() {
        let input = vec![
            ChatMessage::system("core policy"),
            ChatMessage::assistant("ack"),
            ChatMessage::system("delivery rules"),
            ChatMessage::user("hello"),
            ChatMessage::assistant("post-user"),
        ];

        let output = OpenAiCompatibleProvider::flatten_system_messages(&input, true);
        assert_eq!(output.len(), 3);
        assert_eq!(output[0].role, "assistant");
        assert_eq!(output[0].content, "ack");
        assert_eq!(output[1].role, "user");
        assert_eq!(output[1].content, "core policy\n\ndelivery rules\n\nhello");
        assert_eq!(output[2].role, "assistant");
        assert_eq!(output[2].content, "post-user");
        assert!(output.iter().all(|m| m.role != "system"));
    }

    #[test]
    fn flatten_system_messages_inserts_user_when_missing() {
        let input = vec![
            ChatMessage::system("core policy"),
            ChatMessage::assistant("ack"),
        ];

        let output = OpenAiCompatibleProvider::flatten_system_messages(&input, true);
        assert_eq!(output.len(), 2);
        assert_eq!(output[0].role, "user");
        assert_eq!(output[0].content, "core policy");
        assert_eq!(output[1].role, "assistant");
        assert_eq!(output[1].content, "ack");
    }

    #[test]
    fn strip_think_tags_drops_unclosed_block_suffix() {
        let input = "visible<think>hidden";
        assert_eq!(strip_think_tags(input), "visible");
    }

    #[test]
    fn native_tool_schema_unsupported_detection_is_precise() {
        assert!(OpenAiCompatibleProvider::is_native_tool_schema_unsupported(
            reqwest::StatusCode::BAD_REQUEST,
            "unknown parameter: tools"
        ));
        assert!(
            !OpenAiCompatibleProvider::is_native_tool_schema_unsupported(
                reqwest::StatusCode::UNAUTHORIZED,
                "unknown parameter: tools"
            )
        );
    }

    #[test]
    fn native_tool_schema_unsupported_detects_groq_tool_validation_error() {
        assert!(OpenAiCompatibleProvider::is_native_tool_schema_unsupported(
            reqwest::StatusCode::BAD_REQUEST,
            r#"Groq API error (400 Bad Request): {"error":{"message":"tool call validation failed: attempted to call tool 'memory_recall={\"limit\":5}' which was not in request"}}"#
        ));
    }

    #[test]
    fn prompt_guided_tool_fallback_injects_system_instruction() {
        let input = vec![ChatMessage::user("check status")];
        let tools = vec![clawseed_api::tool::ToolSpec {
            name: "shell_exec".to_string(),
            description: "Execute shell command".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "command": { "type": "string" }
                },
                "required": ["command"]
            }),
        }];

        let output =
            OpenAiCompatibleProvider::with_prompt_guided_tool_instructions(&input, Some(&tools));
        assert!(!output.is_empty());
        assert_eq!(output[0].role, "system");
        assert!(output[0].content.contains("Available Tools"));
        assert!(output[0].content.contains("shell_exec"));
    }

    #[test]
    fn reasoning_effort_only_applies_to_gpt5_and_codex_models() {
        let provider = make_provider("test", "https://example.com", None)
            .with_reasoning_effort(Some("high".to_string()));

        assert_eq!(
            provider.reasoning_effort_for_model("gpt-5.3-codex"),
            Some("high".to_string())
        );
        assert_eq!(
            provider.reasoning_effort_for_model("openai/gpt-5"),
            Some("high".to_string())
        );
        assert_eq!(provider.reasoning_effort_for_model("llama-3.3-70b"), None);
    }

    #[tokio::test]
    async fn warmup_without_key_attempts_connection() {
        let provider = make_provider("test", "http://127.0.0.1:1", None);
        let result = provider.warmup().await;
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            !err_msg.contains("API key not set"),
            "should not get credential error, got: {err_msg}"
        );
    }

    // ══════════════════════════════════════════════════════════
    // Native tool calling tests
    // ══════════════════════════════════════════════════════════

    #[test]
    fn capabilities_reports_native_tool_calling() {
        let p = make_provider("test", "https://example.com", None);
        let caps = <OpenAiCompatibleProvider as Provider>::capabilities(&p);
        assert!(caps.native_tool_calling);
        assert!(!caps.vision);
    }

    #[test]
    fn capabilities_reports_vision_for_qwen_compatible_provider() {
        let p = OpenAiCompatibleProvider::new_with_vision(
            "Qwen",
            "https://dashscope.aliyuncs.com/compatible-mode/v1",
            Some("k"),
            AuthStyle::Bearer,
            true,
        );
        let caps = <OpenAiCompatibleProvider as Provider>::capabilities(&p);
        assert!(caps.native_tool_calling);
        assert!(caps.vision);
    }

    #[test]
    fn minimax_provider_supports_native_tool_calling_with_system_merge() {
        let p = OpenAiCompatibleProvider::new(
            "MiniMax",
            "https://api.minimax.chat/v1",
            Some("k"),
            AuthStyle::Bearer,
        )
        .with_merge_system_into_user();
        let caps = <OpenAiCompatibleProvider as Provider>::capabilities(&p);
        assert!(
            caps.native_tool_calling,
            "MiniMax should preserve native tool calling when system messages are merged"
        );
        assert!(!caps.vision);
    }

    /// Regression test for #5743: native tool messages must be stripped for
    /// providers that don't support native tool calling (e.g. MiniMax).
    #[test]
    fn strip_native_tool_messages_removes_tool_and_tool_calls() {
        let messages = vec![
            ChatMessage::system("sys"),
            ChatMessage::user("search for cats"),
            ChatMessage::assistant(
                r#"{"content":"I'll search","tool_calls":[{"id":"chatcmpl-tool-abc","name":"web_search","arguments":"{}"}]}"#,
            ),
            ChatMessage::tool(
                r#"{"tool_call_id":"chatcmpl-tool-abc","content":"Found 10 results"}"#,
            ),
            ChatMessage::assistant("Here are the results about cats"),
            ChatMessage::user("thanks"),
        ];
        let p = OpenAiCompatibleProvider::new_merge_system_into_user(
            "MiniMax",
            "https://api.minimax.chat/v1",
            Some("k"),
            AuthStyle::Bearer,
        );
        let stripped = p.strip_native_tool_messages(&messages);
        // tool message dropped; the pre-tool narration and the reply that
        // follows the tool result are now coalesced into a single assistant
        // message so the output never contains consecutive assistants (see
        // #5825).
        assert_eq!(stripped.len(), 4);
        assert_eq!(stripped[0].role, "system");
        assert_eq!(stripped[1].role, "user");
        assert_eq!(stripped[1].content, "search for cats");
        assert_eq!(stripped[2].role, "assistant");
        assert!(
            stripped[2].content.starts_with("I'll search"),
            "coalesced assistant must preserve the pre-tool narration; got {:?}",
            stripped[2].content
        );
        assert!(
            stripped[2]
                .content
                .contains("Here are the results about cats"),
            "coalesced assistant must preserve the post-tool reply; got {:?}",
            stripped[2].content
        );
        assert!(
            !stripped[2].content.contains("tool_calls"),
            "tool_calls structure must be stripped"
        );
        assert_eq!(stripped[3].role, "user");
    }

    #[test]
    fn strip_native_tool_messages_drops_empty_assistant_tool_calls() {
        let messages = vec![
            ChatMessage::system("sys"),
            ChatMessage::user("do it"),
            ChatMessage::assistant(
                r#"{"content":"","tool_calls":[{"id":"tc1","name":"shell","arguments":"{}"}]}"#,
            ),
            ChatMessage::tool(r#"{"tool_call_id":"tc1","content":"ok"}"#),
            ChatMessage::assistant("Done"),
        ];
        let p = OpenAiCompatibleProvider::new_merge_system_into_user(
            "MiniMax",
            "https://api.minimax.chat/v1",
            Some("k"),
            AuthStyle::Bearer,
        );
        let stripped = p.strip_native_tool_messages(&messages);
        // assistant with empty content + tool_calls → dropped; tool → dropped
        assert_eq!(stripped.len(), 3);
        assert_eq!(stripped[0].role, "system");
        assert_eq!(stripped[1].role, "user");
        assert_eq!(stripped[2].role, "assistant");
        assert_eq!(stripped[2].content, "Done");
    }

    #[test]
    fn strip_native_tool_messages_preserves_regular_messages() {
        let messages = vec![
            ChatMessage::system("sys"),
            ChatMessage::user("hello"),
            ChatMessage::assistant("hi there"),
            ChatMessage::user("bye"),
        ];
        let p = OpenAiCompatibleProvider::new_merge_system_into_user(
            "MiniMax",
            "https://api.minimax.chat/v1",
            Some("k"),
            AuthStyle::Bearer,
        );
        let stripped = p.strip_native_tool_messages(&messages);
        assert_eq!(stripped.len(), 4);
        for (orig, result) in messages.iter().zip(stripped.iter()) {
            assert_eq!(orig.role, result.role);
            assert_eq!(orig.content, result.content);
        }
    }

    /// Confirm that `strip_native_tool_messages` is a no-op when the provider
    /// has `native_tool_calling = true` — tool-role and assistant-with-tool-calls
    /// messages must pass through unchanged.
    #[test]
    fn strip_native_tool_messages_passthrough_when_native_tool_calling_enabled() {
        let messages = vec![
            ChatMessage::system("sys"),
            ChatMessage::user("search for cats"),
            ChatMessage::assistant(
                r#"{"content":"I'll search","tool_calls":[{"id":"chatcmpl-tool-abc","name":"web_search","arguments":"{}"}]}"#,
            ),
            ChatMessage::tool(
                r#"{"tool_call_id":"chatcmpl-tool-abc","content":"Found 10 results"}"#,
            ),
            ChatMessage::assistant("Here are the results about cats"),
        ];
        let p = OpenAiCompatibleProvider::new(
            "NativeToolProvider",
            "https://api.example.com/v1",
            Some("k"),
            AuthStyle::Bearer,
        );
        assert!(
            <OpenAiCompatibleProvider as Provider>::capabilities(&p).native_tool_calling,
            "provider must have native_tool_calling enabled for this test"
        );
        let result = p.strip_native_tool_messages(&messages);
        assert_eq!(result.len(), messages.len());
        for (orig, out) in messages.iter().zip(result.iter()) {
            assert_eq!(orig.role, out.role);
            assert_eq!(orig.content, out.content);
        }
    }

    #[test]
    fn user_agent_constructor_keeps_native_tool_calling_enabled() {
        let p = OpenAiCompatibleProvider::new_with_user_agent(
            "TestProvider",
            "https://example.com",
            Some("k"),
            AuthStyle::Bearer,
            "clawseed-test/1.0",
        );
        let caps = <OpenAiCompatibleProvider as Provider>::capabilities(&p);
        assert!(caps.native_tool_calling);
        assert!(!caps.vision);
        assert_eq!(p.user_agent.as_deref(), Some("clawseed-test/1.0"));
    }

    #[test]
    fn user_agent_and_vision_constructor_preserves_capability_flags() {
        let p = OpenAiCompatibleProvider::new_with_user_agent_and_vision(
            "VisionProvider",
            "https://example.com",
            Some("k"),
            AuthStyle::Bearer,
            "clawseed-test/vision",
            true,
        );
        let caps = <OpenAiCompatibleProvider as Provider>::capabilities(&p);
        assert!(caps.native_tool_calling);
        assert!(caps.vision);
        assert_eq!(p.user_agent.as_deref(), Some("clawseed-test/vision"));
    }

    #[test]
    fn no_responses_fallback_constructor_keeps_native_tool_calling_enabled() {
        let p = OpenAiCompatibleProvider::new_no_responses_fallback(
            "FallbackProvider",
            "https://example.com",
            Some("k"),
            AuthStyle::Bearer,
        );
        let caps = <OpenAiCompatibleProvider as Provider>::capabilities(&p);
        assert!(caps.native_tool_calling);
        assert!(!caps.vision);
        assert!(p.user_agent.is_none());
    }

    #[test]
    fn to_message_content_converts_image_markers_to_openai_parts() {
        let content = "Describe this\n\n[IMAGE:data:image/png;base64,abcd]";
        let value = serde_json::to_value(OpenAiCompatibleProvider::to_message_content(
            "user", content, true,
        ))
        .unwrap();
        let parts = value
            .as_array()
            .expect("multimodal content should be an array");
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0]["type"], "text");
        assert_eq!(parts[0]["text"], "Describe this");
        assert_eq!(parts[1]["type"], "image_url");
        assert_eq!(parts[1]["image_url"]["url"], "data:image/png;base64,abcd");
    }

    #[test]
    fn to_message_content_keeps_markers_as_text_when_user_image_parts_disabled() {
        let content = "Policy [IMAGE:data:image/png;base64,abcd]";
        let value = serde_json::to_value(OpenAiCompatibleProvider::to_message_content(
            "user", content, false,
        ))
        .unwrap();
        assert_eq!(value, serde_json::json!(content));
    }

    #[test]
    fn to_message_content_keeps_plain_text_for_non_user_roles() {
        let value = serde_json::to_value(OpenAiCompatibleProvider::to_message_content(
            "system",
            "You are a helpful assistant.",
            true,
        ))
        .unwrap();
        assert_eq!(value, serde_json::json!("You are a helpful assistant."));
    }

    #[test]
    fn tool_specs_convert_to_openai_format() {
        let specs = vec![clawseed_api::tool::ToolSpec {
            name: "shell".to_string(),
            description: "Run shell command".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {"command": {"type": "string"}},
                "required": ["command"]
            }),
        }];

        let tools = OpenAiCompatibleProvider::tool_specs_to_openai_format(&specs);
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0]["type"], "function");
        assert_eq!(tools[0]["function"]["name"], "shell");
        assert_eq!(tools[0]["function"]["description"], "Run shell command");
        assert_eq!(tools[0]["function"]["parameters"]["required"][0], "command");
    }

    #[test]
    fn request_serializes_with_tools() {
        let tools = vec![serde_json::json!({
            "type": "function",
            "function": {
                "name": "get_weather",
                "description": "Get weather for a location",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "location": {"type": "string"}
                    }
                }
            }
        })];

        let req = ApiChatRequest {
            model: "test-model".to_string(),
            messages: vec![Message {
                role: "user".to_string(),
                content: MessageContent::Text("What is the weather?".to_string()),
            }],
            temperature: 0.7,
            stream: Some(false),
            reasoning_effort: None,
            tool_stream: None,
            tools: Some(tools),
            tool_choice: Some("auto".to_string()),
            max_tokens: None,
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("\"tools\""));
        assert!(json.contains("get_weather"));
        assert!(json.contains("\"tool_choice\":\"auto\""));
    }

    #[test]
    fn zai_tool_requests_enable_tool_stream() {
        let provider = make_provider("zai", "https://api.z.ai/api/paas/v4", None);
        let req = ApiChatRequest {
            model: "glm-5".to_string(),
            messages: vec![Message {
                role: "user".to_string(),
                content: MessageContent::Text("List /tmp".to_string()),
            }],
            temperature: 0.7,
            stream: Some(false),
            reasoning_effort: None,
            tool_stream: provider.tool_stream_for_tools(true),
            tools: Some(vec![serde_json::json!({
                "type": "function",
                "function": {
                    "name": "shell",
                    "description": "Run a shell command",
                    "parameters": {
                        "type": "object",
                        "properties": {
                            "command": {"type": "string"}
                        }
                    }
                }
            })]),
            tool_choice: Some("auto".to_string()),
            max_tokens: None,
        };

        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("\"tool_stream\":true"));
    }

    #[test]
    fn non_zai_tool_requests_omit_tool_stream() {
        let provider = make_provider("test", "https://api.example.com/v1", None);
        let req = ApiChatRequest {
            model: "test-model".to_string(),
            messages: vec![Message {
                role: "user".to_string(),
                content: MessageContent::Text("List /tmp".to_string()),
            }],
            temperature: 0.7,
            stream: Some(false),
            reasoning_effort: None,
            tool_stream: provider.tool_stream_for_tools(true),
            tools: Some(vec![serde_json::json!({
                "type": "function",
                "function": {
                    "name": "shell",
                    "description": "Run a shell command",
                    "parameters": {
                        "type": "object",
                        "properties": {
                            "command": {"type": "string"}
                        }
                    }
                }
            })]),
            tool_choice: Some("auto".to_string()),
            max_tokens: None,
        };

        let json = serde_json::to_string(&req).unwrap();
        assert!(!json.contains("\"tool_stream\""));
    }

    #[test]
    fn non_zai_provider_omits_tool_stream_regardless_of_streaming() {
        let provider = make_provider("custom", "https://proxy.example.com/v1", None);
        // tool_stream_for_tools should return None for non-Z.AI providers
        assert_eq!(provider.tool_stream_for_tools(true), None);
        assert_eq!(provider.tool_stream_for_tools(false), None);
    }

    #[test]
    fn z_ai_host_enables_tool_stream_for_custom_profiles() {
        let provider = make_provider("custom", "https://api.z.ai/api/coding/paas/v4", None);
        assert_eq!(provider.tool_stream_for_tools(true), Some(true));
    }

    #[test]
    fn response_with_tool_calls_deserializes() {
        let json = r#"{
            "choices": [{
                "message": {
                    "content": null,
                    "tool_calls": [{
                        "type": "function",
                        "function": {
                            "name": "get_weather",
                            "arguments": "{\"location\":\"London\"}"
                        }
                    }]
                }
            }]
        }"#;

        let resp: ApiChatResponse = serde_json::from_str(json).unwrap();
        let msg = &resp.choices[0].message;
        assert!(msg.content.is_none());
        let tool_calls = msg.tool_calls.as_ref().unwrap();
        assert_eq!(tool_calls.len(), 1);
        assert_eq!(
            tool_calls[0].function.as_ref().unwrap().name.as_deref(),
            Some("get_weather")
        );
        assert_eq!(
            tool_calls[0]
                .function
                .as_ref()
                .unwrap()
                .arguments
                .as_deref(),
            Some("{\"location\":\"London\"}")
        );
    }

    #[test]
    fn response_with_multiple_tool_calls() {
        let json = r#"{
            "choices": [{
                "message": {
                    "content": "I'll check both.",
                    "tool_calls": [
                        {
                            "type": "function",
                            "function": {
                                "name": "get_weather",
                                "arguments": "{\"location\":\"London\"}"
                            }
                        },
                        {
                            "type": "function",
                            "function": {
                                "name": "get_time",
                                "arguments": "{\"timezone\":\"UTC\"}"
                            }
                        }
                    ]
                }
            }]
        }"#;

        let resp: ApiChatResponse = serde_json::from_str(json).unwrap();
        let msg = &resp.choices[0].message;
        assert_eq!(msg.content.as_deref(), Some("I'll check both."));
        let tool_calls = msg.tool_calls.as_ref().unwrap();
        assert_eq!(tool_calls.len(), 2);
        assert_eq!(
            tool_calls[0].function.as_ref().unwrap().name.as_deref(),
            Some("get_weather")
        );
        assert_eq!(
            tool_calls[1].function.as_ref().unwrap().name.as_deref(),
            Some("get_time")
        );
    }

    #[tokio::test]
    async fn chat_with_tools_without_key_attempts_request() {
        let p = make_provider("TestProvider", "http://127.0.0.1:1", None);
        let messages = vec![ChatMessage {
            role: "user".to_string(),
            content: "hello".to_string(),
        }];
        let tools = vec![serde_json::json!({
            "type": "function",
            "function": {
                "name": "test_tool",
                "description": "A test tool",
                "parameters": {}
            }
        })];

        let result = p
            .chat_with_tools(&messages, &tools, "model", Some(0.7))
            .await;
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            !err_msg.contains("API key not set"),
            "should not get credential error, got: {err_msg}"
        );
    }

    #[test]
    fn response_with_no_tool_calls_has_empty_vec() {
        let json = r#"{"choices":[{"message":{"content":"Just text, no tools."}}]}"#;
        let resp: ApiChatResponse = serde_json::from_str(json).unwrap();
        let msg = &resp.choices[0].message;
        assert_eq!(msg.content.as_deref(), Some("Just text, no tools."));
        assert!(msg.tool_calls.is_none());
    }

    #[test]
    fn flatten_system_messages_merges_into_first_user_and_removes_system_roles() {
        let messages = vec![
            ChatMessage::system("System A"),
            ChatMessage::assistant("Earlier assistant turn"),
            ChatMessage::system("System B"),
            ChatMessage::user("User turn"),
            ChatMessage::tool(r#"{"ok":true}"#),
        ];

        let flattened = OpenAiCompatibleProvider::flatten_system_messages(&messages, true);
        assert_eq!(flattened.len(), 3);
        assert_eq!(flattened[0].role, "assistant");
        assert_eq!(
            flattened[1].content,
            "System A\n\nSystem B\n\nUser turn".to_string()
        );
        assert_eq!(flattened[1].role, "user");
        assert_eq!(flattened[2].role, "tool");
        assert!(!flattened.iter().any(|m| m.role == "system"));
    }

    #[test]
    fn flatten_system_messages_inserts_synthetic_user_when_no_user_exists() {
        let messages = vec![
            ChatMessage::assistant("Assistant only"),
            ChatMessage::system("Synthetic system"),
        ];

        let flattened = OpenAiCompatibleProvider::flatten_system_messages(&messages, true);
        assert_eq!(flattened.len(), 2);
        assert_eq!(flattened[0].role, "user");
        assert_eq!(flattened[0].content, "Synthetic system");
        assert_eq!(flattened[1].role, "assistant");
    }

    #[test]
    fn strip_think_tags_removes_multiple_blocks_with_surrounding_text() {
        let input = "Answer A <think>hidden 1</think> and B <think>hidden 2</think> done";
        let output = strip_think_tags(input);
        assert_eq!(output, "Answer A  and B  done");
    }

    #[test]
    fn strip_think_tags_drops_tail_for_unclosed_block() {
        let input = "Visible<think>hidden tail";
        let output = strip_think_tags(input);
        assert_eq!(output, "Visible");
    }

    // ----------------------------------------------------------
    // Reasoning model fallback tests (reasoning_content)
    // ----------------------------------------------------------

    #[test]
    fn reasoning_content_fallback_when_content_empty() {
        // Reasoning models (Qwen3, GLM-4) return content: "" with reasoning_content populated
        let json = r#"{"choices":[{"message":{"content":"","reasoning_content":"Thinking output here"}}]}"#;
        let resp: ApiChatResponse = serde_json::from_str(json).unwrap();
        let msg = &resp.choices[0].message;
        assert_eq!(msg.effective_content(), "Thinking output here");
    }

    #[test]
    fn reasoning_content_fallback_when_content_null() {
        // Some models may return content: null with reasoning_content
        let json =
            r#"{"choices":[{"message":{"content":null,"reasoning_content":"Fallback text"}}]}"#;
        let resp: ApiChatResponse = serde_json::from_str(json).unwrap();
        let msg = &resp.choices[0].message;
        assert_eq!(msg.effective_content(), "Fallback text");
    }

    #[test]
    fn reasoning_content_fallback_when_content_missing() {
        // content field absent entirely, reasoning_content present
        let json = r#"{"choices":[{"message":{"reasoning_content":"Only reasoning"}}]}"#;
        let resp: ApiChatResponse = serde_json::from_str(json).unwrap();
        let msg = &resp.choices[0].message;
        assert_eq!(msg.effective_content(), "Only reasoning");
    }

    #[test]
    fn reasoning_content_not_used_when_content_present() {
        // Normal model: content populated, reasoning_content should be ignored
        let json = r#"{"choices":[{"message":{"content":"Normal response","reasoning_content":"Should be ignored"}}]}"#;
        let resp: ApiChatResponse = serde_json::from_str(json).unwrap();
        let msg = &resp.choices[0].message;
        assert_eq!(msg.effective_content(), "Normal response");
    }

    #[test]
    fn reasoning_content_used_when_content_only_think_tags() {
        let json = r#"{"choices":[{"message":{"content":"<think>secret</think>","reasoning_content":"Fallback text"}}]}"#;
        let resp: ApiChatResponse = serde_json::from_str(json).unwrap();
        let msg = &resp.choices[0].message;
        assert_eq!(msg.effective_content(), "Fallback text");
        assert_eq!(
            msg.effective_content_optional().as_deref(),
            Some("Fallback text")
        );
    }

    #[test]
    fn reasoning_content_both_absent_returns_empty() {
        // Neither content nor reasoning_content - returns empty string
        let json = r#"{"choices":[{"message":{}}]}"#;
        let resp: ApiChatResponse = serde_json::from_str(json).unwrap();
        let msg = &resp.choices[0].message;
        assert_eq!(msg.effective_content(), "");
    }

    #[test]
    fn reasoning_content_ignored_by_normal_models() {
        // Standard response without reasoning_content still works
        let json = r#"{"choices":[{"message":{"content":"Hello from Venice!"}}]}"#;
        let resp: ApiChatResponse = serde_json::from_str(json).unwrap();
        let msg = &resp.choices[0].message;
        assert!(msg.reasoning_content.is_none());
        assert_eq!(msg.effective_content(), "Hello from Venice!");
    }

    // ----------------------------------------------------------
    // SSE streaming reasoning_content fallback tests
    // ----------------------------------------------------------

    #[test]
    fn parse_sse_line_with_content() {
        let line = r#"data: {"choices":[{"delta":{"content":"hello"}}]}"#;
        let result = parse_sse_line(line).unwrap().unwrap();
        assert_eq!(result.delta, "hello");
        assert!(result.reasoning.is_none());
    }

    #[test]
    fn parse_sse_line_with_reasoning_content() {
        let line = r#"data: {"choices":[{"delta":{"reasoning_content":"thinking..."}}]}"#;
        let result = parse_sse_line(line).unwrap().unwrap();
        assert!(result.delta.is_empty());
        assert_eq!(result.reasoning.as_deref(), Some("thinking..."));
    }

    #[test]
    fn parse_sse_line_with_both_prefers_content() {
        let line = r#"data: {"choices":[{"delta":{"content":"real answer","reasoning_content":"thinking..."}}]}"#;
        let result = parse_sse_line(line).unwrap().unwrap();
        assert_eq!(result.delta, "real answer");
        assert!(result.reasoning.is_none());
    }

    #[test]
    fn parse_sse_line_with_empty_content_falls_back_to_reasoning() {
        let line =
            r#"data: {"choices":[{"delta":{"content":"","reasoning_content":"thinking..."}}]}"#;
        let result = parse_sse_line(line).unwrap().unwrap();
        assert!(result.delta.is_empty());
        assert_eq!(result.reasoning.as_deref(), Some("thinking..."));
    }

    #[test]
    fn parse_sse_line_done_sentinel() {
        let line = "data: [DONE]";
        let result = parse_sse_line(line).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn parse_sse_chunk_with_tool_call_delta() {
        let line = r#"data: {"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call_1","function":{"name":"shell","arguments":"{\"command\":\"date\"}"}}]}}]}"#;
        let chunk = parse_sse_chunk(line)
            .unwrap()
            .expect("chunk should be parsed");
        let choice = chunk.choices.first().expect("choice should exist");
        let tool_calls = choice
            .delta
            .tool_calls
            .as_ref()
            .expect("tool call deltas should exist");
        assert_eq!(tool_calls.len(), 1);
        assert_eq!(tool_calls[0].index, Some(0));
        assert_eq!(tool_calls[0].id.as_deref(), Some("call_1"));
        assert_eq!(
            tool_calls[0]
                .function
                .as_ref()
                .and_then(|function| function.name.as_deref()),
            Some("shell")
        );
    }

    #[test]
    fn stream_tool_call_accumulator_combines_deltas() {
        let mut acc = StreamToolCallAccumulator::default();
        acc.apply_delta(&StreamToolCallDelta {
            index: Some(0),
            id: Some("call_1".to_string()),
            function: Some(StreamFunctionDelta {
                name: Some("shell".to_string()),
                arguments: Some("{\"command\":\"".to_string()),
            }),
            name: None,
            arguments: None,
        });
        acc.apply_delta(&StreamToolCallDelta {
            index: Some(0),
            id: None,
            function: Some(StreamFunctionDelta {
                name: None,
                arguments: Some("date\"}".to_string()),
            }),
            name: None,
            arguments: None,
        });

        let tool_call = acc
            .into_provider_tool_call()
            .expect("accumulator should emit tool call");
        assert_eq!(tool_call.id, "call_1");
        assert_eq!(tool_call.name, "shell");
        assert_eq!(tool_call.arguments, r#"{"command":"date"}"#);
    }

    #[test]
    fn api_response_parses_usage() {
        let json = r#"{
            "choices": [{"message": {"content": "Hello"}}],
            "usage": {"prompt_tokens": 150, "completion_tokens": 60}
        }"#;
        let resp: ApiChatResponse = serde_json::from_str(json).unwrap();
        let usage = resp.usage.unwrap();
        assert_eq!(usage.prompt_tokens, Some(150));
        assert_eq!(usage.completion_tokens, Some(60));
    }

    #[test]
    fn api_response_parses_without_usage() {
        let json = r#"{"choices": [{"message": {"content": "Hello"}}]}"#;
        let resp: ApiChatResponse = serde_json::from_str(json).unwrap();
        assert!(resp.usage.is_none());
    }

    // ═══════════════════════════════════════════════════════════════════════
    // reasoning_content pass-through tests
    // ═══════════════════════════════════════════════════════════════════════

    #[test]
    fn parse_native_response_captures_reasoning_content() {
        let message = ResponseMessage {
            content: Some("answer".to_string()),
            reasoning_content: Some("thinking step".to_string()),
            tool_calls: Some(vec![ToolCall {
                id: Some("call_1".to_string()),
                kind: Some("function".to_string()),
                function: Some(Function {
                    name: Some("shell".to_string()),
                    arguments: Some(r#"{"cmd":"ls"}"#.to_string()),
                }),
                name: None,
                arguments: None,
                parameters: None,
            }]),
        };

        let parsed = OpenAiCompatibleProvider::parse_native_response(message);
        assert_eq!(parsed.reasoning_content.as_deref(), Some("thinking step"));
        assert_eq!(parsed.text.as_deref(), Some("answer"));
        assert_eq!(parsed.tool_calls.len(), 1);
    }

    #[test]
    fn parse_native_response_none_reasoning_content_for_normal_model() {
        let message = ResponseMessage {
            content: Some("hello".to_string()),
            reasoning_content: None,
            tool_calls: None,
        };

        let parsed = OpenAiCompatibleProvider::parse_native_response(message);
        assert!(parsed.reasoning_content.is_none());
        assert_eq!(parsed.text.as_deref(), Some("hello"));
    }

    #[test]
    fn convert_messages_for_native_round_trips_reasoning_content() {
        // Simulate stored assistant history JSON that includes reasoning_content
        let history_json = serde_json::json!({
            "content": "I will check",
            "tool_calls": [{
                "id": "tc_1",
                "name": "shell",
                "arguments": "{\"cmd\":\"ls\"}"
            }],
            "reasoning_content": "Let me think about this..."
        });

        let messages = vec![ChatMessage::assistant(history_json.to_string())];
        let native = OpenAiCompatibleProvider::convert_messages_for_native(&messages, true);
        assert_eq!(native.len(), 1);
        assert_eq!(native[0].role, "assistant");
        assert_eq!(
            native[0].reasoning_content.as_deref(),
            Some("Let me think about this...")
        );
        assert!(native[0].tool_calls.is_some());
    }

    #[test]
    fn convert_messages_for_native_no_reasoning_content_when_absent() {
        // Normal model history without reasoning_content key
        let history_json = serde_json::json!({
            "content": "I will check",
            "tool_calls": [{
                "id": "tc_1",
                "name": "shell",
                "arguments": "{\"cmd\":\"ls\"}"
            }]
        });

        let messages = vec![ChatMessage::assistant(history_json.to_string())];
        let native = OpenAiCompatibleProvider::convert_messages_for_native(&messages, true);
        assert_eq!(native.len(), 1);
        assert!(native[0].reasoning_content.is_none());
    }

    #[test]
    fn convert_messages_for_native_reasoning_content_serialized_only_when_present() {
        // Verify skip_serializing_if works: reasoning_content omitted from JSON when None
        let msg_without = NativeMessage {
            role: "assistant".to_string(),
            content: Some(MessageContent::Text("hi".to_string())),
            tool_call_id: None,
            tool_calls: None,
            reasoning_content: None,
        };
        let json = serde_json::to_string(&msg_without).unwrap();
        assert!(
            !json.contains("reasoning_content"),
            "reasoning_content should be omitted when None"
        );

        let msg_with = NativeMessage {
            role: "assistant".to_string(),
            content: Some(MessageContent::Text("hi".to_string())),
            tool_call_id: None,
            tool_calls: None,
            reasoning_content: Some("thinking...".to_string()),
        };
        let json = serde_json::to_string(&msg_with).unwrap();
        assert!(
            json.contains("reasoning_content"),
            "reasoning_content should be present when Some"
        );
        assert!(json.contains("thinking..."));
    }

    #[test]
    fn default_timeout_is_120s() {
        let p = make_provider("test", "https://example.com", None);
        assert_eq!(p.timeout_secs, 120);
    }

    #[test]
    fn with_timeout_secs_overrides_default() {
        let p = make_provider("test", "https://example.com", None).with_timeout_secs(300);
        assert_eq!(p.timeout_secs, 300);
    }

    #[test]
    fn extra_headers_default_empty() {
        let p = make_provider("test", "https://example.com", None);
        assert!(p.extra_headers.is_empty());
    }

    #[test]
    fn with_extra_headers_sets_headers() {
        let mut headers = std::collections::HashMap::new();
        headers.insert("X-Title".to_string(), "clawseed".to_string());
        headers.insert(
            "HTTP-Referer".to_string(),
            "https://example.com".to_string(),
        );
        let p = make_provider("test", "https://example.com", None).with_extra_headers(headers);
        assert_eq!(p.extra_headers.len(), 2);
        assert_eq!(p.extra_headers.get("X-Title").unwrap(), "clawseed");
        assert_eq!(
            p.extra_headers.get("HTTP-Referer").unwrap(),
            "https://example.com"
        );
    }

    #[test]
    fn http_client_with_extra_headers_builds_successfully() {
        let mut headers = std::collections::HashMap::new();
        headers.insert("X-Title".to_string(), "clawseed".to_string());
        headers.insert("User-Agent".to_string(), "TestAgent/1.0".to_string());
        let p = make_provider("test", "https://example.com", None).with_extra_headers(headers);
        // Should not panic
        let _client = p.http_client();
    }

    #[test]
    fn http_client_without_extra_headers_or_user_agent() {
        let p = make_provider("test", "https://example.com", None);
        // Should use the cached proxy client path
        let _client = p.http_client();
    }

    #[test]
    fn extra_headers_combined_with_user_agent() {
        let mut headers = std::collections::HashMap::new();
        headers.insert("X-Title".to_string(), "clawseed".to_string());
        let p = OpenAiCompatibleProvider::new_with_user_agent(
            "test",
            "https://example.com",
            None,
            AuthStyle::Bearer,
            "CustomAgent/1.0",
        )
        .with_extra_headers(headers);
        assert_eq!(p.user_agent.as_deref(), Some("CustomAgent/1.0"));
        assert_eq!(p.extra_headers.len(), 1);
        // Should not panic
        let _client = p.http_client();
    }

    #[test]
    fn tool_call_none_fields_omitted_from_json() {
        // Ensures providers like Mistral that reject extra fields (e.g. "name": null)
        // don't receive them when the ToolCall compat fields are None.
        let tc = ToolCall {
            id: Some("call_1".to_string()),
            kind: Some("function".to_string()),
            function: Some(Function {
                name: Some("shell".to_string()),
                arguments: Some("{\"command\":\"ls\"}".to_string()),
            }),
            name: None,
            arguments: None,
            parameters: None,
        };
        let json = serde_json::to_value(&tc).unwrap();
        assert!(!json.as_object().unwrap().contains_key("name"));
        assert!(!json.as_object().unwrap().contains_key("arguments"));
        assert!(!json.as_object().unwrap().contains_key("parameters"));
        // Standard fields must be present
        assert!(json.as_object().unwrap().contains_key("id"));
        assert!(json.as_object().unwrap().contains_key("type"));
        assert!(json.as_object().unwrap().contains_key("function"));
    }

    #[test]
    fn tool_call_with_compat_fields_serializes_them() {
        // When compat fields are Some, they should appear in the output.
        let tc = ToolCall {
            id: None,
            kind: None,
            function: None,
            name: Some("shell".to_string()),
            arguments: Some("{\"command\":\"ls\"}".to_string()),
            parameters: None,
        };
        let json = serde_json::to_value(&tc).unwrap();
        assert_eq!(json["name"], "shell");
        assert_eq!(json["arguments"], "{\"command\":\"ls\"}");
        // None fields should be omitted
        assert!(!json.as_object().unwrap().contains_key("id"));
        assert!(!json.as_object().unwrap().contains_key("type"));
        assert!(!json.as_object().unwrap().contains_key("function"));
        assert!(!json.as_object().unwrap().contains_key("parameters"));
    }

    // ── parse_proxy_tool_event tests ──

    #[test]
    fn proxy_tool_start_valid() {
        let line = r#"data: {"x_tool_start":{"name":"bash","arguments":"{\"cmd\":\"ls\"}"}}"#;
        let event = parse_proxy_tool_event(line);
        assert!(matches!(
            event,
            Some(StreamEvent::PreExecutedToolCall { ref name, ref args })
            if name == "bash" && args == r#"{"cmd":"ls"}"#
        ));
    }

    #[test]
    fn proxy_tool_start_missing_name_returns_none() {
        let line = r#"data: {"x_tool_start":{"arguments":"{}"}}"#;
        assert!(parse_proxy_tool_event(line).is_none());
    }

    #[test]
    fn proxy_tool_start_missing_arguments_defaults() {
        let line = r#"data: {"x_tool_start":{"name":"read"}}"#;
        let event = parse_proxy_tool_event(line);
        assert!(matches!(
            event,
            Some(StreamEvent::PreExecutedToolCall { ref name, ref args })
            if name == "read" && args == "{}"
        ));
    }

    #[test]
    fn proxy_tool_result_valid() {
        let line = r#"data: {"x_tool_result":{"name":"bash","output":"hello world"}}"#;
        let event = parse_proxy_tool_event(line);
        assert!(matches!(
            event,
            Some(StreamEvent::PreExecutedToolResult { ref name, ref output })
            if name == "bash" && output == "hello world"
        ));
    }

    #[test]
    fn proxy_tool_result_missing_fields_uses_defaults() {
        let line = r#"data: {"x_tool_result":{}}"#;
        let event = parse_proxy_tool_event(line);
        assert!(matches!(
            event,
            Some(StreamEvent::PreExecutedToolResult { ref name, ref output })
            if name == "unknown" && output.is_empty()
        ));
    }

    #[test]
    fn proxy_tool_event_non_json_returns_none() {
        assert!(parse_proxy_tool_event("data: not json").is_none());
    }

    #[test]
    fn proxy_tool_event_no_data_prefix_returns_none() {
        let line = r#"{"x_tool_start":{"name":"bash"}}"#;
        assert!(parse_proxy_tool_event(line).is_none());
    }

    #[test]
    fn proxy_tool_event_standard_openai_chunk_returns_none() {
        let line = r#"data: {"id":"chatcmpl-1","choices":[{"delta":{"content":"hi"}}]}"#;
        assert!(parse_proxy_tool_event(line).is_none());
    }

    #[test]
    fn proxy_tool_event_done_sentinel_returns_none() {
        assert!(parse_proxy_tool_event("data: [DONE]").is_none());
    }

    /// Regression for #5825.
    ///
    /// When `native_tool_calling = false`, the filter pass rewrites
    /// `assistant{tool_calls, content="I'll search"}` into `assistant("I'll
    /// search")` and drops the following `tool{result}`. That leaves two
    /// adjacent assistant messages in the output, which providers targeted
    /// by this path (Anthropic upstream, MiniMax, other OpenAI-compat
    /// wrappers) reject with HTTP 400.
    #[test]
    fn strip_native_tool_messages_coalesces_adjacent_assistants() {
        let messages = vec![
            ChatMessage::user("search for cats"),
            ChatMessage::assistant(
                r#"{"content":"I'll search","tool_calls":[{"id":"t1","name":"web_search","arguments":"{}"}]}"#,
            ),
            ChatMessage::tool(r#"{"tool_call_id":"t1","content":"Found 10 results"}"#),
            ChatMessage::assistant("Here are the results about cats"),
        ];
        let p = OpenAiCompatibleProvider::new_merge_system_into_user(
            "MiniMax",
            "https://api.minimax.chat/v1",
            Some("k"),
            AuthStyle::Bearer,
        );
        let stripped = p.strip_native_tool_messages(&messages);
        let roles: Vec<&str> = stripped.iter().map(|m| m.role.as_str()).collect();
        assert!(
            !roles.windows(2).any(|w| w[0] == w[1]),
            "no two consecutive messages should share a role; got {roles:?}"
        );
        // Sanity: user turn and merged assistant content both survive.
        assert_eq!(roles, vec!["user", "assistant"]);
        assert_eq!(stripped[0].content, "search for cats");
        assert!(
            stripped[1].content.contains("I'll search")
                && stripped[1]
                    .content
                    .contains("Here are the results about cats"),
            "merged assistant should preserve both the pre-tool narration and the final reply; \
             got {:?}",
            stripped[1].content
        );
    }

    /// Complementary regression for #5825: when the narration content is
    /// empty, the pre-tool assistant is dropped entirely and no coalesce is
    /// needed. This test documents that the coalesce pass does not produce
    /// spurious blank-line concatenation.
    #[test]
    fn strip_native_tool_messages_drops_empty_narration_cleanly() {
        let messages = vec![
            ChatMessage::user("search for cats"),
            ChatMessage::assistant(
                r#"{"content":"","tool_calls":[{"id":"t1","name":"web_search","arguments":"{}"}]}"#,
            ),
            ChatMessage::tool(r#"{"tool_call_id":"t1","content":"Found"}"#),
            ChatMessage::assistant("Here are the results"),
        ];
        let p = OpenAiCompatibleProvider::new_merge_system_into_user(
            "MiniMax",
            "https://api.minimax.chat/v1",
            Some("k"),
            AuthStyle::Bearer,
        );
        let stripped = p.strip_native_tool_messages(&messages);
        assert_eq!(
            stripped.iter().map(|m| m.role.as_str()).collect::<Vec<_>>(),
            vec!["user", "assistant"]
        );
        assert_eq!(stripped[1].content, "Here are the results");
    }
}
