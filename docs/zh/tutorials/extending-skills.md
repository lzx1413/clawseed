# 扩展技能（Tool）教程

本教程介绍如何为 ClawSeed 编写自定义工具（Tool）。

## Tool Trait 回顾

```rust
#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn parameters_schema(&self) -> serde_json::Value;
    async fn execute(&self, args: serde_json::Value, ctx: &dyn ToolContext) -> anyhow::Result<ToolResult>;
    fn spec(&self) -> ToolSpec { /* 默认实现，一般不需要覆盖 */ }
}
```

## 第一步：最简单的工具

创建一个无状态、无外部依赖的工具。以 `CalculatorTool` 为例：

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

**要点**：
- 结构体无状态，实现 `Send + Sync`
- 参数从 `serde_json::Value` 中提取，使用 `.and_then()` 安全访问
- 错误通过 `ToolResult { success: false, error: Some(...) }` 返回，**不要 panic**
- LLM 通过 `description` 和 `parameters_schema` 理解工具的用途和参数

## 第二步：访问工作区文件

文件操作工具需要使用 `ToolContext` 的 `workspace_dir()` 来沙箱化路径：

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

        // 安全检查：路径必须在 workspace 内
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

**要点**：
- 始终使用 `ctx.workspace_dir()` 拼接路径
- 使用 `canonicalize` 防止 `../` 等路径穿越
- 验证 canonical 路径以 workspace 为前缀
- IO 错误通过 `?` 传播给 `anyhow::Result`

## 第三步：使用能力注入

工具通过 `ctx.get::<T>()` 访问运行时能力（如安全策略、记忆、Provider 等）：

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

        // 安全策略检查
        if let Some(policy) = ctx.get::<SecurityPolicy>() {
            if let Err(reason) = policy.check_command(command) {
                return Ok(ToolResult {
                    success: false,
                    output: String::new(),
                    error: Some(format!("Blocked by security policy: {reason}")),
                });
            }
        }
        // 没有安全策略则不检查（优雅降级）

        // 执行命令...
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

**要点**：
- 使用 `if let Some(policy) = ctx.get::<SecurityPolicy>()` 检查能力是否存在
- 能力不存在时优雅降级，不要报错
- 常用能力：`SecurityPolicy`、`dyn Memory`、`Provider`

## 第四步：带状态的工具

工具本身是无状态单例，需要状态时使用 `Arc<Mutex<T>>`：

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

## 第五步：注册工具

在 `clawseed-tools/src/registry.rs` 的 `all_tools()` 函数中注册新工具：

```rust
pub fn all_tools(workspace_dir: PathBuf, config: &Config) -> Vec<Box<dyn Tool>> {
    let mut tools: Vec<Box<dyn Tool>> = Vec::new();

    // ... 现有工具 ...

    // 添加你的工具
    tools.push(Box::new(MyTool::new()));

    tools
}
```

### 条件注册

某些工具可以根据配置决定是否启用：

```rust
if config.my_tool.enabled {
    tools.push(Box::new(MyTool::new(config.my_tool.max_items)));
}
```

### 在配置中控制

```toml
[my_tool]
enabled = true
max_items = 100
```

## 第六步：远程工具（Android 客户端）

Android 客户端通过 WebSocket 注册工具，无需修改服务端代码：

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

远程工具的限制：
- 不使用 `ToolContext`（无法访问服务端记忆、安全策略等）
- 执行超时 30 秒
- 结果通过 WebSocket 传输

## 最佳实践

1. **参数验证**：所有参数使用 `.and_then()` 安全提取，缺失参数返回 `ToolResult::error`
2. **沙箱化**：文件操作必须限制在 workspace 内
3. **优雅降级**：能力不存在时跳过功能，不要报错
4. **描述清晰**：`description` 是 LLM 理解工具用途的唯一途径
5. **JSON Schema 准确**：`parameters_schema` 决定 LLM 生成的参数格式
6. **幂等设计**：工具应尽量幂等，避免副作用
7. **错误封装**：所有错误通过 `ToolResult::error` 返回，不要 panic
8. **异步优先**：IO 操作使用 `tokio::fs` 等异步 API
