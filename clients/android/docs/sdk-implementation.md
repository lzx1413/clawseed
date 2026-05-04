# ClawSeed Android SDK Implementation Notes

> Status: Active | Date: 2026-05-05

## 1. Purpose

This document describes the current implementation state of the Android SDK under `clients/android/sdk/`.
It complements `sdk-design.md`:

- `sdk-design.md` explains the intended architecture and API boundaries.
- This document explains what has already been implemented in code and how the pieces map together.

## 2. Module Layout

The SDK is split into three publishable modules:

| Module | Package | Responsibility |
|--------|---------|----------------|
| `sdk/core` | `dev.clawseed.sdk.core` | Session API, WebSocket protocol, REST client, models, tool registry |
| `sdk/android` | `dev.clawseed.sdk.android` | Android-scoped initialization, session ownership, accumulator, ViewModel helpers |
| `sdk/embedded` | `dev.clawseed.sdk.embedded` | On-device gateway process management and config bootstrapping |

Dependency order:

```text
sdk/core <- sdk/android <- sdk/embedded
```

## 3. Implemented Public Surface

### 3.1 sdk/core

Implemented entry points:

- `ClawSeed.createSession(config)`
- `ClawSeedConfig`
- `ClawSeedSession`
- `GatewayClient`
- `ReconnectPolicy`
- `ChatEvent`, `ConnectionState`, `SessionInfo`, `SessionSummary`, `SessionMessage`
- `GatewayStatus`, `ToolInfo`, `HealthInfo`
- `ClawSeedTool`, `ToolRegistry`, `ToolSpec`, `ToolResult`

Implementation notes:

- `DefaultClawSeedSession` composes the WebSocket client, REST client, and tool registry.
- `ChatClient` is internal and owns WebSocket connection lifecycle, reconnect behavior, queued messages, and remote tool dispatch.
- `GatewayClient` wraps gateway HTTP endpoints and returns `Result<T>` rather than throwing to callers by default.
- Tool registration supports both interface-based tools and lambda-based handlers.

### 3.2 sdk/android

Implemented entry points:

- `ClawSeedAndroid.init(context, config)`
- `ClawSeedAndroid.sessionManager()`
- `ClawSeedAndroid.gatewayClient()`
- `SessionManager`
- `ChatAccumulator`
- `AccumulatedMessage`
- `ClawSeedViewModel`

Implementation notes:

- `ClawSeedAndroid` stores process-wide SDK configuration after initialization.
- `SessionManager` owns the active session and handles reconnect-on-reuse plus optional process lifecycle binding.
- `ChatAccumulator` converts raw `ChatEvent` values into UI-friendly state flows.
- `ClawSeedViewModel` is a convenience base class, not a required integration path.

### 3.3 sdk/embedded

Implemented entry points:

- `EmbeddedGateway`
- `EmbeddedGatewayConfig`
- `GatewayState`
- `GatewayConfigManager`
- `GatewayService`

Implementation notes:

- `EmbeddedGateway` launches the native gateway binary from `nativeLibraryDir`.
- `GatewayConfigManager` creates and patches the `.clawseed` config directory inside app storage.
- `GatewayService` provides a reusable foreground service host for the embedded process.

## 4. Runtime Flow

### 4.1 Remote Gateway Flow

```text
App code
  -> ClawSeed.createSession(config)
  -> session.connect()
  -> ChatClient opens WebSocket
  -> Gateway emits ChatEvent stream
  -> optional ChatAccumulator converts stream into UI state
```

### 4.2 Embedded Gateway Flow

```text
App code
  -> EmbeddedGateway.start()
  -> GatewayConfigManager.ensureConfig()
  -> native gateway process starts
  -> health check succeeds
  -> ClawSeedAndroid.init(context, gateway.localConfig())
  -> app uses SessionManager / GatewayClient as normal
```

## 5. Design-to-Code Mapping

| Design area | Main implementation files |
|-------------|---------------------------|
| Session creation | `sdk/core/ClawSeed.kt`, `sdk/core/DefaultClawSeedSession.kt` |
| WebSocket chat protocol | `sdk/core/client/ChatClient.kt`, `sdk/core/model/ChatEvent.kt` |
| HTTP API | `sdk/core/client/GatewayClient.kt` |
| Remote tools | `sdk/core/tool/ToolRegistry.kt`, `sdk/core/tool/ClawSeedTool.kt` |
| Android lifecycle ownership | `sdk/android/ClawSeedAndroid.kt`, `sdk/android/SessionManager.kt` |
| UI accumulation | `sdk/android/ChatAccumulator.kt`, `sdk/android/AccumulatedMessage.kt` |
| Embedded process | `sdk/embedded/EmbeddedGateway.kt`, `sdk/embedded/GatewayConfigManager.kt`, `sdk/embedded/GatewayService.kt` |

## 6. Current Constraints

- `ChatClient` remains internal and is not part of the external support contract.
- The design document is still broader than the code in a few places; it should be treated as the target architecture, while this document reflects current implementation.
- The demo app is still the main integration consumer, so real-world validation is strongest on Android usage paths.

## 7. Maintenance Guidance

- When public SDK behavior changes, update KDoc in the relevant public types first.
- If module responsibilities or lifecycle assumptions change, update `sdk-design.md` and this file together.
- When onboarding examples change, update `sdk-usage.md` in the same change.