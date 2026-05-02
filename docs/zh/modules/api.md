# clawseed-api — 核心 Trait 定义

## 概述

`clawseed-api` 是整个项目的基石 crate，**零实现，仅定义 trait 和共享类型**。所有其他 crate 都依赖它，而它自身不依赖任何业务 crate。

**核心规则**：扩展导入 api，api 永远不导入扩展。

## 核心 Trait

### Provider — LLM 推理后端

```rust
#[async_trait]
pub trait Provider: Send + Sync {
    async fn chat_with_system(&self, system_prompt: Option<&str>, message: &str, model: &str, temperature: Option<f64>) -> Result<String>;
    async fn chat(&self, request: ChatRequest<'_>, model: &str, temperature: Option<f64>) -> Result<ChatResponse>;
    fn supports_native_tools(&self) -> bool;
    fn stream_chat(&self, request: ChatRequest<'_>, model: &str, temperature: Option<f64>, options: StreamOptions) -> BoxStream<'static, StreamResult<StreamEvent>>;
    // ... 更多带默认实现的方法
}
```

- `chat_with_system()` — 核心聊天方法，其他聊天方法均委托于此
- `chat()` — 完整对话，支持工具规格，返回 `ChatResponse`（可能包含工具调用）
- `supports_native_tools()` — 是否支持原生工具调用协议（如 Anthropic 的 tool_use）
- `stream_chat()` — 流式变体，返回 `BoxStream<StreamEvent>`
- 不支持原生工具的提供商使用 `XmlToolDispatcher`（◁▷ 标记）

### Tool — Agent 可调用的能力

```rust
#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn parameters_schema(&self) -> serde_json::Value;
    async fn execute(&self, args: serde_json::Value, ctx: &dyn ToolContext) -> anyhow::Result<ToolResult>;

    fn spec(&self) -> ToolSpec { /* 默认实现 */ }
}
```

- `name()` — 工具唯一标识
- `description()` — 给 LLM 的工具描述
- `parameters_schema()` — JSON Schema 格式的参数定义
- `execute()` — 执行工具逻辑，通过 `ToolContext` 访问运行时能力
- `spec()` — 生成 `ToolSpec`，供注册到 LLM 时使用

### Hook — 工具调用拦截器

```rust
pub trait Hook: Send + Sync {
    fn before_tool_call(&self, call: &mut ToolCall) -> HookResult;
    fn after_tool_call(&self, result: &ToolExecutionResult) -> HookResult;
}
```

- `before_tool_call()` — 在工具执行前调用，可取消、修改或放行
- `after_tool_call()` — 在工具执行后调用，用于观察和审计

```rust
pub enum HookResult {
    Continue,              // 放行
    Cancel(String),        // 取消执行，附带原因
    Modify(ToolCall),      // 修改工具调用（名称/参数）
}
```

### Memory — 对话记忆后端

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
    // ... 更多带默认实现的方法：purge_namespace, purge_session, recall_namespaced, export, store_with_metadata
}
```

### ToolRegistry — 统一工具注册

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

- `register()` — 注册工具，重名返回 false
- `unregister()` — 按名称移除工具
- `get_tool()` — 按名称查找，返回 `Arc<dyn Tool>`（在 async 上下文中安全共享）
- `tool_specs()` — 获取所有工具规格（带缓存），供 LLM 注册使用
- `register_or_replace()` — 注册或替换同名工具（远程工具重连时使用）

```rust
/// 工具来源标识
pub enum ToolSource {
    BuiltIn,                        // 内置工具
    Mcp { server: String },         // MCP 服务器提供的工具
    Remote { session: String },     // 远程客户端注册的工具
}

/// 工具条目元数据
pub struct ToolEntry {
    pub source: ToolSource,
}
```

## 共享类型

### 消息类型

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

### 工具类型

```rust
// provider.rs 中的 ToolCall — LLM 请求的工具调用
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: String,   // JSON 字符串
}

// hook.rs 中的 ToolCall — hook 拦截的工具调用
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: Value,    // 已解析的 JSON 值
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

## Task-Local 存储

`clawseed-api` 使用 task-local 变量传递跨调用上下文：

- `TOOL_CHOICE_OVERRIDE` — 覆盖工具选择策略

## 依赖

仅依赖：
- `async-trait` — 异步 trait 支持
- `serde` / `serde_json` — 序列化
- `anyhow` — 错误处理
- `tokio` — Task-local 存储
- `futures-util` — Stream 类型
