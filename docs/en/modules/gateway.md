# clawseed-gateway — HTTP/WebSocket Gateway

## Overview

`clawseed-gateway` is built on the Axum framework, providing HTTP/REST and WebSocket endpoints. It serves as the entry point for external clients to interact with the agent. It also handles remote tool bridging — wrapping client-registered tools as `RemoteTool` instances.

## Architecture

```
┌──────────────────────────────────────────────────┐
│                   Gateway                         │
│                                                   │
│  ┌────────────┐  ┌────────────┐  ┌────────────┐ │
│  │  REST API  │  │  WebSocket │  │  Static    │ │
│  │  (api.rs)  │  │  (ws.rs)   │  │  Files     │ │
│  └─────┬──────┘  └─────┬──────┘  └────────────┘ │
│        │               │                         │
│  ┌─────┴───────────────┴──────────────────────┐ │
│  │           Middleware Layer                   │ │
│  │  CORS · Body limit (64KB) · Timeout (30s)  │ │
│  │  Tracing · Rate limiting · Auth             │ │
│  └─────────────────────────────────────────────┘ │
│                                                   │
│  ┌─────────────────────────────────────────────┐ │
│  │           Session Storage                    │ │
│  │  SQLite (session_sqlite.rs)                  │ │
│  │  In-memory queue (session_queue.rs, backup) │ │
│  └─────────────────────────────────────────────┘ │
│                                                   │
│  ┌─────────────────────────────────────────────┐ │
│  │           Remote Tool Bridge                 │ │
│  │  RemoteTool (remote_tool.rs)                 │ │
│  └─────────────────────────────────────────────┘ │
└──────────────────────────────────────────────────┘
```

## Core Modules

### ws.rs — WebSocket Endpoint

The primary communication channel, supporting the following message types:

**Client → Server**:
- `{"type": "message", "content": "..."}` — Send a chat message
- `{"type": "register_tools", "tools": [...]}` — Register client tools
- `{"type": "tool_result", "call_id": "...", "output": "..."}` — Return tool execution result
- `{"type": "tool_error", "call_id": "...", "error": "..."}` — Return tool execution error

**Server → Client**:
- `session_start` — Session started
- `chunk` — Streaming text chunk
- `thinking` — Agent thinking process
- `tool_call` — Tool call notification
- `tool_call_request` — Request client to execute a remote tool
- `done` — Turn completed
- `result_acknowledged` — Result received acknowledgment
- `aborted` — Turn aborted
- `error` — Error notification

### api.rs — REST Endpoints

- `GET /health` — Health check
- `GET /api/tools` — List registered tools (via `tool_registry.tool_specs()`)
- `POST /sessions` — Create session
- `GET /sessions/{id}` — Get session
- `POST /webhook` — Webhook ingestion
- `GET /api/doctor` — System diagnostics (tool count via `tool_registry.len()`)

### remote_tool.rs — Remote Tool Bridge

Wraps client-registered tools as `RemoteTool`, implementing the `Tool` trait:

```rust
impl Tool for RemoteTool {
    fn name(&self) -> &str { &self.spec.name }
    fn description(&self) -> &str { &self.spec.description }
    fn parameters_schema(&self) -> Value { self.spec.parameters.clone() }

    async fn execute(&self, args: Value, _ctx: &dyn ToolContext) -> Result<ToolResult> {
        // 1. Generate call_id
        // 2. Send tool_call_request to client
        // 3. Wait for tool_result or tool_error (30s timeout)
        // 4. Return result
    }
}
```

**Note**: Remote tools do not use `ToolContext` (no access to server-side memory, security policy, etc.).

### Session Management

- `session_backend.rs` — `SessionBackend` trait
- `session_sqlite.rs` — SQLite persistence backend (default)
- `session_queue.rs` — In-memory queue backend (fallback)

### Security and Rate Limiting

- `auth_rate_limit.rs` — Sliding window rate limiting (per IP/token)
- `tls.rs` — TLS/HTTPS support

### Static Files

- `static_files.rs` — Static asset serving

## Configuration Constants

| Constant | Value | Description |
|----------|-------|-------------|
| `MAX_BODY_SIZE` | 64KB | Request body size limit |
| `REQUEST_TIMEOUT_SECS` | 30 | Request timeout (overridable via `CLAWSEED_GATEWAY_TIMEOUT_SECS` env var) |
| `REMOTE_TOOL_TIMEOUT` | 30s | Remote tool execution timeout |
