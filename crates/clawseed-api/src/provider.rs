//! Provider trait and LLM communication types.

use std::fmt::Write;
use std::sync::Arc;

use async_trait::async_trait;
use futures_util::stream::{self, StreamExt};
use serde::{Deserialize, Serialize};

use crate::tool::ToolSpec;

/// A single message in a conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

impl ChatMessage {
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: "system".into(),
            content: content.into(),
        }
    }

    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: "user".into(),
            content: content.into(),
        }
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: "assistant".into(),
            content: content.into(),
        }
    }

    pub fn tool(content: impl Into<String>) -> Self {
        Self {
            role: "tool".into(),
            content: content.into(),
        }
    }
}

/// A tool call requested by the LLM.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: String,
}

/// Raw token counts from a single LLM API response.
#[derive(Debug, Clone, Default)]
pub struct TokenUsage {
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub cached_input_tokens: Option<u64>,
}

/// An LLM response that may contain text, tool calls, or both.
#[derive(Debug, Clone)]
pub struct ChatResponse {
    pub text: Option<String>,
    pub tool_calls: Vec<ToolCall>,
    pub usage: Option<TokenUsage>,
    pub reasoning_content: Option<String>,
}

impl ChatResponse {
    pub fn has_tool_calls(&self) -> bool {
        !self.tool_calls.is_empty()
    }

    pub fn text_or_empty(&self) -> &str {
        self.text.as_deref().unwrap_or("")
    }
}

/// Request payload for provider chat calls.
#[derive(Debug, Clone, Copy)]
pub struct ChatRequest<'a> {
    pub messages: &'a [ChatMessage],
    pub tools: Option<&'a [ToolSpec]>,
}

/// A tool result to feed back to the LLM.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResultMessage {
    pub tool_call_id: String,
    pub content: String,
}

/// A message in a multi-turn conversation, including tool interactions.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum ConversationMessage {
    Chat(ChatMessage),
    AssistantToolCalls {
        text: Option<String>,
        tool_calls: Vec<ToolCall>,
        reasoning_content: Option<String>,
    },
    ToolResults(Vec<ToolResultMessage>),
}

/// A chunk of content from a streaming response.
#[derive(Debug, Clone)]
pub struct StreamChunk {
    pub delta: String,
    pub reasoning: Option<String>,
    pub is_final: bool,
    pub token_count: usize,
}

impl StreamChunk {
    pub fn delta(text: impl Into<String>) -> Self {
        Self {
            delta: text.into(),
            reasoning: None,
            is_final: false,
            token_count: 0,
        }
    }

    pub fn reasoning(text: impl Into<String>) -> Self {
        Self {
            delta: String::new(),
            reasoning: Some(text.into()),
            is_final: false,
            token_count: 0,
        }
    }

    pub fn final_chunk() -> Self {
        Self {
            delta: String::new(),
            reasoning: None,
            is_final: true,
            token_count: 0,
        }
    }

    pub fn error(message: impl Into<String>) -> Self {
        Self {
            delta: message.into(),
            reasoning: None,
            is_final: true,
            token_count: 0,
        }
    }

    pub fn with_token_estimate(mut self) -> Self {
        self.token_count = self.delta.len().div_ceil(4);
        self
    }
}

/// Structured events emitted by provider streaming APIs.
#[derive(Debug, Clone)]
pub enum StreamEvent {
    TextDelta(StreamChunk),
    ToolCall(ToolCall),
    PreExecutedToolCall { name: String, args: String },
    PreExecutedToolResult { name: String, output: String },
    Final,
}

impl StreamEvent {
    pub fn from_chunk(chunk: StreamChunk) -> Self {
        if chunk.is_final {
            Self::Final
        } else {
            Self::TextDelta(chunk)
        }
    }
}

/// Options for streaming chat requests.
#[derive(Debug, Clone, Copy, Default)]
pub struct StreamOptions {
    pub enabled: bool,
    pub count_tokens: bool,
}

impl StreamOptions {
    pub fn new(enabled: bool) -> Self {
        Self {
            enabled,
            count_tokens: false,
        }
    }
}

/// Result type for streaming operations.
pub type StreamResult<T> = std::result::Result<T, StreamError>;

/// Errors during streaming.
#[derive(Debug, thiserror::Error)]
pub enum StreamError {
    #[error("HTTP error: {0}")]
    Http(String),
    #[error("JSON parse error: {0}")]
    Json(serde_json::Error),
    #[error("Invalid SSE format: {0}")]
    InvalidSse(String),
    #[error("Provider error: {0}")]
    Provider(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// Structured error for unsupported provider capabilities.
#[derive(Debug, Clone, thiserror::Error)]
#[error("provider_capability_error provider={provider} capability={capability} message={message}")]
pub struct ProviderCapabilityError {
    pub provider: String,
    pub capability: String,
    pub message: String,
}

/// Provider capabilities declaration.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProviderCapabilities {
    pub native_tool_calling: bool,
    pub vision: bool,
    pub prompt_caching: bool,
}

/// Provider-specific tool payload formats.
#[derive(Debug, Clone)]
pub enum ToolsPayload {
    Gemini {
        function_declarations: Vec<serde_json::Value>,
    },
    Anthropic {
        tools: Vec<serde_json::Value>,
    },
    OpenAI {
        tools: Vec<serde_json::Value>,
    },
    PromptGuided {
        instructions: String,
    },
}

/// Industry-neutral default temperature.
pub const BASELINE_TEMPERATURE: f64 = 0.7;
/// Default max output tokens.
pub const BASELINE_MAX_TOKENS: u32 = 4096;
/// Default HTTP timeout for cloud inference.
pub const BASELINE_TIMEOUT_SECS: u64 = 120;
/// Default wire protocol.
pub const BASELINE_WIRE_API: &str = "chat_completions";

/// Provider trait — every LLM provider implements this.
#[async_trait]
pub trait Provider: Send + Sync {
    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities::default()
    }

    fn default_temperature(&self) -> f64 {
        BASELINE_TEMPERATURE
    }

    fn default_max_tokens(&self) -> u32 {
        BASELINE_MAX_TOKENS
    }

    fn default_timeout_secs(&self) -> u64 {
        BASELINE_TIMEOUT_SECS
    }

    fn default_base_url(&self) -> Option<&str> {
        None
    }

    fn default_wire_api(&self) -> &str {
        BASELINE_WIRE_API
    }

    fn convert_tools(&self, tools: &[ToolSpec]) -> ToolsPayload {
        ToolsPayload::PromptGuided {
            instructions: build_tool_instructions_text(tools),
        }
    }

    async fn simple_chat(
        &self,
        message: &str,
        model: &str,
        temperature: Option<f64>,
    ) -> anyhow::Result<String> {
        self.chat_with_system(None, message, model, temperature)
            .await
    }

    async fn chat_with_system(
        &self,
        system_prompt: Option<&str>,
        message: &str,
        model: &str,
        temperature: Option<f64>,
    ) -> anyhow::Result<String>;

    async fn list_models(&self) -> anyhow::Result<Vec<String>> {
        anyhow::bail!("live model listing is not supported for this provider")
    }

    async fn chat_with_history(
        &self,
        messages: &[ChatMessage],
        model: &str,
        temperature: Option<f64>,
    ) -> anyhow::Result<String> {
        let system = messages
            .iter()
            .find(|m| m.role == "system")
            .map(|m| m.content.as_str());
        let last_user = messages
            .iter()
            .rfind(|m| m.role == "user")
            .map(|m| m.content.as_str())
            .unwrap_or("");
        self.chat_with_system(system, last_user, model, temperature)
            .await
    }

    async fn chat(
        &self,
        request: ChatRequest<'_>,
        model: &str,
        temperature: Option<f64>,
    ) -> anyhow::Result<ChatResponse> {
        if let Some(tools) = request.tools
            && !tools.is_empty()
            && !self.supports_native_tools()
        {
            let tool_instructions = match self.convert_tools(tools) {
                ToolsPayload::PromptGuided { instructions } => instructions,
                payload => {
                    anyhow::bail!(
                        "Provider returned non-prompt-guided tools payload ({payload:?}) while supports_native_tools() is false"
                    )
                }
            };
            let mut modified_messages = request.messages.to_vec();
            if let Some(system_message) = modified_messages.iter_mut().find(|m| m.role == "system")
            {
                if !system_message.content.is_empty() {
                    system_message.content.push_str("\n\n");
                }
                system_message.content.push_str(&tool_instructions);
            } else {
                modified_messages.insert(0, ChatMessage::system(tool_instructions));
            }
            let text = self
                .chat_with_history(&modified_messages, model, temperature)
                .await?;
            return Ok(ChatResponse {
                text: Some(text),
                tool_calls: Vec::new(),
                usage: None,
                reasoning_content: None,
            });
        }
        let text = self
            .chat_with_history(request.messages, model, temperature)
            .await?;
        Ok(ChatResponse {
            text: Some(text),
            tool_calls: Vec::new(),
            usage: None,
            reasoning_content: None,
        })
    }

    fn supports_native_tools(&self) -> bool {
        self.capabilities().native_tool_calling
    }

    fn supports_vision(&self) -> bool {
        self.capabilities().vision
    }

    async fn warmup(&self) -> anyhow::Result<()> {
        Ok(())
    }

    async fn chat_with_tools(
        &self,
        messages: &[ChatMessage],
        _tools: &[serde_json::Value],
        model: &str,
        temperature: Option<f64>,
    ) -> anyhow::Result<ChatResponse> {
        let text = self.chat_with_history(messages, model, temperature).await?;
        Ok(ChatResponse {
            text: Some(text),
            tool_calls: Vec::new(),
            usage: None,
            reasoning_content: None,
        })
    }

    fn supports_streaming(&self) -> bool {
        false
    }

    fn supports_streaming_tool_events(&self) -> bool {
        false
    }

    fn stream_chat_with_system(
        &self,
        _system_prompt: Option<&str>,
        _message: &str,
        _model: &str,
        _temperature: Option<f64>,
        _options: StreamOptions,
    ) -> stream::BoxStream<'static, StreamResult<StreamChunk>> {
        stream::empty().boxed()
    }

    fn stream_chat_with_history(
        &self,
        messages: &[ChatMessage],
        model: &str,
        temperature: Option<f64>,
        options: StreamOptions,
    ) -> stream::BoxStream<'static, StreamResult<StreamChunk>> {
        let system = messages
            .iter()
            .find(|m| m.role == "system")
            .map(|m| m.content.as_str());
        let last_user = messages
            .iter()
            .rfind(|m| m.role == "user")
            .map(|m| m.content.as_str())
            .unwrap_or("");
        self.stream_chat_with_system(system, last_user, model, temperature, options)
    }

    fn stream_chat(
        &self,
        request: ChatRequest<'_>,
        model: &str,
        temperature: Option<f64>,
        options: StreamOptions,
    ) -> stream::BoxStream<'static, StreamResult<StreamEvent>> {
        self.stream_chat_with_history(request.messages, model, temperature, options)
            .map(|chunk_result| chunk_result.map(StreamEvent::from_chunk))
            .boxed()
    }
}

/// Blanket impl: `Arc<T>` delegates to `T`.
#[async_trait]
impl<T: Provider + ?Sized> Provider for Arc<T> {
    fn capabilities(&self) -> ProviderCapabilities {
        self.as_ref().capabilities()
    }
    fn default_temperature(&self) -> f64 {
        self.as_ref().default_temperature()
    }
    fn default_max_tokens(&self) -> u32 {
        self.as_ref().default_max_tokens()
    }
    fn default_timeout_secs(&self) -> u64 {
        self.as_ref().default_timeout_secs()
    }
    fn default_base_url(&self) -> Option<&str> {
        self.as_ref().default_base_url()
    }
    fn default_wire_api(&self) -> &str {
        self.as_ref().default_wire_api()
    }
    fn convert_tools(&self, tools: &[ToolSpec]) -> ToolsPayload {
        self.as_ref().convert_tools(tools)
    }
    fn supports_native_tools(&self) -> bool {
        self.as_ref().supports_native_tools()
    }
    fn supports_vision(&self) -> bool {
        self.as_ref().supports_vision()
    }

    async fn chat_with_system(
        &self,
        system_prompt: Option<&str>,
        message: &str,
        model: &str,
        temperature: Option<f64>,
    ) -> anyhow::Result<String> {
        self.as_ref()
            .chat_with_system(system_prompt, message, model, temperature)
            .await
    }

    async fn chat_with_history(
        &self,
        messages: &[ChatMessage],
        model: &str,
        temperature: Option<f64>,
    ) -> anyhow::Result<String> {
        self.as_ref()
            .chat_with_history(messages, model, temperature)
            .await
    }

    async fn chat(
        &self,
        request: ChatRequest<'_>,
        model: &str,
        temperature: Option<f64>,
    ) -> anyhow::Result<ChatResponse> {
        self.as_ref().chat(request, model, temperature).await
    }

    async fn warmup(&self) -> anyhow::Result<()> {
        self.as_ref().warmup().await
    }

    async fn chat_with_tools(
        &self,
        messages: &[ChatMessage],
        tools: &[serde_json::Value],
        model: &str,
        temperature: Option<f64>,
    ) -> anyhow::Result<ChatResponse> {
        self.as_ref()
            .chat_with_tools(messages, tools, model, temperature)
            .await
    }

    fn supports_streaming(&self) -> bool {
        self.as_ref().supports_streaming()
    }
    fn supports_streaming_tool_events(&self) -> bool {
        self.as_ref().supports_streaming_tool_events()
    }

    fn stream_chat_with_system(
        &self,
        system_prompt: Option<&str>,
        message: &str,
        model: &str,
        temperature: Option<f64>,
        options: StreamOptions,
    ) -> stream::BoxStream<'static, StreamResult<StreamChunk>> {
        self.as_ref()
            .stream_chat_with_system(system_prompt, message, model, temperature, options)
    }

    fn stream_chat_with_history(
        &self,
        messages: &[ChatMessage],
        model: &str,
        temperature: Option<f64>,
        options: StreamOptions,
    ) -> stream::BoxStream<'static, StreamResult<StreamChunk>> {
        self.as_ref()
            .stream_chat_with_history(messages, model, temperature, options)
    }

    fn stream_chat(
        &self,
        request: ChatRequest<'_>,
        model: &str,
        temperature: Option<f64>,
        options: StreamOptions,
    ) -> stream::BoxStream<'static, StreamResult<StreamEvent>> {
        self.as_ref()
            .stream_chat(request, model, temperature, options)
    }
}

/// Build tool instructions text for prompt-guided tool calling.
pub fn build_tool_instructions_text(tools: &[ToolSpec]) -> String {
    let mut instructions = String::new();
    instructions.push_str("## Tool Use Protocol\n\n");
    instructions.push_str("To use a tool, wrap a JSON object in ◁ tags:\n\n");
    instructions.push_str("◁\n");
    instructions.push_str(r#"{"name": "tool_name", "arguments": {"param": "value"}}"#);
    instructions.push_str("\n▷\n\n");
    instructions.push_str("You may use multiple tool calls in a single response. ");
    instructions.push_str("After tool execution, results appear in <tool_result> tags. ");
    instructions
        .push_str("Continue reasoning with the results until you can give a final answer.\n\n");
    instructions.push_str("### Available Tools\n\n");
    for tool in tools {
        writeln!(&mut instructions, "**{}**: {}", tool.name, tool.description)
            .expect("writing to String cannot fail");
        let parameters =
            serde_json::to_string(&tool.parameters).unwrap_or_else(|_| "{}".to_string());
        writeln!(&mut instructions, "Parameters: `{parameters}`")
            .expect("writing to String cannot fail");
        instructions.push('\n');
    }
    instructions
}
