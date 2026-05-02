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

- `GET /health` — 健康检查
- `GET /api/tools` — 列出注册的工具（通过 `tool_registry.tool_specs()` 获取）
- `POST /sessions` — 创建会话
- `GET /sessions/{id}` — 获取会话
- `POST /webhook` — Webhook 接收
- `GET /api/doctor` — 系统诊断（工具数量通过 `tool_registry.len()` 获取）

### remote_tool.rs — 远程工具桥接

将客户端注册的工具包装为 `RemoteTool`，实现 `Tool` trait：

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
| `REQUEST_TIMEOUT_SECS` | 30 | 请求超时（可通过 `CLAWSEED_GATEWAY_TIMEOUT_SECS` 环境变量覆盖） |
| `REMOTE_TOOL_TIMEOUT` | 30s | 远程工具执行超时 |
