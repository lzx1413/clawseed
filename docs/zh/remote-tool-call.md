# 远程工具调用机制

## 概述

远程工具调用（Remote Tool Call）是 ClawSeed 的核心特性之一，允许移动客户端通过 WebSocket 注册和执行工具。Agent 无需区分本地工具和远程工具——两者都实现 `Tool` trait，调用方式完全相同。

## 架构总览

```
┌──────────────────┐                          ┌──────────────────┐
│   移动客户端      │                          │   Gateway 服务端  │
│                  │                          │                  │
│  ClawseedClient  │   1. register_tools      │  WebSocket       │
│  (OkHttp WS)     │ ──────────────────────→  │  Handler         │
│                  │                          │       ↓          │
│                  │   2. tools_registered    │  RemoteTool      │
│                  │ ←──────────────────────  │  Registry        │
│                  │                          │       ↓          │
│                  │                          │  Agent.tools      │
│                  │                          │  (Vec<Box<dyn    │
│                  │                          │   Tool>>)        │
│                  │                          │                  │
│  工具执行器      │   3. tool_call_request   │  Agent Loop      │
│  (ToolCall       │ ←──────────────────────  │  调用 RemoteTool │
│   Handler)       │                          │  .execute()      │
│                  │                          │                  │
│                  │   4. tool_result         │  等待响应        │
│                  │ ──────────────────────→  │  (30s 超时)     │
│                  │                          │       ↓          │
│                  │   5. result_acknowledged │  返回结果给      │
│                  │ ←──────────────────────  │  Agent Loop      │
└──────────────────┘                          └──────────────────┘
```

## 服务端实现

### RemoteTool — 远程工具包装

`RemoteTool` 实现了 `Tool` trait，将工具执行桥接到 WebSocket 客户端：

```rust
pub struct RemoteTool {
    spec: ToolSpec,
    request_tx: mpsc::Sender<RemoteToolRequest>,
}

#[async_trait]
impl Tool for RemoteTool {
    fn name(&self) -> &str { &self.spec.name }
    fn description(&self) -> &str { &self.spec.description }
    fn parameters_schema(&self) -> Value { self.spec.parameters.clone() }

    async fn execute(&self, args: Value, _ctx: &dyn ToolContext) -> Result<ToolResult> {
        let (response_tx, response_rx) = oneshot::channel();
        let call_id = Uuid::new_v4().to_string();

        // 发送请求到 WebSocket 处理器
        self.request_tx.send(RemoteToolRequest {
            call_id: call_id.clone(),
            tool_name: self.spec.name.clone(),
            args,
            response_tx,
        }).await?;

        // 等待客户端响应（30 秒超时）
        match tokio::time::timeout(
            Duration::from_secs(30),
            response_rx,
        ).await {
            Ok(Ok(result)) => Ok(result),
            Ok(Err(_)) => Err(anyhow!("Channel closed")),
            Err(_) => Err(anyhow!("Remote tool timeout (30s)")),
        }
    }
}
```

**关键设计**：
- 使用 `mpsc::Sender` 发送请求到 WebSocket handler
- 使用 `oneshot::channel` 等待单次响应
- 30 秒超时防止无限等待
- 不使用 `ToolContext`（无法访问服务端能力）

### RemoteToolRegistryHandle — 工具注册管理

```rust
pub struct RemoteToolRegistryHandle {
    tools: Vec<RemoteTool>,
    request_rx: mpsc::Receiver<RemoteToolRequest>,
}
```

管理从 WebSocket 客户端注册的工具，提供请求接收通道。

### WebSocket 处理器

WebSocket handler 处理工具注册和请求转发：

```rust
async fn handle_ws(socket: WebSocket, agent: Agent) {
    let (registry_handle, request_rx) = RemoteToolRegistryHandle::new();
    let mut client_tools: Vec<RemoteTool> = Vec::new();

    while let Some(msg) = socket.next().await {
        match msg {
            // 工具注册
            Ok(Text(text)) if type == "register_tools" => {
                for spec in tool_specs {
                    let remote_tool = RemoteTool::new(spec, request_tx);
                    client_tools.push(remote_tool);
                    agent.add_tool(remote_tool);
                }
                socket.send(tools_registered(count)).await;
            }

            // 工具结果返回
            Ok(Text(text)) if type == "tool_result" => {
                let result = ToolResult { success: true, output, error: None };
                response_tx.send(result);
            }

            // 工具错误返回
            Ok(Text(text)) if type == "tool_error" => {
                let result = ToolResult { success: false, output: String::new(), error: Some(err) };
                response_tx.send(result);
            }
        }
    }

    // WebSocket 断开时移除远程工具
    for tool in client_tools {
        agent.remove_tool(&tool.name);
    }
}
```

## 客户端实现

### 工具注册

```kotlin
// 构建工具规格
val toolSpec = ToolSpec(
    name = "device_info",
    description = "获取Android设备信息，包括型号、制造商、Android版本",
    parameters = """{"type":"object","properties":{},"required":[]}"""
)

// 通过 Builder 注册
val client = ClawseedClient.builder(url)
    .registerTool(toolSpec)
    .toolCallHandler { request ->
        when (request.name) {
            "device_info" -> ToolCallResult.Success(queryDeviceInfo())
            else -> ToolCallResult.Failure("unknown tool")
        }
    }
    .build()
```

### 工具调用处理

```kotlin
// 收到 tool_call_request 时的处理
private fun dispatchToolCall(request: ToolCallRequest) {
    val handler = toolCallHandler ?: run {
        // 没有注册处理器，返回错误
        webSocket?.send(ToolCallResult.Failure("No handler").toJson(request.id).toString())
        return
    }
    // 在单线程 executor 中执行，避免竞态
    executor.execute {
        val result = runCatching { handler.handleToolCall(request) }
            .getOrElse { ToolCallResult.Failure(it.message ?: "Exception") }
        // 立即通过 WebSocket 返回结果
        webSocket?.send(result.toJson(request.id).toString())
    }
}
```

## 消息协议详解

### 工具注册阶段

```json
// 客户端 → 服务端
{
    "type": "register_tools",
    "tools": [
        {
            "name": "device_info",
            "description": "获取设备信息",
            "parameters": {"type": "object", "properties": {}, "required": []}
        },
        {
            "name": "camera",
            "description": "拍摄照片",
            "parameters": {
                "type": "object",
                "properties": {
                    "quality": {"type": "string", "enum": ["low", "medium", "high"]}
                }
            }
        }
    ]
}

// 服务端 → 客户端
{
    "type": "tools_registered",
    "count": 2,
    "registered": 2
}
```

### 工具调用阶段

```json
// 服务端 → 客户端（请求执行工具）
{
    "type": "tool_call_request",
    "id": "550e8400-e29b-41d4-a716-446655440000",
    "name": "device_info",
    "args": {}
}

// 客户端 → 服务端（成功结果）
{
    "type": "tool_result",
    "id": "550e8400-e29b-41d4-a716-446655440000",
    "output": "{\"model\":\"Pixel 8\",\"manufacturer\":\"Google\",\"android_version\":\"14\"}",
    "success": true
}

// 客户端 → 服务端（错误结果）
{
    "type": "tool_error",
    "id": "550e8400-e29b-41d4-a716-446655440000",
    "error": "Camera permission denied",
    "success": false
}

// 服务端 → 客户端（确认结果已收到）
{
    "type": "result_acknowledged",
    "id": "550e8400-e29b-41d4-a716-446655440000"
}
```

## 远程工具 vs 本地工具

| 特性 | 本地工具 | 远程工具 |
|------|---------|---------|
| 注册方式 | `all_tools()` 函数 | WebSocket `register_tools` 消息 |
| 执行位置 | Gateway 服务端 | 客户端设备 |
| ToolContext | 完整访问（Memory、SecurityPolicy 等） | 不使用 |
| 超时 | 无限制 | 30 秒 |
| 生命周期 | 随 Gateway 进程 | 随 WebSocket 连接 |
| 典型用途 | 文件操作、Shell、Web 请求 | 设备能力（相机、传感器、联系人） |
| 错误处理 | `ToolResult::error` | `tool_error` 消息或超时 |

## 连接生命周期

```
WebSocket 连接建立
    ↓
客户端发送 register_tools
    ↓
Gateway 创建 RemoteTool 实例，加入 Agent.tools
    ↓
正常对话和工具调用
    ↓
WebSocket 断开
    ↓
Gateway 从 Agent.tools 移除所有远程工具
    ↓
后续对话不再调用已断开客户端的工具
```

**重要**：远程工具的生命周期与 WebSocket 连接绑定。连接断开后，相关工具自动从 Agent 中移除。

## 典型应用场景

### 设备信息查询

```kotlin
ToolSpec("device_info", "获取设备信息",
    """{"type":"object","properties":{},"required":[]}""")
```

### 相机操作

```kotlin
ToolSpec("camera", "拍摄照片",
    """{"type":"object","properties":{"quality":{"type":"string","enum":["low","high"]}},"required":[]}""")
```

### 联系人查询

```kotlin
ToolSpec("contacts", "查询手机联系人",
    """{"type":"object","properties":{"query":{"type":"string"}},"required":["query"]}""")
```

### 传感器数据

```kotlin
ToolSpec("sensors", "读取传感器数据",
    """{"type":"object","properties":{"type":{"type":"string","enum":["accelerometer","gyroscope","gps"]}},"required":["type"]}""")
```

## 错误处理

| 场景 | 处理方式 |
|------|---------|
| 工具处理器未注册 | 返回 `tool_error`，消息："No handler registered" |
| 工具执行抛异常 | 捕获异常，返回 `tool_error`，包含异常消息 |
| 客户端未在 30s 内响应 | Gateway 返回超时错误给 Agent |
| WebSocket 断开 | 移除所有远程工具，Agent 不再调用 |
| call_id 不匹配 | 丢弃无法关联的响应 |

## 安全考虑

- 远程工具**无法**访问服务端能力（Memory、SecurityPolicy、Provider）
- 工具参数由客户端自行验证
- Gateway 仍然通过 Hook 管线拦截工具调用
- `before_tool_call` Hook 可以取消远程工具调用
- 建议：在 `SecurityPolicy` 中限制可注册的工具名称范围
