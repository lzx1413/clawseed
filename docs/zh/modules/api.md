# clawseed-api — 核心 Trait 定义

## 概述

`clawseed-api` 是整个项目的基石 crate，**零实现，仅定义 trait 和共享类型**。所有其他 crate 都依赖它，而它自身不依赖任何业务 crate。

**核心规则**：扩展导入 api，api 永远不导入扩展。

## 核心 Trait

### Provider — LLM 推理后端

```rust
#[async_trait]
pub trait Provider: Send + Sync {
    async fn chat(&self, request: ChatRequest) -> Result<ChatResponse>;
    fn supports_native_tools(&self) -> bool;
}
```

- `chat()` — 发送对话请求，返回 LLM 响应
- `supports_native_tools()` — 是否支持原生工具调用协议（如 Anthropic 的 tool_use）
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
    async fn store(&self, content: &str, category: &str) -> Result<String>;
    async fn recall(&self, query: &str, limit: usize) -> Result<Vec<MemoryEntry>>;
    async fn forget(&self, id: &str) -> Result<()>;
    async fn purge(&self, before: DateTime<Utc>) -> Result<usize>;
    async fn export(&self) -> Result<Vec<MemoryEntry>>;
}
```

### Observer — 指标和追踪

```rust
pub trait Observer: Send + Sync {
    fn on_event(&self, event: &Event);
}
```

### ContextProvider — 能力注入

```rust
pub trait ContextProvider: Send + Sync {
    fn provided_type_id(&self) -> TypeId;
    fn into_any_arc(self: Box<Self>) -> Arc<dyn Any + Send + Sync>;
}
```

工具通过 `ctx.get::<T>()` 查找注入的能力。

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

### 工具类型

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

## Task-Local 存储

`clawseed-api` 使用 task-local 变量传递跨调用上下文：

- `TOOL_LOOP_THREAD_ID` — 当前工具循环线程标识
- `TOOL_CHOICE_OVERRIDE` — 覆盖工具选择策略
- `TOOL_LOOP_SESSION_KEY` — 当前会话密钥

## 依赖

仅依赖：
- `async-trait` — 异步 trait 支持
- `serde` / `serde_json` — 序列化
- `anyhow` — 错误处理
