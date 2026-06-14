use crate::traits::{
    ChatMessage, StreamChunk, StreamError, StreamEvent, StreamResult, ToolCall as ProviderToolCall,
};
use futures_util::{StreamExt, stream};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize)]
pub(super) struct ApiChatRequest {
    pub(super) model: String,
    pub(super) messages: Vec<Message>,
    pub(super) temperature: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) reasoning_effort: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) tool_stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) tools: Option<Vec<serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) tool_choice: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) max_tokens: Option<u32>,
}

#[derive(Debug, Serialize)]
pub(super) struct Message {
    pub(super) role: String,
    pub(super) content: MessageContent,
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
pub(super) enum MessageContent {
    Text(String),
    Parts(Vec<MessagePart>),
}

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(super) enum MessagePart {
    Text { text: String },
    ImageUrl { image_url: ImageUrlPart },
}

#[derive(Debug, Serialize)]
pub(super) struct ImageUrlPart {
    pub(super) url: String,
}

#[derive(Debug, Deserialize)]
pub(super) struct ApiChatResponse {
    pub(super) choices: Vec<Choice>,
    #[serde(default)]
    pub(super) usage: Option<UsageInfo>,
}

#[derive(Debug, Deserialize)]
pub(super) struct UsageInfo {
    #[serde(default)]
    pub(super) prompt_tokens: Option<u64>,
    #[serde(default)]
    pub(super) completion_tokens: Option<u64>,
    /// DeepSeek reports `prompt_cache_hit_tokens` for prefix-cached input.
    #[serde(default, rename = "prompt_cache_hit_tokens")]
    pub(super) prompt_cache_hit_tokens: Option<u64>,
    /// DeepSeek reports `prompt_cache_miss_tokens`. Retained for deserialization
    /// but not yet exposed via `TokenUsage` — will be wired to metrics when
    /// observability is added.
    #[serde(default, rename = "prompt_cache_miss_tokens")]
    #[expect(dead_code, reason = "reserved for future metrics wiring")]
    pub(super) prompt_cache_miss_tokens: Option<u64>,
    /// OpenAI reports `prompt_tokens_details` with a `cached_tokens` sub-field.
    #[serde(default)]
    pub(super) prompt_tokens_details: Option<PromptTokensDetails>,
}

impl UsageInfo {
    /// Extract cached input tokens from whichever format the provider reports.
    /// DeepSeek uses `prompt_cache_hit_tokens`; OpenAI uses nested `prompt_tokens_details.cached_tokens`.
    pub(super) fn extract_cached_tokens(&self) -> Option<u64> {
        self.prompt_cache_hit_tokens
            .or_else(|| self.prompt_tokens_details.as_ref()?.cached_tokens)
    }
}

/// OpenAI `prompt_tokens_details` sub-object, containing `cached_tokens`.
#[derive(Debug, Deserialize)]
pub(super) struct PromptTokensDetails {
    #[serde(default)]
    pub(super) cached_tokens: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub(super) struct Choice {
    pub(super) message: ResponseMessage,
    #[serde(default)]
    pub(super) finish_reason: Option<String>,
}

/// Remove `<think>...</think>` blocks from model output.
/// Some reasoning models (e.g. MiniMax) embed their chain-of-thought inline
/// in the `content` field rather than a separate `reasoning_content` field.
/// The resulting `<think>` tags must be stripped before returning to the user.
pub(super) fn strip_think_tags(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut rest = s;
    loop {
        if let Some(start) = rest.find("<think>") {
            result.push_str(&rest[..start]);
            if let Some(end) = rest[start..].find("</think>") {
                rest = &rest[start + end + "</think>".len()..];
            } else {
                // Unclosed tag: drop the rest to avoid leaking partial reasoning.
                break;
            }
        } else {
            result.push_str(rest);
            break;
        }
    }
    result.trim().to_string()
}

#[derive(Debug, Deserialize, Serialize)]
pub(super) struct ResponseMessage {
    #[serde(default)]
    pub(super) content: Option<String>,
    /// Reasoning/thinking models (e.g. Qwen3, GLM-4) may return their output
    /// in `reasoning_content` instead of `content`. Used as automatic fallback.
    #[serde(default)]
    pub(super) reasoning_content: Option<String>,
    #[serde(default)]
    pub(super) tool_calls: Option<Vec<ToolCall>>,
}

impl ResponseMessage {
    /// Extract text content, falling back to `reasoning_content` when `content`
    /// is missing or empty. Reasoning/thinking models (Qwen3, GLM-4, etc.)
    /// often return their output solely in `reasoning_content`.
    /// Strips `<think>...</think>` blocks that some models (e.g. MiniMax) embed
    /// inline in `content` instead of using a separate field.
    pub(super) fn effective_content(&self) -> String {
        if let Some(content) = self.content.as_ref().filter(|c| !c.is_empty()) {
            let stripped = strip_think_tags(content);
            if !stripped.is_empty() {
                return stripped;
            }
        }

        self.reasoning_content
            .as_ref()
            .map(|c| strip_think_tags(c))
            .filter(|c| !c.is_empty())
            .unwrap_or_default()
    }

    pub(super) fn effective_content_optional(&self) -> Option<String> {
        if let Some(content) = self.content.as_ref().filter(|c| !c.is_empty()) {
            let stripped = strip_think_tags(content);
            if !stripped.is_empty() {
                return Some(stripped);
            }
        }

        self.reasoning_content
            .as_ref()
            .map(|c| strip_think_tags(c))
            .filter(|c| !c.is_empty())
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub(super) struct ToolCall {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) id: Option<String>,
    #[serde(rename = "type")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) function: Option<Function>,

    // Compatibility: Some providers (e.g., older GLM) may use 'name' directly
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) arguments: Option<String>,

    // Compatibility: DeepSeek sometimes wraps arguments differently
    #[serde(
        rename = "parameters",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub(super) parameters: Option<serde_json::Value>,
}

impl ToolCall {
    /// Extract function name with fallback logic for various provider formats
    pub(super) fn function_name(&self) -> Option<String> {
        // Standard OpenAI format: tool_calls[].function.name
        if let Some(ref func) = self.function
            && let Some(ref name) = func.name
        {
            return Some(name.clone());
        }
        // Fallback: direct name field
        self.name.clone()
    }

    /// Extract arguments with fallback logic and type conversion
    pub(super) fn function_arguments(&self) -> Option<String> {
        // Standard OpenAI format: tool_calls[].function.arguments (string)
        if let Some(ref func) = self.function
            && let Some(ref args) = func.arguments
        {
            return Some(args.clone());
        }
        // Fallback: direct arguments field
        if let Some(ref args) = self.arguments {
            return Some(args.clone());
        }
        // Compatibility: Some providers return parameters as object instead of string
        if let Some(ref params) = self.parameters {
            return serde_json::to_string(params).ok();
        }
        None
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub(super) struct Function {
    #[serde(default)]
    pub(super) name: Option<String>,
    #[serde(default)]
    pub(super) arguments: Option<String>,
}

#[derive(Debug, Serialize)]
pub(super) struct NativeChatRequest {
    pub(super) model: String,
    pub(super) messages: Vec<NativeMessage>,
    pub(super) temperature: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) reasoning_effort: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) tool_stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) tools: Option<Vec<serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) tool_choice: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) max_tokens: Option<u32>,
}

#[derive(Debug, Serialize)]
pub(super) struct NativeMessage {
    pub(super) role: String,
    pub(super) content: Option<MessageContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) tool_calls: Option<Vec<ToolCall>>,
    /// Raw reasoning content from thinking models; pass-through for providers
    /// that require it in assistant tool-call history messages.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) reasoning_content: Option<String>,
}

#[derive(Debug, Serialize)]
pub(super) struct ResponsesRequest {
    pub(super) model: String,
    pub(super) input: Vec<ResponsesInput>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) instructions: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) stream: Option<bool>,
}

#[derive(Debug, Serialize)]
pub(super) struct ResponsesInput {
    pub(super) role: String,
    pub(super) content: ResponsesInputContent,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub(super) kind: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
pub(super) enum ResponsesInputContent {
    Text(String),
    Parts(Vec<ResponsesInputPart>),
}

#[derive(Debug, Serialize)]
pub(super) struct ResponsesInputPart {
    #[serde(rename = "type")]
    pub(super) kind: String,
    pub(super) text: String,
}

impl ResponsesInput {
    pub(super) fn user_text(content: String) -> Self {
        Self {
            role: "user".to_string(),
            content: ResponsesInputContent::Text(content),
            kind: None,
        }
    }

    pub(super) fn assistant_output_text(content: String) -> Self {
        Self {
            role: "assistant".to_string(),
            content: ResponsesInputContent::Parts(vec![ResponsesInputPart {
                kind: "output_text".to_string(),
                text: content,
            }]),
            kind: Some("message".to_string()),
        }
    }
}

#[derive(Debug, Deserialize)]
pub(super) struct ResponsesResponse {
    #[serde(default)]
    pub(super) output: Vec<ResponsesOutput>,
    #[serde(default)]
    pub(super) output_text: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct ResponsesOutput {
    #[serde(default)]
    pub(super) content: Vec<ResponsesContent>,
}

#[derive(Debug, Deserialize)]
pub(super) struct ResponsesContent {
    #[serde(rename = "type")]
    pub(super) kind: Option<String>,
    pub(super) text: Option<String>,
}

// ---------------------------------------------------------------
// Streaming support (SSE parser)
// ---------------------------------------------------------------

/// Server-Sent Event stream chunk for OpenAI-compatible streaming.
#[derive(Debug, Deserialize)]
pub(super) struct StreamChunkResponse {
    #[serde(default)]
    pub(super) choices: Vec<StreamChoice>,
}

#[derive(Debug, Deserialize)]
pub(super) struct StreamChoice {
    #[serde(default)]
    pub(super) delta: StreamDelta,
    #[serde(default)]
    pub(super) finish_reason: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
pub(super) struct StreamDelta {
    #[serde(default)]
    pub(super) content: Option<String>,
    /// Reasoning/thinking models may stream output via `reasoning_content`.
    #[serde(default)]
    pub(super) reasoning_content: Option<String>,
    /// Native tool-calling deltas in OpenAI chat-completions streaming format.
    #[serde(default)]
    pub(super) tool_calls: Option<Vec<StreamToolCallDelta>>,
}

#[derive(Debug, Deserialize)]
pub(super) struct StreamToolCallDelta {
    #[serde(default)]
    pub(super) index: Option<usize>,
    #[serde(default)]
    pub(super) id: Option<String>,
    #[serde(default)]
    pub(super) function: Option<StreamFunctionDelta>,
    // Compatibility: some providers stream name/arguments at top-level.
    #[serde(default)]
    pub(super) name: Option<String>,
    #[serde(default)]
    pub(super) arguments: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(super) struct StreamFunctionDelta {
    #[serde(default)]
    pub(super) name: Option<String>,
    #[serde(default)]
    pub(super) arguments: Option<String>,
}

#[derive(Debug, Default)]
pub(super) struct StreamToolCallAccumulator {
    pub(super) id: Option<String>,
    pub(super) name: Option<String>,
    pub(super) arguments: String,
}

impl StreamToolCallAccumulator {
    pub(super) fn apply_delta(&mut self, delta: &StreamToolCallDelta) {
        if let Some(id) = delta.id.as_ref().filter(|value| !value.is_empty()) {
            self.id = Some(id.clone());
        }

        let delta_name = delta
            .function
            .as_ref()
            .and_then(|function| function.name.as_ref())
            .or(delta.name.as_ref())
            .filter(|value| !value.is_empty());
        if let Some(name) = delta_name {
            self.name = Some(name.clone());
        }

        if let Some(arguments_delta) = delta
            .function
            .as_ref()
            .and_then(|function| function.arguments.as_ref())
            .or(delta.arguments.as_ref())
            .filter(|value| !value.is_empty())
        {
            self.arguments.push_str(arguments_delta);
        }
    }

    pub(super) fn into_provider_tool_call(self) -> Option<ProviderToolCall> {
        let name = self.name?;
        let arguments = if self.arguments.trim().is_empty() {
            "{}".to_string()
        } else {
            self.arguments
        };
        let normalized_arguments = if serde_json::from_str::<serde_json::Value>(&arguments).is_ok()
        {
            arguments
        } else {
            tracing::warn!(
                function = %name,
                arguments = %arguments,
                "Invalid JSON in streamed native tool-call arguments, using empty object"
            );
            "{}".to_string()
        };

        Some(ProviderToolCall {
            id: self.id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
            name,
            arguments: normalized_arguments,
        })
    }
}

pub(super) fn parse_sse_chunk(line: &str) -> StreamResult<Option<StreamChunkResponse>> {
    let line = line.trim();

    if line.is_empty() || line.starts_with(':') {
        return Ok(None);
    }

    let Some(data) = line.strip_prefix("data:") else {
        return Ok(None);
    };
    let data = data.trim();

    if data == "[DONE]" {
        return Ok(None);
    }

    serde_json::from_str(data)
        .map(Some)
        .map_err(StreamError::Json)
}

/// Parse custom proxy tool events from SSE lines.
/// These are emitted by proxies like claude-max-api-proxy that execute tools
/// internally and forward observability events via custom SSE fields.
pub(super) fn parse_proxy_tool_event(line: &str) -> Option<StreamEvent> {
    let data = line.trim().strip_prefix("data:")?.trim();
    let obj: serde_json::Value = serde_json::from_str(data).ok()?;

    if let Some(ts) = obj.get("x_tool_start") {
        let Some(name) = ts.get("name").and_then(|v| v.as_str()) else {
            tracing::debug!("proxy x_tool_start event missing required 'name' field");
            return None;
        };
        let name = name.to_string();
        let args = ts
            .get("arguments")
            .and_then(|v| v.as_str())
            .unwrap_or("{}")
            .to_string();
        return Some(StreamEvent::PreExecutedToolCall { name, args });
    }

    if let Some(tr) = obj.get("x_tool_result") {
        let name = tr
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();
        let output = tr
            .get("output")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        return Some(StreamEvent::PreExecutedToolResult { name, output });
    }

    None
}

pub(super) fn extract_sse_text_delta(choice: &StreamChoice) -> Option<String> {
    if let Some(content) = &choice.delta.content
        && !content.is_empty()
    {
        return Some(content.clone());
    }

    None
}

pub(super) fn extract_sse_reasoning_delta(choice: &StreamChoice) -> Option<String> {
    choice
        .delta
        .reasoning_content
        .as_ref()
        .filter(|value| !value.is_empty())
        .cloned()
}

/// Parse SSE (Server-Sent Events) stream from OpenAI-compatible providers.
/// Handles the `data: {...}` format and `[DONE]` sentinel.
///
/// Returns a `StreamChunk` that distinguishes content from reasoning:
/// - Content deltas → `StreamChunk::delta`
/// - Reasoning deltas → `StreamChunk::reasoning`
pub(super) fn parse_sse_line(line: &str) -> StreamResult<Option<StreamChunk>> {
    let chunk = match parse_sse_chunk(line)? {
        Some(c) => c,
        None => return Ok(None),
    };

    if let Some(choice) = chunk.choices.first() {
        if let Some(content) = &choice.delta.content
            && !content.is_empty()
        {
            return Ok(Some(StreamChunk::delta(content.clone())));
        }
        if let Some(reasoning) = &choice.delta.reasoning_content
            && !reasoning.is_empty()
        {
            return Ok(Some(StreamChunk::reasoning(reasoning.clone())));
        }
    }

    Ok(None)
}

/// Convert SSE byte stream to text chunks.
pub(super) fn sse_bytes_to_chunks(
    response: reqwest::Response,
    count_tokens: bool,
) -> stream::BoxStream<'static, StreamResult<StreamChunk>> {
    let (tx, rx) = tokio::sync::mpsc::channel::<StreamResult<StreamChunk>>(100);

    tokio::spawn(async move {
        let mut buffer = String::new();

        match response.error_for_status_ref() {
            Ok(_) => {}
            Err(e) => {
                let _ = tx.send(Err(StreamError::Http(e.to_string()))).await;
                return;
            }
        }

        let mut bytes_stream = response.bytes_stream();
        // Accumulate partial UTF-8 sequences that may be split across
        // HTTP/1.1 chunked transfer boundaries (e.g. 3-byte CJK chars).
        let mut utf8_buf: Vec<u8> = Vec::new();

        while let Some(item) = bytes_stream.next().await {
            match item {
                Ok(bytes) => {
                    utf8_buf.extend_from_slice(&bytes);
                    let text = match std::str::from_utf8(&utf8_buf) {
                        Ok(s) => {
                            let owned = s.to_string();
                            utf8_buf.clear();
                            owned
                        }
                        Err(e) => {
                            let valid_up_to = e.valid_up_to();
                            if valid_up_to == 0 && utf8_buf.len() < 4 {
                                // Could still be an incomplete multi-byte char; wait for more data
                                continue;
                            }
                            let valid =
                                String::from_utf8_lossy(&utf8_buf[..valid_up_to]).into_owned();
                            utf8_buf.drain(..valid_up_to);
                            valid
                        }
                    };
                    if text.is_empty() {
                        continue;
                    }

                    buffer.push_str(&text);

                    while let Some(pos) = buffer.find('\n') {
                        let line = buffer[..pos].to_string();
                        buffer.drain(..=pos);

                        match parse_sse_line(&line) {
                            Ok(Some(chunk)) => {
                                let chunk = if count_tokens {
                                    chunk.with_token_estimate()
                                } else {
                                    chunk
                                };
                                if tx.send(Ok(chunk)).await.is_err() {
                                    return; // Receiver dropped
                                }
                            }
                            Ok(None) => {}
                            Err(e) => {
                                let _ = tx.send(Err(e)).await;
                                return;
                            }
                        }
                    }
                }
                Err(e) => {
                    let _ = tx.send(Err(StreamError::Http(e.to_string()))).await;
                    return;
                }
            }
        }

        let _ = tx.send(Ok(StreamChunk::final_chunk())).await;
    });

    stream::unfold(rx, |mut rx| async {
        rx.recv().await.map(|chunk| (chunk, rx))
    })
    .boxed()
}

/// Convert SSE byte stream to structured streaming events.
pub(crate) fn sse_bytes_to_events(
    response: reqwest::Response,
    count_tokens: bool,
) -> stream::BoxStream<'static, StreamResult<StreamEvent>> {
    let (tx, rx) = tokio::sync::mpsc::channel::<StreamResult<StreamEvent>>(100);

    tokio::spawn(async move {
        let mut buffer = String::new();
        let mut tool_calls: Vec<StreamToolCallAccumulator> = Vec::new();
        let mut emitted_tool_calls = false;
        let mut last_stop_reason = clawseed_api::provider::StopReason::EndTurn;

        match response.error_for_status_ref() {
            Ok(_) => {}
            Err(e) => {
                let _ = tx.send(Err(StreamError::Http(e.to_string()))).await;
                return;
            }
        }

        let mut bytes_stream = response.bytes_stream();
        // Accumulate partial UTF-8 sequences split across chunk boundaries.
        let mut utf8_buf: Vec<u8> = Vec::new();
        while let Some(item) = bytes_stream.next().await {
            match item {
                Ok(bytes) => {
                    utf8_buf.extend_from_slice(&bytes);
                    let text = match std::str::from_utf8(&utf8_buf) {
                        Ok(s) => {
                            let owned = s.to_string();
                            utf8_buf.clear();
                            owned
                        }
                        Err(e) => {
                            let valid_up_to = e.valid_up_to();
                            if valid_up_to == 0 && utf8_buf.len() < 4 {
                                continue;
                            }
                            let valid =
                                String::from_utf8_lossy(&utf8_buf[..valid_up_to]).into_owned();
                            utf8_buf.drain(..valid_up_to);
                            valid
                        }
                    };
                    if text.is_empty() {
                        continue;
                    }

                    buffer.push_str(&text);

                    while let Some(pos) = buffer.find('\n') {
                        let line = buffer[..pos].to_string();
                        buffer.drain(..=pos);

                        // Custom proxy events for pre-executed tool calls
                        // (e.g. claude-max-api-proxy streaming x_tool_start/x_tool_result)
                        if let Some(event) = parse_proxy_tool_event(&line) {
                            if tx.send(Ok(event)).await.is_err() {
                                return;
                            }
                            continue;
                        }

                        let chunk = match parse_sse_chunk(&line) {
                            Ok(Some(chunk)) => chunk,
                            Ok(None) => continue,
                            Err(e) => {
                                let _ = tx.send(Err(e)).await;
                                return;
                            }
                        };

                        let mut should_emit_tool_calls = false;
                        for choice in &chunk.choices {
                            if let Some(reasoning_delta) = extract_sse_reasoning_delta(choice) {
                                let reasoning_chunk = StreamChunk::reasoning(reasoning_delta);
                                if tx
                                    .send(Ok(StreamEvent::TextDelta(reasoning_chunk)))
                                    .await
                                    .is_err()
                                {
                                    return;
                                }
                            }
                            if let Some(text_delta) = extract_sse_text_delta(choice) {
                                let mut text_chunk = StreamChunk::delta(text_delta);
                                if count_tokens {
                                    text_chunk = text_chunk.with_token_estimate();
                                }
                                if tx
                                    .send(Ok(StreamEvent::TextDelta(text_chunk)))
                                    .await
                                    .is_err()
                                {
                                    return;
                                }
                            }

                            if let Some(deltas) = choice.delta.tool_calls.as_ref() {
                                for delta in deltas {
                                    let index = delta.index.unwrap_or(tool_calls.len());
                                    if index >= tool_calls.len() {
                                        tool_calls.resize_with(index + 1, Default::default);
                                    }
                                    if let Some(acc) = tool_calls.get_mut(index) {
                                        acc.apply_delta(delta);
                                    }
                                }
                            }

                            if choice.finish_reason.as_deref() == Some("tool_calls") {
                                should_emit_tool_calls = true;
                            }
                            if let Some(reason) = &choice.finish_reason {
                                last_stop_reason = match reason.as_str() {
                                    "length" => clawseed_api::provider::StopReason::MaxTokens,
                                    "tool_calls" => clawseed_api::provider::StopReason::ToolUse,
                                    _ => clawseed_api::provider::StopReason::EndTurn,
                                };
                            }
                        }

                        if should_emit_tool_calls && !emitted_tool_calls {
                            emitted_tool_calls = true;
                            for tool_call in tool_calls
                                .drain(..)
                                .filter_map(StreamToolCallAccumulator::into_provider_tool_call)
                            {
                                if tx.send(Ok(StreamEvent::ToolCall(tool_call))).await.is_err() {
                                    return;
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    let _ = tx.send(Err(StreamError::Http(e.to_string()))).await;
                    return;
                }
            }
        }

        if !emitted_tool_calls {
            for tool_call in tool_calls
                .drain(..)
                .filter_map(StreamToolCallAccumulator::into_provider_tool_call)
            {
                if tx.send(Ok(StreamEvent::ToolCall(tool_call))).await.is_err() {
                    return;
                }
            }
        }

        let _ = tx
            .send(Ok(StreamEvent::Final {
                stop_reason: last_stop_reason,
            }))
            .await;
    });

    stream::unfold(rx, |mut rx| async move {
        rx.recv().await.map(|event| (event, rx))
    })
    .boxed()
}

pub(super) fn first_nonempty(text: Option<&str>) -> Option<String> {
    text.and_then(|value| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

pub(super) fn build_responses_prompt(
    messages: &[ChatMessage],
) -> (Option<String>, Vec<ResponsesInput>) {
    let mut instructions_parts = Vec::new();
    let mut input = Vec::new();

    for message in messages {
        if message.content.trim().is_empty() {
            continue;
        }

        if message.role == "system" {
            instructions_parts.push(message.content.clone());
            continue;
        }

        let input_item = match message.role.as_str() {
            // llama.cpp Responses parser expects assistant history items in
            // "output_message" shape (`type=message`, `output_text` parts).
            "assistant" | "tool" => ResponsesInput::assistant_output_text(message.content.clone()),
            _ => ResponsesInput::user_text(message.content.clone()),
        };
        input.push(input_item);
    }

    let instructions = if instructions_parts.is_empty() {
        None
    } else {
        Some(instructions_parts.join("\n\n"))
    };

    (instructions, input)
}

pub(super) fn extract_responses_text(response: ResponsesResponse) -> Option<String> {
    if let Some(text) = first_nonempty(response.output_text.as_deref()) {
        return Some(text);
    }

    for item in &response.output {
        for content in &item.content {
            if content.kind.as_deref() == Some("output_text")
                && let Some(text) = first_nonempty(content.text.as_deref())
            {
                return Some(text);
            }
        }
    }

    for item in &response.output {
        for content in &item.content {
            if let Some(text) = first_nonempty(content.text.as_deref()) {
                return Some(text);
            }
        }
    }

    None
}

pub(super) fn compact_sanitized_body_snippet(body: &str) -> String {
    crate::sanitize_api_error(body)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

pub(super) fn parse_chat_response_body(
    provider_name: &str,
    body: &str,
) -> anyhow::Result<ApiChatResponse> {
    serde_json::from_str::<ApiChatResponse>(body).map_err(|error| {
        let snippet = compact_sanitized_body_snippet(body);
        anyhow::anyhow!(
            "{provider_name} API returned an unexpected chat-completions payload: {error}; body={snippet}"
        )
    })
}

pub(super) fn parse_responses_response_body(
    provider_name: &str,
    body: &str,
) -> anyhow::Result<ResponsesResponse> {
    serde_json::from_str::<ResponsesResponse>(body).map_err(|error| {
        let snippet = compact_sanitized_body_snippet(body);
        anyhow::anyhow!(
            "{provider_name} Responses API returned an unexpected payload: {error}; body={snippet}"
        )
    })
}
