# Remote Tool Call Mechanism

## Overview

Remote Tool Call is one of ClawSeed's core features, allowing mobile clients to register and execute tools over WebSocket. The agent has no distinction between local and remote tools — both implement the `Tool` trait and are invoked identically.

## Architecture Overview

```
┌──────────────────┐                          ┌──────────────────┐
│   Mobile Client   │                          │   Gateway Server  │
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
│  Tool Executor   │   3. tool_call_request   │  Agent Loop      │
│  (ToolCall       │ ←──────────────────────  │  calls RemoteTool│
│   Handler)       │                          │  .execute()      │
│                  │                          │                  │
│                  │   4. tool_result         │  Waits for       │
│                  │ ──────────────────────→  │  response        │
│                  │                          │  (30s timeout)   │
│                  │                          │       ↓          │
│                  │   5. result_acknowledged │  Returns result  │
│                  │ ←──────────────────────  │  to Agent Loop   │
└──────────────────┘                          └──────────────────┘
```

## Server-Side Implementation

### RemoteTool — Remote Tool Wrapper

`RemoteTool` implements the `Tool` trait, bridging execution to the WebSocket client:

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

        // Send request to WebSocket handler
        self.request_tx.send(RemoteToolRequest {
            call_id: call_id.clone(),
            tool_name: self.spec.name.clone(),
            args,
            response_tx,
        }).await?;

        // Wait for client response (30-second timeout)
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

**Key design**:
- Uses `mpsc::Sender` to send requests to the WebSocket handler
- Uses `oneshot::channel` to await a single response
- 30-second timeout prevents indefinite waiting
- Does not use `ToolContext` (no access to server-side capabilities)

### RemoteToolRegistryHandle — Tool Registration Manager

```rust
pub struct RemoteToolRegistryHandle {
    tools: Vec<RemoteTool>,
    request_rx: mpsc::Receiver<RemoteToolRequest>,
}
```

Manages tools registered from WebSocket clients, providing a request receiver channel.

### WebSocket Handler

The WebSocket handler processes tool registration and request forwarding:

```rust
async fn handle_ws(socket: WebSocket, agent: Agent) {
    let (registry_handle, request_rx) = RemoteToolRegistryHandle::new();
    let mut client_tools: Vec<RemoteTool> = Vec::new();

    while let Some(msg) = socket.next().await {
        match msg {
            // Tool registration
            Ok(Text(text)) if type == "register_tools" => {
                for spec in tool_specs {
                    let remote_tool = RemoteTool::new(spec, request_tx);
                    client_tools.push(remote_tool);
                    agent.add_tool(remote_tool);
                }
                socket.send(tools_registered(count)).await;
            }

            // Tool result response
            Ok(Text(text)) if type == "tool_result" => {
                let result = ToolResult { success: true, output, error: None };
                response_tx.send(result);
            }

            // Tool error response
            Ok(Text(text)) if type == "tool_error" => {
                let result = ToolResult { success: false, output: String::new(), error: Some(err) };
                response_tx.send(result);
            }
        }
    }

    // Remove remote tools when WebSocket disconnects
    for tool in client_tools {
        agent.remove_tool(&tool.name);
    }
}
```

## Client-Side Implementation

### Tool Registration

```kotlin
// Build tool specification
val toolSpec = ToolSpec(
    name = "device_info",
    description = "Get Android device information including model, manufacturer, Android version",
    parameters = """{"type":"object","properties":{},"required":[]}"""
)

// Register via Builder
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

### Tool Call Handling

```kotlin
// When a tool_call_request message is received
private fun dispatchToolCall(request: ToolCallRequest) {
    val handler = toolCallHandler ?: run {
        // No handler registered, return error
        webSocket?.send(ToolCallResult.Failure("No handler").toJson(request.id).toString())
        return
    }
    // Execute on single-threaded executor to avoid races
    executor.execute {
        val result = runCatching { handler.handleToolCall(request) }
            .getOrElse { ToolCallResult.Failure(it.message ?: "Exception") }
        // Send result back immediately via WebSocket
        webSocket?.send(result.toJson(request.id).toString())
    }
}
```

## Message Protocol Details

### Tool Registration Phase

```json
// Client → Server
{
    "type": "register_tools",
    "tools": [
        {
            "name": "device_info",
            "description": "Get device information",
            "parameters": {"type": "object", "properties": {}, "required": []}
        },
        {
            "name": "camera",
            "description": "Take a photo",
            "parameters": {
                "type": "object",
                "properties": {
                    "quality": {"type": "string", "enum": ["low", "medium", "high"]}
                }
            }
        }
    ]
}

// Server → Client
{
    "type": "tools_registered",
    "count": 2,
    "registered": 2
}
```

### Tool Invocation Phase

```json
// Server → Client (request tool execution)
{
    "type": "tool_call_request",
    "id": "550e8400-e29b-41d4-a716-446655440000",
    "name": "device_info",
    "args": {}
}

// Client → Server (success result)
{
    "type": "tool_result",
    "id": "550e8400-e29b-41d4-a716-446655440000",
    "output": "{\"model\":\"Pixel 8\",\"manufacturer\":\"Google\",\"android_version\":\"14\"}",
    "success": true
}

// Client → Server (error result)
{
    "type": "tool_error",
    "id": "550e8400-e29b-41d4-a716-446655440000",
    "error": "Camera permission denied",
    "success": false
}

// Server → Client (acknowledge result received)
{
    "type": "result_acknowledged",
    "id": "550e8400-e29b-41d4-a716-446655440000"
}
```

## Remote vs. Local Tools

| Feature | Local Tool | Remote Tool |
|---------|-----------|-------------|
| Registration | `all_tools()` function | WebSocket `register_tools` message |
| Execution location | Gateway server | Client device |
| ToolContext | Full access (Memory, SecurityPolicy, etc.) | Not used |
| Timeout | Unlimited | 30 seconds |
| Lifecycle | With Gateway process | With WebSocket connection |
| Typical use | File ops, shell, web requests | Device capabilities (camera, sensors, contacts) |
| Error handling | `ToolResult::error` | `tool_error` message or timeout |

## Connection Lifecycle

```
WebSocket connection established
    ↓
Client sends register_tools
    ↓
Gateway creates RemoteTool instances, adds to Agent.tools
    ↓
Normal conversation and tool calls
    ↓
WebSocket disconnects
    ↓
Gateway removes all remote tools from Agent.tools
    ↓
Subsequent conversations no longer call the disconnected client's tools
```

**Important**: Remote tool lifecycle is bound to the WebSocket connection. On disconnect, associated tools are automatically removed from the agent.

## Typical Use Cases

### Device Information

```kotlin
ToolSpec("device_info", "Get device information",
    """{"type":"object","properties":{},"required":[]}""")
```

### Camera Operations

```kotlin
ToolSpec("camera", "Take a photo",
    """{"type":"object","properties":{"quality":{"type":"string","enum":["low","high"]}},"required":[]}""")
```

### Contact Queries

```kotlin
ToolSpec("contacts", "Query phone contacts",
    """{"type":"object","properties":{"query":{"type":"string"}},"required":["query"]}""")
```

### Sensor Data

```kotlin
ToolSpec("sensors", "Read sensor data",
    """{"type":"object","properties":{"type":{"type":"string","enum":["accelerometer","gyroscope","gps"]}},"required":["type"]}""")
```

## Error Handling

| Scenario | Handling |
|----------|----------|
| No tool handler registered | Return `tool_error` with message "No handler registered" |
| Tool execution throws exception | Catch exception, return `tool_error` with exception message |
| Client doesn't respond within 30s | Gateway returns timeout error to Agent |
| WebSocket disconnects | Remove all remote tools; agent won't call them |
| Unmatched call_id | Discard uncorrelatable responses |

## Security Considerations

- Remote tools **cannot** access server-side capabilities (Memory, SecurityPolicy, Provider)
- Tool parameters are validated by the client
- Gateway still intercepts tool calls through the Hook pipeline
- `before_tool_call` hooks can cancel remote tool calls
- Recommendation: Restrict registerable tool name ranges in `SecurityPolicy`
