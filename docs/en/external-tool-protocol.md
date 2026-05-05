# ClawSeed External Tool Protocol (CETP) v1

Defines how third-party Android apps expose read-only data to ClawSeed.

## 1. Design Goals

Many utility apps on Android hold valuable data as their core asset, but building an AI chat interface in each app is redundant and low-quality. CETP's core idea: **ClawSeed serves as the unified AI interaction layer; other apps only need to be data providers.**

- v1 defines discovery and invocation for read-only tools only
- Third-party apps do not connect to the gateway directly; the ClawSeed Android client discovers and bridges them
- The Bridge wraps external tools as local tools, then registers them with the gateway via the existing WebSocket `register_tools` mechanism
- Tool specs reuse JSON Schema, fully aligned with the `Tool` trait
- The protocol does not require the Consumer to be the official ClawSeed app; access control is the Provider's decision
- Minimal integration effort — implement one ContentProvider, ~100 lines of code

### v1 Scope

- Discovery (`list_tools`) and invocation (`execute_tool`) of read-only tools
- Optional provider metadata query (`get_provider_info`)
- Provider-controlled authorization model (via `AUTH_REQUIRED` error code + retry after user completes authorization)
- Android ContentProvider as the cross-process communication mechanism

### Non-Goals

- No write operation protocol — orders, transfers, deletions are deferred to v2
- No unified OAuth-style authorization protocol — authorization strategy is entirely up to the Provider
- No strong Consumer identity verification — package name identification is only for Provider's own authorization policy
- No unified data model across Providers — each app defines its own return structure
- No strict idempotency requirement — unnecessary for read-only scenarios

## 2. Protocol Definition

### 2.1 Roles

| Role | Description |
|---|---|
| **Tool Provider** | Third-party app that exposes read-only tools via ContentProvider |
| **Tool Consumer** | Any CETP-compatible Android app that discovers and invokes external tools |

### 2.2 Discovery

Provider declares a Service + ContentProvider in `AndroidManifest.xml`:

```xml
<service
    android:name=".cetp.ToolProviderService"
    android:exported="true">
    <intent-filter>
        <action android:name="com.clawseed.action.TOOL_PROVIDER" />
    </intent-filter>
    <meta-data android:name="com.clawseed.tools.authority"
        android:value="com.example.app.clawseed.tools" />
    <meta-data android:name="com.clawseed.tools.version"
        android:value="1" />
</service>

<provider
    android:name=".cetp.ToolProvider"
    android:authorities="com.example.app.clawseed.tools"
    android:exported="true"
    android:permission="com.clawseed.permission.ACCESS_TOOLS"
    android:initOrder="100" />
```

- `com.clawseed.permission.ACCESS_TOOLS` (`protectionLevel="normal"`) reduces accidental calls and **does not constitute a strong security boundary**
- Consumer must declare `<uses-permission android:name="com.clawseed.permission.ACCESS_TOOLS" />`
- `android:initOrder="100"` ensures early Provider initialization (needed on some OEM ROMs)

Consumer discovers Providers via `PackageManager.queryIntentServices()` scanning for the `com.clawseed.action.TOOL_PROVIDER` intent, extracting authority from meta-data.

### 2.3 Data Exchange

All interactions use `ContentResolver.call()` with a unified response format:

**Success:** `{ "status": "success", "data": "<JSON string>" }`

**Error:** `{ "status": "error", "error_code": "<code>", "error_message": "<description>" }`

`AUTH_REQUIRED` errors may include optional fields:

```json
{
  "status": "error",
  "error_code": "AUTH_REQUIRED",
  "error_message": "User has not authorized this data scope",
  "resolution_hint": "Please open the app to complete authorization",
  "authorize_intent": "com.example.app.ACTION_AUTHORIZE_CLAWSEED"
}
```

#### `list_tools`

| | Value |
|---|---|
| method | `"list_tools"` |
| extras | null |

Response `data`:

```json
{
  "tools": [
    {
      "name": "get_data",
      "description": "Natural language description; Agent uses this to decide when to invoke",
      "parameters": { "<JSON Schema>" }
    }
  ]
}
```

- `name`: Only needs to be unique within a single Provider. The Consumer handles namespace isolation (e.g., prefixing `app__get_data`)
- `parameters`: Standard JSON Schema, aligned with `Tool::parameters_schema()`

#### `execute_tool`

| | Value |
|---|---|
| method | `"execute_tool"` |
| extras | `tool_name`, `args` (JSON), `request_id` (optional UUID) |

In CETP v1, **only read-only operations are allowed**. No write, submit, or delete side effects.

`request_id` is for log correlation and retry identification. v1 does not mandate strict idempotency.

Response `data`: Free-form JSON returned by the tool.

#### `get_provider_info` (Optional)

| | Value |
|---|---|
| method | `"get_provider_info"` |
| extras | null |

Response `data`:

```json
{
  "provider_name": "MyApp",
  "description": "Data description",
  "scopes": [
    { "name": "scope_name", "description": "Scope description" }
  ]
}
```

### 2.4 Error Codes

| Error Code | Meaning |
|---|---|
| `AUTH_REQUIRED` | User needs to complete authentication or authorization in the Provider app |
| `PERMISSION_DENIED` | Caller is not authorized by the Provider |
| `TOOL_NOT_FOUND` | Requested tool name does not exist |
| `INVALID_ARGS` | Arguments do not conform to schema |
| `RATE_LIMITED` | Call frequency exceeded |
| `INTERNAL_ERROR` | Provider internal error |

### 2.5 Version Compatibility

- `com.clawseed.tools.version` meta-data identifies the protocol version
- Consumer should check version and gracefully degrade for unknown versions
- New `call()` methods do not affect older versions (they return `TOOL_NOT_FOUND`)
- New fields in tool specs are backward-compatible (Consumer ignores unknown fields)

## 3. Implementation Logic

### 3.1 Consumer Bridge Architecture

```
Provider App (ContentProvider)
  │  ContentResolver.call()
  ▼
ClawSeed Android (ExternalToolBridge)
  │  Wraps as local tool + namespace prefix
  │  WebSocket register_tools
  ▼
ClawSeed Gateway → Agent (no awareness of difference)
```

ExternalToolBridge responsibilities:
1. Scan PackageManager to discover Providers
2. Call `list_tools` to fetch tool specs
3. Add namespace prefix (`{label}__{tool_name}`)
4. Register with gateway (reuses existing RemoteTool mechanism)
5. Forward Agent invocations back to the Provider
6. Listen for `PACKAGE_ADDED` / `PACKAGE_REPLACED` / `PACKAGE_REMOVED` broadcasts to dynamically refresh

### 3.2 Provider Self-Managed Authorization

CETP v1 does not define a unified authorization protocol. Providers can identify the calling app via `Binder.getCallingUid()` + `PackageManager` to reverse-lookup the caller's package name, and autonomously decide:
- Maintain a whitelist of authorized callers
- Return `AUTH_REQUIRED` on first call, with `resolution_hint` and `authorize_intent`
- Return different data granularity for different callers (sanitized vs. full)
- Set authorization expiration

### 3.3 Provider Process Keep-Alive

Android theoretically auto-starts ContentProvider processes on demand, but some OEM ROMs (MIUI, ColorOS) may prevent automatic startup.

**Provider recommendations:**
- Declare a `BOOT_COMPLETED` broadcast receiver to ensure process availability
- Set `android:initOrder="100"` on the `<provider>` element

**Consumer fallback:**
- `acquireContentProviderClient()` to trigger process creation
- `startActivity(launchIntent)` to start the Provider App
- If both fail, wait for the next session connection or `PACKAGE_ADDED` broadcast to retry
