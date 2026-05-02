# Extending Skills (Tools) Tutorial

This tutorial covers how to write custom tools for ClawSeed.

## Tool Trait Recap

```rust
#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn parameters_schema(&self) -> serde_json::Value;
    async fn execute(&self, args: serde_json::Value, ctx: &dyn ToolContext) -> anyhow::Result<ToolResult>;
    fn spec(&self) -> ToolSpec { /* default implementation, usually no need to override */ }
}
```

## Step 1: The Simplest Tool

Create a stateless tool with no external dependencies. Example: `CalculatorTool`:

```rust
use async_trait::async_trait;
use clawseed_api::{Tool, ToolResult, ToolContext};
use serde_json::{Value, json};

pub struct CalculatorTool;

#[async_trait]
impl Tool for CalculatorTool {
    fn name(&self) -> &str { "calculator" }
    fn description(&self) -> &str {
        "Perform arithmetic calculations. Supports add, subtract, multiply, divide."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "function": {
                    "type": "string",
                    "enum": ["add", "subtract", "multiply", "divide"],
                    "description": "The operation to perform"
                },
                "a": { "type": "number", "description": "First operand" },
                "b": { "type": "number", "description": "Second operand" }
            },
            "required": ["function", "a", "b"]
        })
    }

    async fn execute(&self, args: Value, _ctx: &dyn ToolContext) -> anyhow::Result<ToolResult> {
        let function = args.get("function").and_then(|v| v.as_str()).unwrap_or("");
        let a = args.get("a").and_then(|v| v.as_f64()).unwrap_or(0.0);
        let b = args.get("b").and_then(|v| v.as_f64()).unwrap_or(0.0);

        let result = match function {
            "add" => a + b,
            "subtract" => a - b,
            "multiply" => a * b,
            "divide" => {
                if b == 0.0 {
                    return Ok(ToolResult {
                        success: false,
                        output: String::new(),
                        error: Some("Division by zero".into()),
                    });
                }
                a / b
            }
            _ => {
                return Ok(ToolResult {
                    success: false,
                    output: String::new(),
                    error: Some(format!("Unknown function: {function}")),
                });
            }
        };

        Ok(ToolResult {
            success: true,
            output: result.to_string(),
            error: None,
        })
    }
}
```

**Key points**:
- Struct is stateless, implements `Send + Sync`
- Parameters extracted from `serde_json::Value` using `.and_then()` for safe access
- Errors returned via `ToolResult { success: false, error: Some(...) }`, **never panic**
- The LLM understands the tool's purpose and parameters through `description` and `parameters_schema`

## Step 2: Accessing Workspace Files

File operation tools use `ToolContext`'s `workspace_dir()` to sandbox paths:

```rust
use std::path::Path;
use clawseed_api::{Tool, ToolResult, ToolContext};

pub struct FileReadTool;

#[async_trait]
impl Tool for FileReadTool {
    fn name(&self) -> &str { "file_read" }
    fn description(&self) -> &str { "Read the contents of a file within the workspace" }
    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "Relative path within workspace" }
            },
            "required": ["path"]
        })
    }

    async fn execute(&self, args: Value, ctx: &dyn ToolContext) -> anyhow::Result<ToolResult> {
        let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
        let full_path = ctx.workspace_dir().join(path);

        // Security check: path must be within workspace
        let canonical = match std::fs::canonicalize(&full_path) {
            Ok(p) => p,
            Err(e) => {
                return Ok(ToolResult {
                    success: false,
                    output: String::new(),
                    error: Some(format!("Cannot resolve path: {e}")),
                });
            }
        };

        let workspace_canon = std::fs::canonicalize(ctx.workspace_dir())
            .unwrap_or_else(|_| ctx.workspace_dir().to_path_buf());

        if !canonical.starts_with(&workspace_canon) {
            return Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some(format!("Path '{}' is outside workspace", path)),
            });
        }

        let content = std::fs::read_to_string(&canonical)?;
        Ok(ToolResult { success: true, output: content, error: None })
    }
}
```

**Key points**:
- Always use `ctx.workspace_dir()` to build paths
- Use `canonicalize` to prevent `../` and other path traversal attacks
- Verify the canonical path starts with the workspace prefix
- IO errors propagated via `?` to `anyhow::Result`

## Step 3: Using Capability Injection

Tools access runtime capabilities (security policy, memory, provider, etc.) via `ctx.get::<T>()`:

```rust
use clawseed_api::{Tool, ToolResult, ToolContext};
use clawseed_agent::security::SecurityPolicy;

pub struct ShellTool;

#[async_trait]
impl Tool for ShellTool {
    fn name(&self) -> &str { "shell" }
    fn description(&self) -> &str { "Execute a shell command" }
    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "command": { "type": "string", "description": "The shell command to execute" }
            },
            "required": ["command"]
        })
    }

    async fn execute(&self, args: Value, ctx: &dyn ToolContext) -> anyhow::Result<ToolResult> {
        let command = args.get("command").and_then(|v| v.as_str()).unwrap_or("");

        // Security policy check
        if let Some(policy) = ctx.get::<SecurityPolicy>() {
            if let Err(reason) = policy.check_command(command) {
                return Ok(ToolResult {
                    success: false,
                    output: String::new(),
                    error: Some(format!("Blocked by security policy: {reason}")),
                });
            }
        }
        // No security policy → no check (graceful degradation)

        // Execute command...
        let output = std::process::Command::new("sh")
            .arg("-c")
            .arg(command)
            .output()?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        Ok(ToolResult {
            success: output.status.success(),
            output: stdout,
            error: if stderr.is_empty() { None } else { Some(stderr) },
        })
    }
}
```

**Key points**:
- Use `if let Some(policy) = ctx.get::<SecurityPolicy>()` to check if a capability exists
- Gracefully degrade when capability is absent — don't error
- Common capabilities: `SecurityPolicy`, `dyn Memory`, `Provider`

## Step 4: Stateful Tools

Tools are stateless singletons by design. When state is needed, use `Arc<Mutex<T>>`:

```rust
use std::sync::{Arc, Mutex};
use async_trait::async_trait;
use clawseed_api::{Tool, ToolResult, ToolContext};
use serde_json::{Value, json};

pub struct CounterTool {
    count: Arc<Mutex<usize>>,
}

impl CounterTool {
    pub fn new() -> Self {
        Self { count: Arc::new(Mutex::new(0)) }
    }
}

#[async_trait]
impl Tool for CounterTool {
    fn name(&self) -> &str { "counter" }
    fn description(&self) -> &str { "Increment and read a counter" }
    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "action": { "type": "string", "enum": ["increment", "read"], "description": "Action to perform" }
            },
            "required": ["action"]
        })
    }

    async fn execute(&self, args: Value, _ctx: &dyn ToolContext) -> anyhow::Result<ToolResult> {
        let action = args.get("action").and_then(|v| v.as_str()).unwrap_or("read");
        let mut count = self.count.lock().unwrap();

        match action {
            "increment" => {
                *count += 1;
                Ok(ToolResult { success: true, output: count.to_string(), error: None })
            }
            "read" => {
                Ok(ToolResult { success: true, output: count.to_string(), error: None })
            }
            _ => Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some(format!("Unknown action: {action}")),
            }),
        }
    }
}
```

## Step 5: Registering Tools

Register new tools in the `all_tools()` function in `clawseed-tools/src/registry.rs`:

```rust
pub fn all_tools(workspace_dir: PathBuf, config: &Config) -> Vec<Box<dyn Tool>> {
    let mut tools: Vec<Box<dyn Tool>> = Vec::new();

    // ... existing tools ...

    // Add your tool
    tools.push(Box::new(MyTool::new()));

    tools
}
```

### Conditional Registration

Some tools can be conditionally enabled based on configuration:

```rust
if config.my_tool.enabled {
    tools.push(Box::new(MyTool::new(config.my_tool.max_items)));
}
```

### Config-Driven Control

```toml
[my_tool]
enabled = true
max_items = 100
```

## Step 6: Remote Tools (Android Client)

Android clients register tools over WebSocket — no server-side code changes needed:

```kotlin
val client = ClawseedClient(
    gatewayUrl = "ws://localhost:3000/ws/chat",
    tools = listOf(
        ToolSpec(
            "local_contacts",
            "Query phone contacts",
            contactsSchema  // JSONObject JSON Schema
        ),
    )
) { request ->
    when (request.name) {
        "local_contacts" -> ToolCallResult.Success(queryContacts(request.args))
        else -> ToolCallResult.Failure("unknown tool")
    }
}
client.connect()
```

Limitations of remote tools:
- No `ToolContext` access (no server-side memory, security policy, etc.)
- 30-second execution timeout
- Results transmitted over WebSocket

## Best Practices

1. **Validate parameters**: Extract all parameters safely using `.and_then()`, return `ToolResult::error` for missing params
2. **Sandbox file access**: File operations must be scoped to the workspace
3. **Degrade gracefully**: Skip features when capabilities are absent, don't error
4. **Clear descriptions**: `description` is the LLM's only way to understand the tool's purpose
5. **Accurate JSON Schema**: `parameters_schema` determines the parameter format the LLM generates
6. **Design for idempotency**: Tools should be idempotent where possible, avoiding unintended side effects
7. **Wrap errors**: Return all errors through `ToolResult::error`, never panic
8. **Prefer async**: Use async APIs like `tokio::fs` for IO operations
