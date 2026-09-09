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

Each WebSocket connection creates its own Agent via `Agent::from_config_with_shared_components()`, reusing the shared provider, memory, observer, model, temperature, and BuiltIn tool instances from `AppState`. Per-connection components (hooks, dispatcher, skill index) are still created fresh; BuiltIn tools reuse shared `Arc<dyn Tool>` instances registered via `register_all_arc()`. See [Architecture Overview](../architecture.md) for the runtime init chain.

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

`api.rs` is the stable REST API facade. It retains bearer-token authentication,
shared entry points, and handler re-exports. Implementations live under `api/` and
are grouped by configuration, identity and personas, providers, skills, cron,
integrations, memory and user profiles, sessions, system status, and hooks. Domain
request DTOs and sensitive-config restoration stay with their handlers, while
`api/tests/` shares common fixtures and groups regression tests by domain. Routes
continue to reference `api::handle_*`, so the split does not change HTTP paths,
request/response shapes, or authentication behavior.

#### System
- `GET /health` — Health check
- `GET /api/doctor` — System diagnostics (tool count, memory health, etc.)
- `GET /api/cost` — Token cost metrics

#### Tools & Skills
- `GET /api/tools` — List registered tools (via `tool_registry.tool_specs()`)
- `GET /api/cli-tools` — List available CLI tools
- `POST /api/skills/reload` — Re-read skill index from disk without restarting (returns `{ ok, skills_count }`)

#### Sessions
- `GET /api/sessions` — List all sessions
- `GET /api/sessions/running` — Get running sessions
- `GET /api/sessions/{id}/messages` — Get session messages
- `GET /api/sessions/{id}/state` — Get session state
- `PUT /api/sessions/{id}` — Rename session
- `DELETE /api/sessions/{id}` — Delete session
- `POST /api/sessions/{id}/abort` — Abort running session

#### Memory
- `GET /api/memory` — List memories
- `POST /api/memory` — Store new memory
- `DELETE /api/memory/{key}` — Delete memory

#### User Profile
- `GET /api/users/me/profile` — Get the authenticated local user's profile
- `POST /api/users/me/profile` — Create or replace a profile key
- `PATCH /api/users/me/profile/items/{id}` — Update a profile item or set `status` to `rejected`
- `DELETE /api/users/me/profile/items/{id}` — Delete a profile item
- `DELETE /api/users/me/profile` — Delete all profile items
- `PUT /api/users/me/profile/import` — Atomically import profile items with `replace`, `merge`, or `append`
- `POST /api/users/me/profile/search` — Read-only profile search and summary input
- `POST /api/users/me/profile/change-plans` — Create a versioned, expiring change preview
- `POST /api/users/me/profile/change-plans/{id}/apply` — Apply a plan once
- `POST /api/users/me/profile/operations/{id}/undo` — Undo a retained operation
- `POST /api/users/me/knowledge/forget-plans` — Preview exact and possible profile/memory matches
- `POST /api/users/me/knowledge/forget-plans/{id}/apply` — Apply an explicit cross-store forget plan
- `GET /api/users/me/knowledge/legacy-report` — Read-only profile/Core duplicate report

Memory endpoints accept `namespace` or `persona` query fields. The default view combines the configured private namespace and `public`; an arbitrary unconfigured namespace is rejected with 403. Cross-store forget plans use a local operation journal. An interrupted `applying` operation is retried from its snapshots, memory deletion is compensated on failure, and profile deletion is the final atomic commit.

Profile plans are bound to the authenticated local user, expected profile version, TTL, and single-use state. Version conflicts return 409 and expired plans return 410.

The current local gateway maps authenticated connections to the stable `owner`
principal. Session ownership is bound on first use and cannot be reassigned.
Rejecting an inferred item preserves its provenance and prevents automatic inference
from replacing the same key. Editing an item makes it explicit and active.
Imported items use the `imported` source while retaining their confidence, status,
evidence session, and expiration metadata.

#### Cron Jobs
- `GET /api/cron` — List jobs
- `POST /api/cron` — Add job
- `PATCH /api/cron/{id}` — Update job
- `DELETE /api/cron/{id}` — Delete job
- `GET /api/cron/{id}/runs` — Job execution history
- `GET /api/cron/settings` — Cron settings
- `PATCH /api/cron/settings` — Update cron settings

#### Personality & Configuration
- `GET /api/personality` — Read personality files (SOUL.md, etc.) from workspace
- `PUT /api/personality` — Write personality files (allowlist-validated)
- `GET /api/config` — Get TOML configuration
- `PUT /api/config` — Update configuration (returns warning: provider/model/memory changes require gateway restart)
- `GET /api/provider/models` — Proxy fetch available models using configured API key

#### Webhook
- `POST /webhook` — Webhook ingestion (persists messages to session store, returns session_id)

### remote_tool.rs — Remote Tool Bridge

Wraps client-registered tools as `RemoteTool`, implementing the `Tool` trait. Remote tools follow a three-step registration flow:

1. **Register to shared registry** — `state.tool_registry.register_or_replace(tool, ToolSource::Remote { session })` so `/api/tools` reflects the tool globally
2. **Inject into per-connection Agent** — `agent.add_remote_tools(tools, session)` before processing each message
3. **Cleanup on disconnect** — `state.tool_registry.unregister_by_source(&ToolSource::Remote { session })`

This means the shared registry (`AppState.tool_registry`) and each agent's private registry (`Agent.tool_registry`) are separate instances. See the "Dual Tool Registry" section in [Architecture Overview](../architecture.md) for implications.

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
| `REQUEST_TIMEOUT_SECS` | 30 | Request timeout (overridable via `CLAWSEED_GATEWAY_TIMEOUT_SECS` env var; Android default: 300s) |
| `REMOTE_TOOL_TIMEOUT` | 30s | Remote tool execution timeout |

## Image attachment protocol

With session persistence enabled, `/api/status.image_attachments` declares protocol support and limits. `session_start.image_attachments_supported` also advertises support; absence means an older gateway. Establish a WebSocket session before uploading.

- `POST /api/sessions/{id}/attachments`: raw image bytes (not JSON or multipart); returns durable image metadata with HTTP 201.
- `GET /api/sessions/{id}/attachments/{attachment_id}`: authenticated image bytes with their verified MIME type.
- WebSocket message: `{"type":"message","content":"","attachments":[{"type":"image","id":"att_..."}]}`. Images allow an empty text body.
- History entries include `attachments`; they never contain encoded image bytes.

All image operations use gateway authentication and check session ownership. The current gateway identifies authenticated clients as the local owner; it is not a multi-user identity service. IDs are server-generated and cannot address arbitrary paths. JPEG, PNG, GIF and WebP are decoded and independently checked; maximum input is 5 MiB and 8192 pixels per side, with a bounded decoder allocation. At most 4 images are accepted per message.

Metadata lives in `gateway/sessions.db`; durable files live in `gateway/images`. Unsent uploads expire after 24 hours, with collection every five minutes, on upload, and after session deletion. Collection retains any image still referenced by a message. Unavailable images produce explicit errors. The `image_context` WebSocket event lists `omitted_ids` outside the current model image budget; their history previews remain available.
