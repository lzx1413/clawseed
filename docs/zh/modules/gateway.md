# clawseed-gateway — HTTP/WebSocket 网关

## 概述

`clawseed-gateway` 基于 Axum 框架，提供 HTTP/REST 和 WebSocket 端点，是外部客户端与 Agent 交互的入口。它还负责远程工具桥接——将客户端注册的工具包装为 `RemoteTool`。

## 架构

```
┌──────────────────────────────────────────────────┐
│                   Gateway                         │
│                                                   │
│  ┌────────────┐  ┌────────────┐  ┌────────────┐ │
│  │  REST API  │  │  WebSocket │  │ 静态文件   │ │
│  │  (api.rs)  │  │  (ws.rs)   │  │ (static_)  │ │
│  └─────┬──────┘  └─────┬──────┘  └────────────┘ │
│        │               │                         │
│  ┌─────┴───────────────┴──────────────────────┐ │
│  │           中间件层                           │ │
│  │  CORS · 请求体限制(64KB) · 超时(30s)       │ │
│  │  追踪 · 速率限制 · 认证                     │ │
│  └─────────────────────────────────────────────┘ │
│                                                   │
│  ┌─────────────────────────────────────────────┐ │
│  │           会话存储                           │ │
│  │  SQLite (session_sqlite.rs)                  │ │
│  │  内存队列 (session_queue.rs, 兜底)          │ │
│  └─────────────────────────────────────────────┘ │
│                                                   │
│  ┌─────────────────────────────────────────────┐ │
│  │           远程工具桥接                       │ │
│  │  RemoteTool (remote_tool.rs)                 │ │
│  └─────────────────────────────────────────────┘ │
└──────────────────────────────────────────────────┘
```

## 核心模块

### ws.rs — WebSocket 端点

主要通信通道，支持以下消息类型：

每个 WebSocket 连接通过 `Agent::from_config_with_shared_components()` 创建独立的 Agent 实例，复用 `AppState` 中的共享 provider、memory、observer、model、temperature 和 BuiltIn 工具实例。每连接组件（hooks、dispatcher、skill index）仍独立创建；BuiltIn 工具通过 `register_all_arc()` 注册共享的 `Arc<dyn Tool>` 实例。运行时初始化链路详见[架构概览](../architecture.md)。

**客户端 → 服务器**：
- `{"type": "message", "content": "..."}` — 发送聊天消息
- `{"type": "register_tools", "tools": [...]}` — 注册客户端工具
- `{"type": "tool_result", "call_id": "...", "output": "..."}` — 返回工具执行结果
- `{"type": "tool_error", "call_id": "...", "error": "..."}` — 返回工具执行错误

**服务器 → 客户端**：
- `session_start` — 会话开始
- `chunk` — 流式文本块
- `thinking` — Agent 思考过程
- `tool_call` — 工具调用通知
- `tool_call_request` — 请求客户端执行远程工具
- `done` — 回合完成
- `result_acknowledged` — 确认结果已收到
- `aborted` — 回合中止
- `error` — 错误通知

### api.rs — REST 端点

#### 系统
- `GET /health` — 健康检查
- `GET /api/doctor` — 系统诊断（工具数量、记忆健康等）
- `GET /api/cost` — Token 费用指标

#### 工具与技能
- `GET /api/tools` — 列出注册的工具（通过 `tool_registry.tool_specs()` 获取）
- `GET /api/cli-tools` — 列出可用的 CLI 工具
- `POST /api/skills/reload` — 从磁盘重新读取技能索引，无需重启（返回 `{ ok, skills_count }`）

#### 会话
- `GET /api/sessions` — 列出所有会话
- `GET /api/sessions/running` — 获取运行中的会话
- `GET /api/sessions/{id}/messages` — 获取会话消息
- `GET /api/sessions/{id}/state` — 获取会话状态
- `PUT /api/sessions/{id}` — 重命名会话
- `DELETE /api/sessions/{id}` — 删除会话
- `POST /api/sessions/{id}/abort` — 中止运行中的会话

#### 记忆
- `GET /api/memory` — 列出记忆
- `POST /api/memory` — 存储新记忆
- `DELETE /api/memory/{key}` — 删除记忆

#### 用户画像
- `GET /api/users/me/profile` — 获取当前已认证本地用户的画像
- `POST /api/users/me/profile` — 创建或替换一个画像键
- `PATCH /api/users/me/profile/items/{id}` — 更新画像条目，或将 `status` 设为 `rejected`
- `DELETE /api/users/me/profile/items/{id}` — 删除画像条目
- `DELETE /api/users/me/profile` — 删除全部画像条目

当前本地 Gateway 将已认证连接映射到稳定的 `owner` 主体。会话首次使用时绑定所有者，
之后不能重新分配。
拒绝推断条目会保留其来源信息，并阻止自动推断再次覆盖相同键；手动编辑条目会将其标记为
显式设置并恢复为生效状态。

#### 定时任务
- `GET /api/cron` — 列出任务
- `POST /api/cron` — 添加任务
- `PATCH /api/cron/{id}` — 更新任务
- `DELETE /api/cron/{id}` — 删除任务
- `GET /api/cron/{id}/runs` — 任务执行历史
- `GET /api/cron/settings` — 定时任务设置
- `PATCH /api/cron/settings` — 更新定时任务设置

#### 人格与配置
- `GET /api/personality` — 读取工作区的人格文件（SOUL.md 等）
- `PUT /api/personality` — 写入人格文件（白名单验证）
- `GET /api/config` — 获取 TOML 配置
- `PUT /api/config` — 更新配置（返回警告：provider/model/memory 变更需要重启网关）
- `GET /api/provider/models` — 通过网关代理获取可用模型列表

#### Webhook
- `POST /webhook` — Webhook 接收（消息持久化到会话存储，返回 session_id）

### remote_tool.rs — 远程工具桥接

将客户端注册的工具包装为 `RemoteTool`，实现 `Tool` trait。远程工具注册是三步流程：

1. **注册到共享注册表** — `state.tool_registry.register_or_replace(tool, ToolSource::Remote { session })`，使 `/api/tools` 全局可见
2. **注入到当前连接的 Agent** — `agent.add_remote_tools(tools, session)`，在处理每条消息前注入
3. **断连清理** — `state.tool_registry.unregister_by_source(&ToolSource::Remote { session })`

这意味着共享注册表（`AppState.tool_registry`）和每个 Agent 的私有注册表（`Agent.tool_registry`）是独立实例。影响详见[架构概览](../architecture.md)中的"双重工具注册表"一节。

```rust
impl Tool for RemoteTool {
    fn name(&self) -> &str { &self.spec.name }
    fn description(&self) -> &str { &self.spec.description }
    fn parameters_schema(&self) -> Value { self.spec.parameters.clone() }

    async fn execute(&self, args: Value, _ctx: &dyn ToolContext) -> Result<ToolResult> {
        // 1. 生成 call_id
        // 2. 发送 tool_call_request 到客户端
        // 3. 等待 tool_result 或 tool_error（30s 超时）
        // 4. 返回结果
    }
}
```

**注意**：远程工具不使用 `ToolContext`（无法访问服务端记忆、安全策略等）。

### 会话管理

- `session_backend.rs` — `SessionBackend` trait
- `session_sqlite.rs` — SQLite 持久化后端（默认）
- `session_queue.rs` — 内存队列后端（兜底）

### 安全与限流

- `auth_rate_limit.rs` — 滑动窗口速率限制（per IP/token）
- `tls.rs` — TLS/HTTPS 支持

### 静态文件

- `static_files.rs` — 静态资源服务

## 配置常量

| 常量 | 值 | 说明 |
|------|-----|------|
| `MAX_BODY_SIZE` | 64KB | 请求体大小限制 |
| `REQUEST_TIMEOUT_SECS` | 30 | 请求超时（可通过 `CLAWSEED_GATEWAY_TIMEOUT_SECS` 环境变量覆盖；Android 默认：300s） |
| `REMOTE_TOOL_TIMEOUT` | 30s | 远程工具执行超时 |
