# ClawSeed 整体架构

## 概述

ClawSeed 是一个用 Rust 编写的 AI Agent 运行时。它连接 LLM 提供商（Anthropic、Gemini、Bedrock、OpenAI 兼容端点等），通过可插拔的工具（Tool）执行操作，并通过 HTTP/WebSocket 为客户端提供服务。

核心设计理念：**trait 在边界，实现在外部，核心永不修改**。

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
    ├← clawseed-parser      （消息解析）
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
| `Provider` | LLM 推理后端 | 在 `clawseed-providers` 中实现 |
| `Tool` | Agent 可调用的能力 | 在 `clawseed-tools` 中实现，或通过 WebSocket 注册远程工具 |
| `Hook` | 工具调用拦截器 | 实现 `before_tool_call` / `after_tool_call` |
| `Memory` | 对话记忆后端 | 在 `clawseed-memory` 中实现 |
| `Observer` | 指标和追踪 | 实现 `on_event()` |
| `ContextProvider` | 能力注入 | 将 `Send + Sync + 'static` 类型注入 Agent |

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

## 安全模型

- **自主等级**：`ReadOnly`（只读）/ `Supervised`（需审批）/ `Full`（完全自主）
- **SecurityPolicy**：作为能力注入，工具通过 `ctx.get::<SecurityPolicy>()` 检查权限
- **命令白名单**：`allowed_commands` 验证 shell 命令
- **路径守卫**：阻止访问敏感路径（`/etc/passwd`、`/root/.ssh` 等）
- **速率限制**：`max_actions_per_hour` 限制每会话操作数
- **Hook 管线**：`Hook::before_tool_call()` 可取消或修改任何工具调用

## 设计原则

1. **显式优于隐式** — `all_tools()` 列出每个工具，能力集一目了然
2. **声明式优于命令式** — 配置驱动组合，而非代码修改
3. **trait 在边界** — 核心依赖抽象，实现在外部
4. **优雅降级** — 缺少能力 → 工具跳过功能；内存失败 → NoneMemory 兜底；提供者不稳定 → ReliableProvider 重试

## Crate 一览

| Crate | 职责 | 依赖 api | 依赖 agent |
|-------|------|:---------:|:----------:|
| `clawseed-api` | 仅 trait 定义 | — | — |
| `clawseed-agent` | Agent 循环、Hook、调度 | yes | — |
| `clawseed-tools` | 25+ 内置工具 | yes | no |
| `clawseed-providers` | LLM 提供商实现 | yes | no |
| `clawseed-memory` | SQLite 存储 + 向量搜索 | yes | no |
| `clawseed-config` | TOML 配置加载 | yes | no |
| `clawseed-parser` | 工具调用解析 | yes | no |
| `clawseed-macros` | 过程宏 | no | no |
| `clawseed-gateway` | Axum HTTP/WS 服务器 + 远程工具桥 | yes | yes |
| `clawseed` | 二进制（CLI） | — | — |
