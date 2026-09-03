---
description: Experimental CETP v2 specification for capability negotiation, secure authorization, controlled side effects, asynchronous execution, and compatible migration on Android.
---

# ClawSeed External Tool Protocol (CETP) v2 Experimental Specification

> Status: Experimental. The ClawSeed Android SDK implements a v2 Consumer and Provider base class,
> but the wire protocol may still change before its stable release. Production integrations should
> retain a read-only [CETP v1](external-tool-protocol.md) compatibility layer.

CETP v2 keeps the low integration cost of Android `ContentProvider` while extending CETP from a
read-only data bridge into a cross-app tool protocol that can safely carry side effects, long-running
operations, and large results. A v2 Consumer must continue to support v1. A Provider may implement
v1, v2, or both.

The terms MUST, MUST NOT, SHOULD, and MAY express requirements, prohibitions, recommendations, and
optional behavior respectively.

## 1. Goals

- Declare each tool's side effects, risk, idempotency, authorization scopes, and confirmation policy.
- Make the Provider the final enforcer of authorization, user confirmation, and mutation deduplication.
- Negotiate versions and capabilities explicitly instead of comparing one integer.
- Support deadlines, cancellation, asynchronous operations, and results too large to inline through Binder.
- Keep v1 Provider and Consumer behavior stable and provide a dual-stack migration path.
- Retain JSON Schema for tool input and output without imposing shared business schemas across Providers.

### Non-Goals

- This binding does not support transports outside Android. Other platforms may reuse its semantics but
  require a separate transport binding.
- It does not provide transactions across Providers or atomicity across multiple tool calls.
- It does not consider the Consumer, LLM, or Provider trusted by default.
- Consumer UI cannot substitute for Provider-side confirmation of high-risk operations.
- Unattended authorization of high-risk actions is outside v2; scheduled trading and similar features
  require a separate specification.

## 2. Compatible Discovery and Negotiation

### 2.1 Manifest Declaration

v2 retains the v1 discovery action and authority. It adds `com.clawseed.tools.versions`, containing a
comma-separated set of supported major versions.

A Provider supporting both v1 and v2 MUST keep the legacy `version=1` field so v1 Consumers can still
discover it:

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
    <meta-data android:name="com.clawseed.tools.versions"
        android:value="1,2" />
</service>
```

- If only `version` exists, a v2 Consumer treats it as the sole supported version.
- If both fields exist, `versions` is authoritative and `version` serves legacy Consumers only.
- A dual-stack Provider MUST NOT return side-effecting tools from its v1 `list_tools` call.
- A v2-only Provider uses `version=2` and `versions="2"`; legacy Consumers will ignore it.
- A Consumer MUST verify that the discovered Service and authority's `ContentProvider` belong to the
  same package.

### 2.2 `negotiate`

After the manifest indicates possible v2 support, the Consumer MUST call `negotiate` first. Request extras:

| Key | Bundle type | Requirement |
|---|---|---|
| `consumer_versions` | `int[]` | Required, supported major versions in descending order |
| `consumer_capabilities` | `String` | Required JSON string array |

A successful response retains Bundle `status="success"` and a JSON string in `data`:

```json
{
  "selected_version": 2,
  "provider_id": "com.example.app",
  "provider_name": "Example App",
  "session_token": "opaque-provider-issued-token",
  "expires_at": "2026-09-02T12:00:00Z",
  "capabilities": [
    "async_operations",
    "cancellation",
    "large_results"
  ],
  "limits": {
    "max_request_bytes": 65536,
    "max_inline_result_bytes": 262144,
    "max_concurrent_requests": 4,
    "idempotency_window_seconds": 86400
  }
}
```

The Provider selects the highest common version and returns only optional capabilities supported by both
sides. The `session_token`:

- MUST be opaque and unpredictable, with a maximum length of 256 bytes;
- MUST be bound to the calling UID, selected version, and capability set;
- MAY expire and MAY become invalid after Provider process restart; and
- MUST accompany subsequent v2 calls, which return `NEGOTIATION_REQUIRED` when it is invalid.

If no common version exists, the Provider returns `UNSUPPORTED_VERSION`. It MUST NOT silently interpret
the request as another version.

`limits` MUST satisfy `max_request_bytes >= 65536`, `max_inline_result_bytes >= 262144`,
`max_concurrent_requests >= 1`, and `idempotency_window_seconds >= 86400`. Both sides SHOULD retain
independent local resource limits. Negotiated values express protocol allowances and do not require a
Consumer to allocate the same amount of memory eagerly.

### 2.3 v2 Core and Optional Capabilities

Every v2 implementation MUST support tool annotations, output schemas, scoped authorization, structured
errors, Provider resolutions, and idempotent mutation deduplication. The following capabilities are negotiated:

| Capability | Meaning |
|---|---|
| `async_operations` | `execute_tool` may return a background operation |
| `cancellation` | `cancel_operation` is available |
| `large_results` | Large results use `ParcelFileDescriptor` |

A Consumer MUST NOT invoke an unnegotiated optional capability. A Provider MUST NOT change capability
semantics during a negotiated session; it SHOULD invalidate the old `session_token` and require a new
negotiation when capabilities change.

## 3. Provider and Tool Descriptors

### 3.1 Stable Identity

- `provider_id` MUST be stable and defaults to the Android package name. A custom value MUST remain
  within that package's namespace.
- Tool `name` MUST match `[a-z][a-z0-9_]{0,63}` and remain stable and unique within one Provider.
- Local display names may change, but routing MUST use `(provider_id, tool_name)` and MUST NOT depend
  on scan order or application labels.

### 3.2 `get_provider_info`

A v2 Provider MUST return stable identity, description, and a scope catalog:

```json
{
  "provider_id": "com.example.app",
  "provider_name": "Example App",
  "description": "Manage price alerts and watchlists",
  "schema_dialect": "https://json-schema.org/draft/2020-12/schema",
  "scopes": [
    {
      "name": "alerts.write",
      "title": "Manage price alerts",
      "description": "Create, update, and delete alerts",
      "access": "write",
      "sensitivity": "financial"
    }
  ]
}
```

A scope `name` MUST be stable and match `[a-z][a-z0-9_.-]{0,127}`. `access` is `read` or `write`;
`sensitivity` is `public`, `personal`, `financial`, `health`, `device`, or `other`. These fields help the
Consumer present a grant, while the Provider remains responsible for authorization decisions.

`schema_dialect` applies to every input and output schema in the negotiated session. v2 defaults to JSON
Schema Draft 2020-12. A schema MUST NOT reference network resources; `$ref` may resolve only to fragments
within the same schema document. Both sides MUST ignore unknown descriptor fields, but MUST NOT ignore a
missing required field or an unknown security enum value.

### 3.3 `list_tools`

v2 retains the `list_tools` method and requires these extras:

| Key | Bundle type | Requirement |
|---|---|---|
| `protocol_version` | `Int` | Must be `2` |
| `session_token` | `String` | Token returned by `negotiate` |

Example response:

```json
{
  "revision": "tools-42",
  "tools": [
    {
      "name": "create_alert",
      "title": "Create price alert",
      "description": "Create a price alert for a security",
      "input_schema": {
        "type": "object",
        "properties": {
          "symbol": {"type": "string"},
          "price": {"type": "number", "exclusiveMinimum": 0}
        },
        "required": ["symbol", "price"],
        "additionalProperties": false
      },
      "output_schema": {
        "type": "object",
        "properties": {"alert_id": {"type": "string"}},
        "required": ["alert_id"]
      },
      "scopes": ["alerts.write"],
      "annotations": {
        "effect": "create",
        "risk": "moderate",
        "destructive": false,
        "idempotent": false,
        "open_world": false,
        "confirmation": "provider_policy",
        "execution": "sync_or_async"
      }
    }
  ]
}
```

`revision` MUST change whenever the tool set, schema, or security semantics change. A Consumer may cache
a list with the same revision but MUST check the revision at least once per new negotiation.

### 3.4 Tool Annotations

| Field | Allowed values | Meaning |
|---|---|---|
| `effect` | `read`, `create`, `update`, `delete`, `transaction` | Primary effect on persistent state or the real world |
| `risk` | `low`, `moderate`, `high` | Impact of failure or accidental invocation |
| `destructive` | Boolean | Whether irreversible data or asset loss is possible |
| `idempotent` | Boolean | Whether repeated equivalent execution naturally has the same effect |
| `open_world` | Boolean | Whether the tool reads or affects systems outside Provider control |
| `confirmation` | `never`, `provider_policy`, `always` | Per-call Provider-side confirmation policy |
| `execution` | `sync`, `sync_or_async` | Whether the tool may return a background operation |

Normative constraints:

- A tool with `effect=read` MUST NOT mutate business state beyond logging, caching, and metering.
- `effect=delete`, `effect=transaction`, `destructive=true`, or `risk=high` requires
  `confirmation=always`.
- `execution=sync_or_async` is valid only when `async_operations` was negotiated.
- Annotations inform Consumer UI and policy; they are not a security boundary. The Provider MUST validate
  them again at execution time.
- When runtime risk exceeds the static declaration, the Provider MUST apply the stricter policy and MAY
  require user action.

`scopes` is the complete set required to execute the tool. If any scope is missing, the Provider MUST
return `AUTH_REQUIRED` or `PERMISSION_DENIED`; it MUST NOT return degraded data as if it were complete.

## 4. Invocation Model

v2 defines these `ContentResolver.call()` methods:

| Method | Capability | Purpose |
|---|---|---|
| `negotiate` | Core | Select version, capabilities, and limits |
| `get_provider_info` | Core | Read descriptions, scope help, and support information |
| `list_tools` | Core | List tools visible to the current caller |
| `execute_tool` | Core | Start or resume a tool execution |
| `get_operation` | `async_operations` | Query background execution state |
| `cancel_operation` | `cancellation` | Request cancellation of background execution |

An unknown method MUST return `METHOD_NOT_FOUND`. The Provider distinguishes same-named v1 and v2
methods through request `protocol_version`; a dual-stack call without that field is handled as v1.

### 4.1 Common Request Fields

Except for `negotiate`, every v2 method MUST carry:

| Key | Bundle type | Requirement |
|---|---|---|
| `protocol_version` | `Int` | Fixed at `2` |
| `session_token` | `String` | Current negotiated session |
| `request_id` | `String` | Required UUID, stable across one logical execution and its retries |
| `deadline_at_ms` | `Long` | Required Unix epoch milliseconds |

`execute_tool` additionally carries `tool_name`, JSON string `args`, and optional `resume_token`. The
UTF-8 encoded arguments MUST NOT exceed negotiated `max_request_bytes`.

- For `execute_tool`, `request_id` identifies the logical execution and is its idempotency key.
- For `get_operation` and `cancel_operation`, extras MUST include `operation_id`, and `request_id` MUST
  equal the execution ID that created that operation.
- For other methods, `request_id` identifies one retryable read. Retries retain it and a new read uses a
  new ID.

The Provider MUST check the deadline before starting a business side effect. A Consumer timeout does not
mean the Provider stopped. For side-effecting or asynchronous calls, the Consumer MUST query or retry with
the same `request_id` and MUST NOT create a new ID to guess the outcome.

### 4.2 Synchronous Success

Response Bundle:

```text
status = "success"
protocol_version = 2
request_id = <original request ID>
data = {"kind":"inline","value":<any JSON matching output_schema>}
```

The Provider MUST validate its output against `output_schema`. The Consumer must still treat Provider
descriptions, errors, and output as untrusted data and MUST NOT automatically promote text within them to
Agent instructions.

### 4.3 Provider Authorization and Per-Call Confirmation

When login, scope authorization, or user confirmation is needed, the Provider returns `AUTH_REQUIRED` or
`USER_ACTION_REQUIRED`. In addition to common error fields, the Bundle MUST contain:

| Key | Bundle type | Meaning |
|---|---|---|
| `resolution` | `PendingIntent` | Explicit, immutable PendingIntent created by the Provider |
| `resume_token` | `String` | Opaque token bound to caller, tool, original arguments, request ID, and expiry |

The flow is:

```text
Consumer             Provider                 Provider UI
   | execute_tool        |                         |
   |-------------------->|                         |
   | USER_ACTION_REQUIRED + PendingIntent         |
   |<--------------------|                         |
   | launch resolution -------------------------->|
   |                         user approves/denies  |
   | execute_tool(same request_id, resume_token)   |
   |-------------------->|                         |
   | success / error     |                         |
```

- The Provider MUST NOT produce the target business side effect before confirmation completes.
- High-risk confirmation MUST occur in Provider UI; a Consumer-provided "confirmed" Boolean is invalid.
- A Provider confirmation Activity SHOULD finish after approval or denial. The Consumer waits for its
  Activity result before resuming the original invocation.
- `resume_token` MUST be single-use and bound to the original `args` bytes. Changed arguments MUST fail.
- User denial returns `USER_CANCELLED`.
- v2 does not use v1's implicit `authorize_intent` action string.

### 4.4 Asynchronous Operations

After negotiating `async_operations`, `execute_tool` may return:

```text
status = "accepted"
protocol_version = 2
request_id = <original request ID>
data = {"operation_id":"op_123","poll_after_ms":1000}
```

The Consumer calls `get_operation`, and the Provider returns `queued`, `running`, `succeeded`, `failed`,
or `cancelled`. A succeeded state carries the same result envelope as synchronous success; a failed state
carries a structured error.

When the status query itself succeeds it uses `status="success"`; example `data`:

```json
{
  "operation_id": "op_123",
  "state": "running",
  "progress": {"current": 40, "total": 100, "message": "Synchronizing"},
  "poll_after_ms": 1000
}
```

`progress` is optional display data and cannot establish business success. Terminal `succeeded` includes
`result`, while `failed` includes an `error` object matching Section 5 fields. A successful
`cancel_operation` only acknowledges the cancellation request and returns
`state="cancellation_requested"`; the Consumer must still query a terminal state.

- `operation_id` MUST be bound to the calling UID and unavailable to other callers.
- An operation MUST remain available until at least the end of the negotiated idempotency window. A
  Provider may advertise a longer TTL.
- Mutation deduplication records and operation state MUST be durable; Provider process restart MUST NOT
  cause duplicate execution.
- If `cancellation` was negotiated, the Consumer may call `cancel_operation`.
- Cancellation is cooperative. Only a queried `cancelled` state means no later effect is expected. An
  already committed external transaction may return `CONFLICT` and MUST NOT claim cancellation succeeded.

### 4.5 Large Results

When inline JSON exceeds `max_inline_result_bytes`, the Provider MUST NOT continue placing it in the
Bundle. After negotiating `large_results`, the success Bundle uses:

```text
data = {
  "kind":"file",
  "media_type":"application/json",
  "size_bytes":1048576,
  "sha256":"<lowercase hex>"
}
result_fd = <read-only ParcelFileDescriptor>
```

The Consumer MUST enforce a local read limit, verify size, digest, and `output_schema`, and close the file
descriptor. The file belongs to one response; its path is not stable. Without `large_results`, the Provider
returns `RESULT_TOO_LARGE`.

## 5. Error Model

An error response MUST include `status="error"`, `error_code`, a safely displayable `error_message`, a
Boolean `retryable`, and echoed `protocol_version` and `request_id`. Optional fields are JSON
`error_details` and Long `retry_after_ms`.

| Error code | Retryable | Meaning |
|---|---:|---|
| `UNSUPPORTED_VERSION` | No | No common protocol version |
| `UNSUPPORTED_CAPABILITY` | No | Request uses an unnegotiated capability |
| `NEGOTIATION_REQUIRED` | Yes | Session is absent, expired, or lost after Provider restart |
| `AUTH_REQUIRED` | Conditional | User must log in or grant scopes |
| `USER_ACTION_REQUIRED` | Conditional | This call needs Provider-side confirmation |
| `USER_CANCELLED` | No | User denied or cancelled confirmation |
| `PERMISSION_DENIED` | No | Caller is not allowed access |
| `TOOL_NOT_FOUND` | No | Tool does not exist or is hidden from this caller |
| `METHOD_NOT_FOUND` | No | Method does not exist |
| `INVALID_ARGS` | No | Arguments do not match the input schema |
| `INVALID_REQUEST` | No | Envelope, token, or request ID use is invalid |
| `CONFLICT` | Conditional | Current state prevents execution or cancellation |
| `RATE_LIMITED` | Yes | Provider rate limit exceeded |
| `DEADLINE_EXCEEDED` | Conditional | Deadline elapsed; query the same request ID first |
| `CANCELLED` | No | Operation is confirmed cancelled |
| `RESULT_TOO_LARGE` | No | Large result transport was not negotiated and inline is impossible |
| `RESULT_EXPIRED` | No | Operation result exceeded retention |
| `UNAVAILABLE` | Yes | Provider is temporarily unavailable |
| `INTERNAL_ERROR` | Conditional | Internal failure without stacks or sensitive details |

A Consumer may automatically retry only when `retryable=true` and MUST honor `retry_after_ms`. It MUST
NOT loop on authorization, confirmation, or argument errors without user action or changed input.

## 6. Idempotency, Retry, and Concurrency

- Every tool with `effect != read` MUST deduplicate by `request_id`, regardless of its `idempotent` annotation.
- The deduplication key includes at least calling UID, `provider_id`, and `request_id`. Its record MUST bind
  tool name and a digest of the original arguments. Reusing the ID with different content returns
  `INVALID_REQUEST`.
- The Provider MUST return the same terminal outcome during `idempotency_window_seconds`, whose minimum
  value is 86400 seconds.
- Concurrent arrival of the same request permits only one executor; other calls return the same operation
  or terminal outcome.
- After Binder disconnect or timeout, a Consumer may only retry with the same ID or query status.
- `idempotent=true` describes natural business semantics and does not remove request deduplication duties.

## 7. Security Model

### 7.1 Caller Identity

The Provider MUST synchronously capture `Binder.getCallingUid()` at the `ContentProvider.call()` entry
before switching execution contexts. Package names, caller fields in `args`, and self-reported Consumer
identity are untrusted.

For authorization, a Provider SHOULD:

1. Resolve every package for the UID instead of taking the first package.
2. Verify current signing-certificate digests and, where appropriate, Android signing-certificate rotation history.
3. Bind grants to UID, package, certificate digest, scopes, and expiry.
4. Deny or require reauthorization for shared UIDs, ambiguous package mappings, and signature changes.

`com.clawseed.permission.ACCESS_TOOLS` remains a discovery marker and defense in depth. Its `normal`
protection level is not an identity or authorization boundary.

### 7.2 Least Privilege and Confirmation

- `list_tools` SHOULD return only tools discoverable by the caller; sensitive tools may be hidden entirely.
- The Provider MUST check scopes for every execution and cannot rely on a previous `list_tools` result.
- A `PendingIntent` MUST explicitly target a Provider component and use the immutable flag.
- High-risk UI MUST show the concrete target, action, critical parameters, and irreversible consequences,
  not only the tool description.
- A Provider SHOULD record redacted audit events containing caller, tool, request ID, authorization or
  confirmation outcome, and terminal state.

### 7.3 Untrusted Content

Provider names, tool descriptions, schemas, errors, and results may be malicious. A Consumer MUST enforce
length limits, validate JSON/schema, and mark tool output as data. A Provider must likewise treat
Agent-generated arguments as untrusted input and perform business validation beyond JSON Schema.

## 8. v1 Coexistence and Migration

| Phase | Provider | Consumer |
|---|---|---|
| 0: Current | Continue serving v1 read-only tools | Preserve existing v1 behavior |
| 1: Dual discovery | Add `versions="1,2"`; keep only read tools in v1 list | Parse `versions` and implement `negotiate` |
| 2: v2 reads | Add schemas, scopes, annotations, and structured errors | A Provider advertising v2 must negotiate successfully or be disabled |
| 3: Controlled writes | Expose mutations only through v2 with confirmation and deduplication | Present risk and correctly resume confirmation |
| 4: Async/large | Enable optional capabilities as needed | Use methods only after negotiation |

Fallback rules:

- A Consumer uses v1 only when the Provider manifest includes v1 and does not advertise v2. Failed v2
  negotiation MUST NOT downgrade.
- Failed v2 negotiation, authorization, or security validation MUST NOT downgrade to v1 to bypass controls.
- A v1 Consumer can see only the dual-stack Provider's read-only subset.
- Existing side-effecting v1 extensions SHOULD migrate to v2. Until then, Consumers SHOULD disable them
  by default or require local human approval and MUST NOT describe them as v1-conformant.

## 9. Conformance Requirements

Before v2 is released, it SHOULD have shared test vectors and Android Provider/Consumer suites covering:

- manifest version sets, no common version, and dual-stack fallback;
- unknown fields, missing required fields, invalid JSON, and schema mismatch;
- caller UID/signature changes, scope expiry, PendingIntent binding, and token replay;
- concurrent same-ID calls, retry after timeout, different arguments with one ID, and idempotency windows;
- approval, denial, token expiry, and absence of effects before confirmation;
- operation success, failure, cancellation races, and Provider restart recovery;
- Binder inline boundaries, invalid file digests, bounded reads, and descriptor closure; and
- malicious Provider descriptions, injected instructions, unknown errors, and sensitive error content.

An implementation may claim CETP v2 only after all core tests pass. Each optional capability must be
declared and pass its corresponding tests independently.

## 10. Android SDK Implementation

The SDK provides `CetpClient`, `ExternalToolBridge`, and an extensible `CetpV2ContentProvider`. A minimal
Provider has this shape:

```kotlin
class AlertToolProvider : CetpV2ContentProvider() {
    override val provider = CetpProviderDescriptor(
        providerId = "com.example.app",
        providerName = "Example App",
        description = "Price alerts",
        scopes = listOf(
            ProviderScope(
                name = "alerts.write",
                description = "Manage price alerts",
                access = ScopeAccess.WRITE,
                sensitivity = ScopeSensitivity.FINANCIAL,
            ),
        ),
    )

    override val providerTools = listOf(
        CetpProviderTool(
            name = "create_alert",
            title = "Create price alert",
            description = "Create a price alert for a security",
            inputSchema = createAlertInputSchema,
            outputSchema = createAlertOutputSchema,
            scopes = listOf("alerts.write"),
            annotations = ToolAnnotations(
                effect = ToolEffect.CREATE,
                risk = ToolRisk.MODERATE,
                idempotent = false,
                confirmation = ConfirmationPolicy.PROVIDER_POLICY,
            ),
        ),
    )

    override val mutationExecutor = appDurableMutationExecutor

    override fun authorize(request: CetpProviderRequest): CetpAccessDecision =
        appAuthorizationPolicy.authorize(request)

    override fun executeTool(request: CetpProviderRequest): CetpProviderResult =
        alertRepository.execute(request)
}
```

`CetpV2ContentProvider` denies authorization by default and rejects inconsistent high-risk declarations,
expired deadlines, invalid sessions, and mutation tools without a `CetpMutationExecutor`. The application's
mutation executor must persist request reservation, business mutation, and result within one reliable
boundary; the SDK cannot substitute for atomicity in the business database.

The SDK owns protocol envelopes, negotiation tokens, caller UID/signature collection, structured errors,
async polling/cancellation, file-result verification, and Provider UI resumption. The Provider application
still owns JSON Schema and business validation, scope policy, confirmation-token state, operation
persistence, and audit records.
