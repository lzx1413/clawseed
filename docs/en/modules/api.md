# clawseed-api — Core Trait Definitions

## Overview

`clawseed-api` is the foundational crate of the entire project — **zero implementation, trait definitions and shared types only**. All other crates depend on it, while it depends on no business crate.

**Core rule**: Extensions import api; api never imports extensions.

## Core Traits

### Provider — LLM Inference Backend

```rust
#[async_trait]
pub trait Provider: Send + Sync {
    async fn chat(&self, request: ChatRequest) -> Result<ChatResponse>;
    fn supports_native_tools(&self) -> bool;
}
```

- `chat()` — Send a chat request, return the LLM response
- `supports_native_tools()` — Whether the provider supports native tool calling protocol (e.g., Anthropic's tool_use)
- Providers without native tool support use `XmlToolDispatcher` (◁▷ markers)

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
    async fn store(&self, content: &str, category: &str) -> Result<String>;
    async fn recall(&self, query: &str, limit: usize) -> Result<Vec<MemoryEntry>>;
    async fn forget(&self, id: &str) -> Result<()>;
    async fn purge(&self, before: DateTime<Utc>) -> Result<usize>;
    async fn export(&self) -> Result<Vec<MemoryEntry>>;
}
```

### Observer — Metrics and Tracing

```rust
pub trait Observer: Send + Sync {
    fn on_event(&self, event: &Event);
}
```

### ContextProvider — Capability Injection

```rust
pub trait ContextProvider: Send + Sync {
    fn provided_type_id(&self) -> TypeId;
    fn into_any_arc(self: Box<Self>) -> Arc<dyn Any + Send + Sync>;
}
```

Tools look up injected capabilities via `ctx.get::<T>()`.

## Shared Types

### Message Types

```rust
pub struct ChatMessage {
    pub role: Role,        // System / User / Assistant / Tool
    pub content: String,
    pub tool_calls: Option<Vec<ToolCall>>,
    pub tool_call_id: Option<String>,
}

pub struct ChatResponse {
    pub message: ChatMessage,
    pub usage: Option<Usage>,
    pub stop_reason: Option<StopReason>,
}
```

### Tool Types

```rust
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: Value,
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

- `TOOL_LOOP_THREAD_ID` — Current tool loop thread identifier
- `TOOL_CHOICE_OVERRIDE` — Override tool selection strategy
- `TOOL_LOOP_SESSION_KEY` — Current session key

## Dependencies

Only depends on:
- `async-trait` — Async trait support
- `serde` / `serde_json` — Serialization
- `anyhow` — Error handling
