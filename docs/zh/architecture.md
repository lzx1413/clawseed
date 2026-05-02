# ClawSeed 整体架构

## 概述

ClawSeed 是一个用 Rust 编写的 AI Agent 运行时。它连接 LLM 提供商（Anthropic、Gemini、Bedrock、OpenAI 兼容端点等），通过可插拔的工具（Tool）执行操作，并通过 HTTP/WebSocket 为客户端提供服务。

核心设计理念：**是运行时，不是应用**。ClawSeed 提供 crate 供应用组装——它不捆绑渠道、面板或集成。详见下文[运行时 vs 应用](#运行时-vs-应用)。

## 运行时 vs 应用

一个 agent **运行时**应该只做三件事：接收消息、调用 LLM、执行工具。其他一切——消息从哪来、结果怎么展示、接入哪些集成——都属于应用层。

ClawSeed 是运行时。基于它构建的应用自己决定：

- 用户如何交互（CLI、手机 App、聊天机器人、Web 面板）
- 接入哪些渠道（Discord、Telegram、邮件——或者不接入）
- 暴露哪些工具（内置的、移动端远程的、自定义的）
- 如何处理安全和审批流程

```toml
# 一个 Discord 机器人应用
[dependencies]
clawseed-agent = "0.7"
clawseed-providers = "0.7"
serenity = "0.12"          # 应用自己选择 Discord SDK

# 一个 Android 应用
[dependencies]
clawseed-gateway = "0.7"
clawseed-agent = "0.7"

# 一个 CLI 工具
[dependencies]
clawseed-agent = "0.7"
clawseed-tools = "0.7"
```

这是 ClawSeed 与 ZeroClaw 最根本的架构分野。ZeroClaw 把 40+ 渠道适配器、硬件外设、TUI、Web 面板、SOP 引擎都塞进同一个二进制——这做的是应用，不是运行时。加一个新渠道要改运行时代码，加一个新集成要理解整个系统。

ClawSeed 的方式：**运行时提供稳定的 trait crate，应用自己组装。** 新需求来了，写一个新应用——不需要改运行时。

## 架构总览

```
┌──────────────────────────────────────────────────────────┐
│                  gateway (REST / WebSocket)               │
│                       ↓                                   │
│  ┌──────────────────────────────────────────────────┐    │
│  │              Agent (稳定核心)                      │    │
│  │     turn → LLM → dispatch → execute → loop       │    │
│  └──┬──────────┬──────────┬──────────┬─────────────┘    │
│     │          │          │          │                    │
│  provider    tools      memory    hooks                  │
│  (dyn)     (dyn)       (dyn)    (pipeline)               │
│     │          │          │          │                    │
│  Anthropic   25+        SQLite   security                │
│  Gemini      built-in   vector   audit                   │
│  Bedrock                search   approval                │
│  OpenAI*     + remote ──→ mobile client                  │
│  Ollama                                                  │
│  DeepSeek                                                │
│  Groq                                                    │
└──────────────────────────────────────────────────────────┘
   * 及任何 OpenAI 兼容端点
```

## 依赖关系

依赖流是单向的，形成清晰的分层架构：

```
clawseed-api（零依赖，仅 trait 定义）
    ↑
    ├← clawseed-tools      （工具实现）
    ├← clawseed-memory      （存储后端）
    ├← clawseed-providers   （LLM 提供商）
    └← clawseed-agent       （Agent 核心）
            ↑
            └← clawseed-config  （配置加载）
                    ↑
                    └← clawseed-gateway（HTTP/WS 服务器 + 远程工具桥接）
                            ↑
                            └← clawseed（二进制入口）
```

**关键规则**：`clawseed-api` 是唯一被广泛依赖的 crate，且它自身不依赖任何其他 crate。核心永远不导入扩展。

## 核心抽象

ClawSeed 的所有扩展点都是 trait：

| Trait | 作用 | 扩展方式 |
|-------|------|---------|
| `Provider` | LLM 推理后端 | 在 `clawseed-providers` 中实现，或通过 `ProviderFactory` 注册自定义工厂 |
| `Tool` | Agent 可调用的能力 | 在 `clawseed-tools` 中实现，或通过 WebSocket 注册远程工具 |
| `ToolRegistry` | 统一工具注册与查找 | 在 `clawseed-agent` 中提供 `DefaultToolRegistry`，支持 BuiltIn / MCP / Remote 三种来源 |
| `Hook` | 工具调用拦截器 | 实现 `before_tool_call` / `after_tool_call`，或通过 `HookFactory` 从配置声明式创建 |
| `Memory` | 对话记忆后端 | 在 `clawseed-memory` 中实现 |

## Agent 循环

Agent 的核心是一个 turn 循环，每次用户消息触发一次：

```
用户消息
  ↓
构建系统提示（prompt.rs）
  ↓
调用 LLM（Provider::chat()）
  ↓
解析响应（ToolDispatcher::parse_response()）
├── NativeToolDispatcher：直接从 provider 原生 tool_calls 提取
└── XmlToolDispatcher：先尝试 ◁▷ 格式，失败后 fallback 到多格式解析器（12+ 种格式）
    ├── 纯文本响应 → 返回给用户
    └── 包含工具调用 → 进入工具循环
        ↓
  对每个工具调用：
    1. before_hook 拦截（可取消/修改）
    2. Tool::execute() 执行
    3. after_hook 观察
        ↓
  将工具结果格式化，发送回 LLM
        ↓
  回到"解析响应"步骤，直到 LLM 返回纯文本
```

## 远程工具调用

移动客户端通过 WebSocket 注册工具，Gateway 将其包装为 `RemoteTool`（实现了 `Tool` trait），Agent 无需区分本地和远程工具：

```
┌──────────────┐     register_tools       ┌──────────────┐
│   Mobile     │ ───────────────────────→ │   Gateway    │
│   Client     │                          │              │
│              │ ←── tool_call_request ── │   Agent      │
│  (设备端     │ ──── tool_result ──────→ │   无差别     │
│   执行)      │                          │   调用       │
│              │ ←── result_acknowledged─ │              │
└──────────────┘                          └──────────────┘
```

## 能力注入机制

工具不通过构造函数获取依赖，而是通过 `ToolContext` 在运行时查找：

```rust
// 构建时注入
agent_builder.capability(Arc::new(my_service));

// 执行时查找
if let Some(svc) = ctx.get::<MyService>() {
    svc.do_thing();
}
```

底层使用 `TypeId` → `Arc<dyn Any>` 映射，无需泛型参数，解耦工具 trait 和扩展类型。

## 工具注册机制

Agent 通过 `ToolRegistry` trait（定义在 `clawseed-api`）统一管理所有来源的工具：

```rust
// 三种工具来源
pub enum ToolSource {
    BuiltIn,                        // 内置工具
    Mcp { server: String },         // MCP 服务器工具
    Remote { session: String },     // 远程客户端工具（如 Android）
}

// 注册与查找
registry.register(tool, ToolSource::BuiltIn);
registry.register_or_replace(tool, ToolSource::Remote { session });
let tool = registry.get_tool("shell");
let specs = registry.tool_specs();  // 带缓存的 ToolSpec 列表
```

`DefaultToolRegistry`（在 `clawseed-agent` 中）使用 `DashMap` 实现无锁并发访问，支持 glob 模式的工具过滤（`allowed_tools` / `denied_tools`）和按 MCP 服务器过滤。

## Provider 工厂机制

Provider 通过 `ProviderFactory` trait + `ProviderFactoryRegistry` 注册：

```rust
// 自定义 Provider 工厂
impl ProviderFactory for MyFactory {
    fn name(&self) -> &str { "my-provider" }
    fn aliases(&self) -> &[&str] { &["my-alias"] }
    fn create(&self, name: &str, api_key: Option<&str>,
              base_url: Option<&str>, options: &ProviderRuntimeOptions
    ) -> Result<Box<dyn Provider>> { /* ... */ }
}

// 注册到 registry
let mut reg = ProviderFactoryRegistry::new();
reg.register(MyFactory);

// 使用自定义 registry 创建 Agent
Agent::from_config_with_registry(&config, Some(Arc::new(reg))).await?;
```

替代了原来 300+ 行的 match 链，Android/嵌入式场景可传入最小化的 provider 集合。

## 安全模型

- **自主等级**：`ReadOnly`（只读）/ `Supervised`（需审批）/ `Full`（完全自主）
- **SecurityPolicy**：作为 Hook 注入，实现 `Hook` trait 在工具执行前全局拦截（检查自主等级、速率限制、命令白名单、路径守卫），始终作为管线的第一个 Hook
- **命令白名单**：`allowed_commands` 验证 shell 命令
- **路径守卫**：阻止访问敏感路径（`/etc/passwd`、`/root/.ssh` 等）
- **速率限制**：`max_actions_per_hour` 限制每会话操作数
- **Hook 管线**：`Hook::before_tool_call()` 可取消或修改任何工具调用；SecurityPolicy 始终作为管线的第一个 Hook
- **工具过滤**：`allowed_tools` / `denied_tools` glob 模式过滤，`mcp_tool_filters` 按 MCP 服务器过滤

## 会话历史管理

每次 agent turn 都会向对话历史（`Vec<ChatMessage>`）追加消息，并在每次请求时发送给 LLM。无限增长的历史会导致 token 溢出和成本失控，因此 agent 会自动裁剪：

- **`trim_history()`** — 当历史消息数超过 `max_history`（默认 50）时，删除最早的非 system 消息，始终保留位置 0 的 system prompt
- **`truncate_tool_result()`** — 将过大的工具输出截断到 `max_chars`，保留头部（2/3）和尾部（1/3），中间插入 `[... N characters truncated ...]` 标记
- **`estimate_history_tokens()`** — 粗略估算 token 数（每条消息 `content.len() / 4 + 4`），用于预算决策

```
System prompt（始终保留）
  ↓
用户消息 ─→ LLM 响应 ─→ 工具结果 ─→ ...
  ↑                                      │
  └──── trim_history() 删除最早的消息 ───┘
```

这确保长时间运行的会话保持在 token 预算内，同时不丢失 system prompt。

## 记忆系统

历史（History）是发送给 LLM 的短期对话上下文；记忆（Memory）是跨会话持久化的长期知识存储。它们服务于不同目的：

| | 历史（History） | 记忆（Memory） |
|---|---------|--------|
| **范围** | 当前会话 | 跨会话，持久化 |
| **存储** | 内存 `Vec<ChatMessage>` | SQLite 数据库 |
| **生命周期** | 会话结束时清除 | 重启后依然存在 |
| **访问方式** | 自动（每轮发送给 LLM） | 显式（工具调用 `memory.recall()`） |
| **内容** | 完整对话文本 | 带元数据的结构化条目 |

记忆由 `clawseed-memory` 实现，遵循 `clawseed-api` 中的 `Memory` trait：

```
┌─────────────────────────────────────┐
│            Memory trait              │
│  store / recall / get / list /      │
│  forget / count / health_check      │
└─────────────┬───────────────────────┘
              │
     ┌────────┴────────┐
     │                  │
┌────┴─────┐     ┌─────┴──────┐
│SqliteMemory│    │ NoneMemory │
│  (默认)    │    │  (兜底)    │
└────┬──────┘    └────────────┘
     │
┌────┴──────────────────────────────┐
│            检索引擎                │
│  ┌──────────────┐ ┌─────────────┐ │
│  │  向量相似度   │ │  BM25 关键词│ │
│  │  (embedding) │ │    搜索     │ │
│  └──────┬───────┘ └──────┬──────┘ │
│         └────┬───────────┘        │
│              ↓                     │
│         混合排序 (Hybrid)          │
└────────────────────────────────────┘
```

核心特性：
- **混合检索**：向量相似度（语义）与 BM25（关键词）加权融合，由 `SearchMode` 枚举控制（`Hybrid` / `Embedding` / `Bm25`）
- **记忆分类**：`Core`（持久化知识）、`Daily`（临时信息）、`Conversation`（对话上下文）、`Custom(String)`（用户自定义）
- **命名空间隔离**：`recall_namespaced()` 按命名空间过滤，支持多租户或按用户隔离
- **导出**：`export()` 配合 `ExportFilter` 支持按命名空间、会话、分类和时间范围过滤
- **优雅降级**：SQLite 初始化失败时使用 `NoneMemory` 作为无操作兜底——依赖记忆的工具直接跳过该功能

## 设计原则

1. **显式优于隐式** — `all_tools()` 列出每个工具，能力集一目了然
2. **声明式优于命令式** — 配置驱动组合，而非代码修改
3. **trait 在边界** — 核心依赖抽象，实现在外部
4. **优雅降级** — 缺少能力 → 工具跳过功能；内存失败 → NoneMemory 兜底；提供者不稳定 → ReliableProvider 重试

## Crate 一览

| Crate | 职责 | 依赖 api | 依赖 agent |
|-------|------|:---------:|:----------:|
| `clawseed-api` | 仅 trait 定义 | — | — |
| `clawseed-agent` | Agent 循环、Hook、调度、解析 | yes | — |
| `clawseed-tools` | 25+ 内置工具 | yes | no |
| `clawseed-providers` | LLM 提供商实现 | yes | no |
| `clawseed-memory` | SQLite 存储 + 向量搜索 | yes | no |
| `clawseed-config` | TOML 配置加载 | yes | no |
| `clawseed-gateway` | Axum HTTP/WS 服务器 + 远程工具桥 | yes | yes |
| `clawseed` | 二进制（CLI） | — | — |
