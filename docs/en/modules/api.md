# clawseed-api — Core Trait Definitions

## Overview

`clawseed-api` is the foundational crate of the entire project — **zero implementation, trait definitions and shared types only**. All other crates depend on it, while it depends on no business crate.

**Core rule**: Extensions import api; api never imports extensions.

## Core Traits

### Provider — LLM Inference Backend

```rust
#[async_trait]
pub trait Provider: Send + Sync {
    async fn chat_with_system(&self, system_prompt: Option<&str>, message: &str, model: &str, temperature: Option<f64>) -> Result<String>;
    async fn chat(&self, request: ChatRequest<'_>, model: &str, temperature: Option<f64>) -> Result<ChatResponse>;
    fn supports_native_tools(&self) -> bool;
    fn stream_chat(&self, request: ChatRequest<'_>, model: &str, temperature: Option<f64>, options: StreamOptions) -> BoxStream<'static, StreamResult<StreamEvent>>;
    fn capabilities(&self) -> ProviderCapabilities;
    // ... more methods with defaults
}
```

- `chat_with_system()` — Core chat method; all other chat methods delegate to this
- `chat()` — Full chat with tool specs; returns `ChatResponse` with optional tool calls
- `supports_native_tools()` — Whether the provider supports native tool calling protocol (e.g., Anthropic's tool_use)
- `stream_chat()` — Streaming variant returning `BoxStream<StreamEvent>`
- `capabilities()` — Returns `ProviderCapabilities` including `CacheStrategy`
- Providers without native tool support use `XmlToolDispatcher` (◁▷ markers)

**CacheStrategy** — How a provider handles prompt caching:

```rust
pub enum CacheStrategy {
    #[default]
    None,                // No explicit markers; automatic prefix caching with stable prompts
    ExplicitAnthropic,   // Anthropic-style cache_control:ephemeral or Bedrock CachePoint
}

pub struct ProviderCapabilities {
    pub native_tool_calling: bool,
    pub vision: bool,
    pub cache_strategy: CacheStrategy,
}
```

- `None` — Default. The system prompt is 100% stable across turns, so providers with automatic prefix caching (DeepSeek, OpenAI, Groq) work without any message-level transformation.
- `ExplicitAnthropic` — Anthropic and Bedrock inject `cache_control: ephemeral` markers or `CachePoint` blocks within system messages. Used by Anthropic, Bedrock, and DeepSeek-anthropic providers.

### Tool — Agent-Callable Capability

```rust
#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn parameters_schema(&self) -> serde_json::Value;
    async fn execute(&self, args: serde_json::Value, ctx: &dyn ToolContext) -> anyhow::Result<ToolResult>;

    fn spec(&self) -> ToolSpec { /* default implementation */ }
}
```

- `name()` — Unique tool identifier
- `description()` — Tool description for the LLM
- `parameters_schema()` — JSON Schema parameter definition
- `execute()` — Execute tool logic, access runtime capabilities via `ToolContext`
- `spec()` — Generate `ToolSpec` for LLM registration

### Hook — Tool Call Interceptor

```rust
pub trait Hook: Send + Sync {
    fn before_tool_call(&self, call: &mut ToolCall) -> HookResult;
    fn after_tool_call(&self, result: &ToolExecutionResult) -> HookResult;
}
```

- `before_tool_call()` — Called before tool execution; can cancel, modify, or allow
- `after_tool_call()` — Called after tool execution; for observation and auditing

```rust
pub enum HookResult {
    Continue,              // Allow execution
    Cancel(String),        // Cancel execution with a reason
    Modify(ToolCall),      // Modify the tool call (name/arguments)
}
```

### Memory — Conversation Memory Backend

```rust
#[async_trait]
pub trait Memory: Send + Sync {
    fn name(&self) -> &str;
    async fn store(&self, key: &str, content: &str, category: MemoryCategory, session_id: Option<&str>) -> Result<()>;
    async fn recall(&self, query: &str, limit: usize, session_id: Option<&str>, since: Option<&str>, until: Option<&str>) -> Result<Vec<MemoryEntry>>;
    async fn get(&self, key: &str) -> Result<Option<MemoryEntry>>;
    async fn list(&self, category: Option<&MemoryCategory>, session_id: Option<&str>) -> Result<Vec<MemoryEntry>>;
    async fn forget(&self, key: &str) -> Result<bool>;
    async fn count(&self) -> Result<usize>;
    async fn health_check(&self) -> bool;
    // ... more methods with defaults: purge_namespace, purge_session, recall_namespaced, export, store_with_metadata
}
```

### ToolRegistry — Unified Tool Registration

```rust
pub trait ToolRegistry: Send + Sync {
    fn register(&self, tool: Box<dyn Tool>, source: ToolSource) -> bool;
    fn unregister(&self, name: &str) -> bool;
    fn get_tool(&self, name: &str) -> Option<Arc<dyn Tool>>;
    fn tool_specs(&self) -> Vec<ToolSpec>;
    fn get_entry(&self, name: &str) -> Option<ToolEntry>;
    fn tool_names(&self) -> Vec<String>;
    fn register_or_replace(&self, tool: Box<dyn Tool>, source: ToolSource) -> Option<ToolEntry>;
    fn len(&self) -> usize;
    fn is_empty(&self) -> bool;
}
```

- `register()` — Register a tool; returns false if one with the same name already exists
- `unregister()` — Remove a tool by name
- `get_tool()` — Look up by name, returns `Arc<dyn Tool>` (safe to share across async contexts)
- `tool_specs()` — Get all tool specs (cached) for LLM registration
- `register_or_replace()` — Register or replace a tool with the same name (used for remote tool reconnection)

```rust
/// Provenance of a registered tool
pub enum ToolSource {
    BuiltIn,                        // Built-in tool
    Mcp { server: String },         // Tool from an MCP server
    Remote { session: String },     // Tool registered by a remote client
}

/// Tool entry metadata
pub struct ToolEntry {
    pub source: ToolSource,
}
```

## Shared Types

### Message Types

```rust
pub struct ChatMessage {
    pub role: String,        // "system" / "user" / "assistant" / "tool"
    pub content: String,
}

pub struct ChatResponse {
    pub text: Option<String>,
    pub tool_calls: Vec<ToolCall>,
    pub usage: Option<TokenUsage>,
    pub reasoning_content: Option<String>,
}
```

### Tool Types

```rust
// From provider.rs — LLM-requested tool call
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: String,   // JSON string
}

// From hook.rs — hook interception tool call
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: Value,    // Parsed JSON value
}

pub struct ToolResult {
    pub success: bool,
    pub output: String,
    pub error: Option<String>,
}

pub struct ToolSpec {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}
```

## Task-Local Storage

`clawseed-api` uses task-local variables for cross-call context:

- `TOOL_CHOICE_OVERRIDE` — Override tool selection strategy

## Dependencies

Only depends on:
- `async-trait` — Async trait support
- `serde` / `serde_json` — Serialization
- `anyhow` — Error handling
- `tokio` — Task-local storage
- `futures-util` — Stream types
