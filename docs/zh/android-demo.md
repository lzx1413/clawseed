# Android Demo 架构设计

## 概述

ClawSeed Android Demo 是一个完整的端侧 AI Agent 应用，在 Android 设备上运行整个 Agent 栈。Rust 编译的 Gateway 二进制作为前台服务进程运行，Android 客户端通过 WebSocket 连接并注册设备端工具。

## 整体架构

```
┌─────────────────────────────────────────────────────────┐
│                    Android 设备                          │
│                                                         │
│  ┌──────────────────────────────────────────────────┐  │
│  │  ClawseedService (前台服务)                       │  │
│  │                                                   │  │
│  │  ┌────────────────────────────────────────────┐  │  │
│  │  │  libclawseed.so (Rust Gateway 进程)        │  │  │
│  │  │  - Axum HTTP/WS 服务器 (端口 42617)        │  │  │
│  │  │  - Agent 循环 + LLM 调用                   │  │  │
│  │  │  - 内置工具执行                            │  │  │
│  │  │  - 远程工具桥接                            │  │  │
│  │  └────────────────────────────────────────────┘  │  │
│  │         ↑ ProcessBuilder 启动                     │  │
│  │         ↓ /health 轮询就绪                        │  │
│  └──────────────────────────────────────────────────┘  │
│                      ↕ WebSocket                       │
│  ┌──────────────────────────────────────────────────┐  │
│  │  MainActivity (Compose UI)                        │  │
│  │  ┌────────────────────────────────────────────┐  │  │
│  │  │  ClawseedClient (SDK 库)                   │  │  │
│  │  │  - OkHttp WebSocket 连接                   │  │  │
│  │  │  - 工具注册 (device_info 等)              │  │  │
│  │  │  - 工具调用处理                            │  │  │
│  │  │  - 流式响应回调                            │  │  │
│  │  └────────────────────────────────────────────┘  │  │
│  └──────────────────────────────────────────────────┘  │
│                                                         │
│                    ↕ 网络                                │
│              LLM Provider (Anthropic 等)                │
└─────────────────────────────────────────────────────────┘
```

**关键设计**：整个 Agent 栈在设备端运行，LLM 推理通过网络调用云端 Provider。

## 模块结构

```
clients/android/
├── lib/                          # ClawSeed SDK 库
│   ├── build.gradle.kts
│   └── src/main/kotlin/dev/clawseed/client/
│       ├── ClawseedClient.kt     # WebSocket 客户端
│       └── ClawseedMessages.kt   # 消息协议类型
├── app/                          # Demo 应用
│   ├── build.gradle.kts
│   └── src/main/kotlin/dev/clawseed/demo/
│       ├── MainActivity.kt       # Compose 主界面
│       └── ClawseedService.kt    # 前台服务（Gateway 进程管理）
└── settings.gradle.kts
```

### lib — SDK 库

| 类 | 职责 |
|----|------|
| `ClawseedClient` | WebSocket 连接管理、工具注册、消息收发 |
| `ToolSpec` | 工具规格（名称、描述、JSON Schema 参数） |
| `ToolCallRequest` | 服务端发来的工具调用请求 |
| `ToolCallResult` | 工具调用结果（Success / Failure） |
| `IncomingMessage` | 所有服务端消息的密封类 |
| `ToolCallHandler` | 工具调用处理接口（函数式接口） |

### app — Demo 应用

| 类 | 职责 |
|----|------|
| `MainActivity` | Compose UI，连接/消息/工具注册入口 |
| `ClawseedService` | 前台服务，管理 Gateway 进程生命周期 |

## ClawseedClient — WebSocket 客户端

### 构建器模式

```kotlin
val client = ClawseedClient.builder("ws://127.0.0.1:42617/ws/chat")
    .authToken("optional-token")
    .registerTool(ToolSpec(
        name = "device_info",
        description = "获取Android设备信息",
        parameters = """{"type":"object","properties":{},"required":[]}"""
    ))
    .toolCallHandler { request ->
        when (request.name) {
            "device_info" -> ToolCallResult.Success(queryDeviceInfo())
            else -> ToolCallResult.Failure("unknown tool")
        }
    }
    .onConnected { /* 连接成功 */ }
    .onDisconnected { /* 连接断开 */ }
    .onChunk { text -> /* 流式文本块 */ }
    .onThinking { text -> /* 思考过程 */ }
    .onDone { finalText -> /* 回合完成 */ }
    .onToolCall { id, name, args -> /* 工具调用通知 */ }
    .onToolResult { id, name, output -> /* 工具结果通知 */ }
    .onAborted { /* 回合中止 */ }
    .onError { message -> /* 错误 */ }
    .build()

client.connect()   // 建立 WebSocket 连接
client.sendMessage("你好")  // 发送用户消息
client.disconnect() // 断开连接
```

### 连接流程

1. OkHttp 建立 WebSocket 连接（readTimeout=0，支持流式）
2. `onOpen` 回调中自动发送 `register_tools` 消息
3. 服务端确认工具注册（`tools_registered`）
4. 连接就绪，可发送消息

### 工具调用处理

```kotlin
// 收到 tool_call_request 消息
private fun dispatchToolCall(request: ToolCallRequest) {
    val handler = toolCallHandler ?: run {
        webSocket?.send(ToolCallResult.Failure("No handler").toJson(request.id).toString())
        return
    }
    executor.execute {
        val result = runCatching { handler.handleToolCall(request) }
            .getOrElse { ToolCallResult.Failure(it.message ?: "Exception") }
        webSocket?.send(result.toJson(request.id).toString())
    }
}
```

**要点**：
- 使用单线程 executor 处理工具调用，避免竞态
- 异常被捕获并包装为 `ToolCallResult.Failure`
- 结果通过 WebSocket 立即发送回服务端

## 消息协议

### 客户端 → 服务端

| 类型 | 格式 | 说明 |
|------|------|------|
| 用户消息 | `{"type":"message","content":"..."}` | 发送聊天消息 |
| 工具注册 | `{"type":"register_tools","tools":[...]}` | 注册工具列表 |
| 工具结果 | `{"type":"tool_result","id":"...","output":"...","success":true}` | 返回成功结果 |
| 工具错误 | `{"type":"tool_error","id":"...","error":"...","success":false}` | 返回执行错误 |

### 服务端 → 客户端

| 类型 | 说明 |
|------|------|
| `session_start` | 会话开始（sessionId, name, resumed, messageCount） |
| `connected` | 连接确认 |
| `chunk` | 流式文本块 |
| `thinking` | Agent 思考过程 |
| `done` | 回合完成（full_response） |
| `tool_call` | 工具调用通知（信息性） |
| `tool_result` | 工具结果通知（信息性） |
| `tool_call_request` | 请求客户端执行工具（需要响应） |
| `tools_registered` | 工具注册确认（count, registered） |
| `result_acknowledged` | 结果已确认 |
| `chunk_reset` | 重置流式输出 |
| `aborted` | 回合中止 |
| `error` | 错误消息 |

### 完整交互流程示例

```
客户端                                  服务端
  │                                       │
  │ ──── WebSocket 连接 ──────────────→  │
  │ ──── register_tools ──────────────→  │
  │ ←─── tools_registered ────────────  │
  │                                       │
  │ ──── message: "告诉我设备信息" ────→  │
  │ ←─── chunk: "让我查看" ───────────  │
  │ ←─── tool_call_request ────────────  │
  │      {id:"tc1", name:"device_info"}  │
  │                                       │
  │ ──── tool_result ─────────────────→  │
  │      {id:"tc1", output:"..."}        │
  │                                       │
  │ ←─── chunk: "您的设备是..." ──────  │
  │ ←─── done ─────────────────────────  │
```

## ClawseedService — 前台服务

### 生命周期

```
onCreate()
  ├── 创建通知渠道
  └── startForeground("启动 clawseed gateway...")

onStartCommand()
  └── scope.launch { startGateway() }
        ├── 提取 libclawseed.so 二进制
        ├── ensureConfig() — 配置初始化
        ├── ProcessBuilder 启动 Gateway 进程
        │     环境: HOME, XDG_CONFIG_HOME, XDG_DATA_HOME
        │     参数: gateway --port 42617
        │     API Key: 从 .clawseed/api_key 加载
        └── waitUntilReady()
              └── 轮询 http://127.0.0.1:42617/health
                    每 500ms，最多 40 次（20 秒）

onDestroy()
  ├── 取消协程
  ├── 销毁 Gateway 进程
  └── 清理资源
```

### 二进制提取与执行

```kotlin
// APK 打包时 useLegacyPackaging = true
// libclawseed.so 被解压到 nativeLibraryDir
val binary = File(applicationInfo.nativeLibraryDir, "libclawseed.so")

// 作为子进程启动
process = ProcessBuilder(binary.absolutePath, "gateway", "--port", "42617")
    .redirectErrorStream(true)
    .also { pb ->
        pb.environment()["HOME"] = filesDir.absolutePath
        pb.environment()["CLAWSEED_API_KEY"] = apiKey
    }
    .start()
```

**为什么命名为 `.so`**：Android APK 只允许打包 `.so` 文件到 `jniLibs/`，但实际这是一个可执行的 Rust 二进制，通过 `ProcessBuilder` 执行而非 `System.loadLibrary()`。

### 配置管理

`ensureConfig()` 负责初始化和修补配置：

1. 创建 `~/.clawseed/` 和 `workspace/` 目录
2. 若 `config.toml` 不存在，生成初始配置
3. 若存在，修补缺失字段（workspace_dir、web 功能启用等）
4. 自动启用 `web_fetch`、`http_request`、`web_search`
5. 为网络工具添加 `allowed_domains = ["*"]`

初始配置模板：

```toml
workspace_dir = "{WORKSPACE_DIR}"

[gateway]

[web_fetch]
enabled = true
allowed_domains = ["*"]

[http_request]
enabled = true
allowed_domains = ["*"]

[web_search]
enabled = true
provider = "duckduckgo"
```

### 就绪检测

```kotlin
private suspend fun waitUntilReady() {
    val healthUrl = "http://127.0.0.1:42617/health"
    repeat(MAX_HEALTH_ATTEMPTS) {  // 40 次
        val code = // HTTP GET healthUrl
        if (code in 200..299) {
            isReady = true
            readyCallbacks.forEach { it() }  // 通知 MainActivity
            return
        }
        delay(500)  // 500ms 间隔
    }
    // 超时则停止服务
    stopSelf()
}
```

## 网络安全配置

```xml
<network-security-config>
    <domain-config cleartextTrafficPermitted="true">
        <domain includeSubdomains="false">127.0.0.1</domain>
        <domain includeSubdomains="false">localhost</domain>
    </domain-config>
</network-security-config>
```

仅允许 localhost 明文连接（Gateway 在本地 42617 端口）。

## 权限

| 权限 | 用途 |
|------|------|
| `INTERNET` | 网络访问（LLM API、WebSocket） |
| `FOREGROUND_SERVICE` | 前台服务运行 |
| `FOREGROUND_SERVICE_SPECIAL_USE` | Android 14+ 前台服务类型声明 |
| `POST_NOTIFICATIONS` | Android 13+ 通知权限 |

## 构建配置

| 项目 | 值 |
|------|-----|
| minSdk | 26 (Android 8.0) |
| targetSdk / compileSdk | 36 (Android 15) |
| Java 版本 | 17 |
| Compose BOM | 2026.04.01 |
| OkHttp | 4.12.0 |
| Kotlin Coroutines | 1.9.0 |
| useLegacyPackaging | true（二进制提取） |

## 自定义 Demo 的步骤

1. **添加工具**：在 `MainActivity.kt` 中定义新的 `ToolSpec` 和对应的处理逻辑
2. **修改 UI**：调整 Compose 布局
3. **添加权限**：如需设备能力（相机、位置等），在 `AndroidManifest.xml` 中声明
4. **配置 Gateway**：修改 `ClawseedService.INITIAL_CONFIG` 调整默认配置
5. **API Key**：放置在 `filesDir/.clawseed/api_key` 文件中
