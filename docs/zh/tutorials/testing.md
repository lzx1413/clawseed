# 测试教程

本教程介绍如何为 ClawSeed 编写和运行测试。

## 测试基础设施

ClawSeed 的测试基础设施位于 `crates/clawseed-agent/tests/common/`：

```
tests/
├── common/
│   ├── mod.rs            # 共享模块声明
│   ├── helpers.rs        # 测试构建器
│   ├── mock_provider.rs  # Mock Provider
│   └── mock_tools.rs     # Mock Tool
├── agent_integration.rs  # 集成测试
├── agent_robustness.rs   # 健壮性测试
└── agent_system.rs       # 系统级测试
```

## Mock Provider

### MockProvider — 脚本化响应

按 FIFO 顺序返回预定义的响应：

```rust
use clawseed_agent::tests::common::mock_provider::{MockProvider, text_response, tool_response};
use clawseed_api::{ChatResponse, ToolCall};

// 创建返回一系列响应的 Mock Provider
let provider = Box::new(MockProvider::new(vec![
    // 第一次调用：返回工具调用
    tool_response(vec![ToolCall {
        id: "tc1".into(),
        name: "echo".into(),
        arguments: r#"{"message": "hello"}"#.into(),
    }]),
    // 第二次调用：返回纯文本
    text_response("Tool executed successfully"),
]));
```

### RecordingProvider — 记录请求

除了返回脚本化响应，还记录所有收到的请求，用于断言：

```rust
use clawseed_agent::tests::common::mock_provider::RecordingProvider;

let recorded = Arc::new(Mutex::new(Vec::new()));
let provider = Box::new(RecordingProvider::new(
    vec![text_response("Hello")],
    recorded.clone(),
));

// ... 执行测试 ...

// 断言发送给 Provider 的消息
let requests = recorded.lock().unwrap();
assert_eq!(requests.len(), 1);
```

### 辅助函数

```rust
// 创建纯文本响应
pub fn text_response(text: &str) -> ChatResponse { ... }

// 创建工具调用响应
pub fn tool_response(calls: Vec<ToolCall>) -> ChatResponse { ... }
```

## Mock Tools

### EchoTool — 回显工具

将参数原样返回：

```rust
pub struct EchoTool;

impl Tool for EchoTool {
    fn name(&self) -> &str { "echo" }
    fn description(&self) -> &str { "Echo back the input" }
    fn parameters_schema(&self) -> Value { json!({ "type": "object", "properties": { "message": { "type": "string" } } }) }
    async fn execute(&self, args: Value, _ctx: &dyn ToolContext) -> anyhow::Result<ToolResult> {
        let msg = args.get("message").and_then(|v| v.as_str()).unwrap_or("");
        Ok(ToolResult { success: true, output: msg.to_string(), error: None })
    }
}
```

### CountingTool — 计数工具

追踪工具被调用的次数：

```rust
pub struct CountingTool {
    count: Arc<Mutex<usize>>,
}

impl CountingTool {
    pub fn new(count: Arc<Mutex<usize>>) -> Self { Self { count } }
    pub fn count(&self) -> usize { *self.count.lock().unwrap() }
}
```

### RecordingTool — 记录参数

捕获工具调用的参数用于断言：

```rust
pub struct RecordingTool {
    name: String,
    calls: Arc<Mutex<Vec<Value>>>,
}

impl RecordingTool {
    pub fn new(name: &str, calls: Arc<Mutex<Vec<Value>>>) -> Self { ... }
    pub fn calls(&self) -> Vec<Value> { self.calls.lock().unwrap().clone() }
}
```

### FailingTool — 模拟失败

总是返回错误，测试错误处理路径：

```rust
pub struct FailingTool;

impl Tool for FailingTool {
    fn name(&self) -> &str { "failing" }
    async fn execute(&self, _args: Value, _ctx: &dyn ToolContext) -> anyhow::Result<ToolResult> {
        Ok(ToolResult { success: false, output: String::new(), error: Some("Intentional failure".into()) })
    }
}
```

## Agent 构建器

### 基本构建

```rust
use clawseed_agent::tests::common::helpers::build_agent;

let agent = build_agent(provider, vec![Box::new(EchoTool)]);
```

`build_agent` 内部创建 `NoneMemory`（内存）和 `NoopObserver`，使用 `NativeToolDispatcher`。

### 带 SQLite 记忆

```rust
use clawseed_agent::tests::common::helpers::build_agent_with_sqlite_memory;

let temp_dir = tempfile::tempdir().unwrap();
let agent = build_agent_with_sqlite_memory(provider, vec![Box::new(EchoTool)], temp_dir.path());
```

使用真实的 SQLite 后端，适合测试记忆相关的集成。

### 手动构建

需要更精细控制时，直接使用 `Agent::builder()`：

```rust
let agent = Agent::builder()
    .provider(provider)
    .tools(vec![Box::new(EchoTool), Box::new(CountingTool::new(count))])
    .memory(Arc::new(NoneMemory))
    .observer(Box::new(NoopObserver))
    .tool_dispatcher(Box::new(NativeToolDispatcher))
    .workspace_dir(std::env::temp_dir())
    .capability(Arc::new(SecurityPolicy::read_only()))
    .build()
    .unwrap();
```

## 编写集成测试

### 单工具调用周期

```rust
#[tokio::test]
async fn test_single_tool_call() {
    let provider = Box::new(MockProvider::new(vec![
        tool_response(vec![ToolCall {
            id: "tc1".into(),
            name: "echo".into(),
            arguments: r#"{"message": "hello"}"#.into(),
        }]),
        text_response("Echo: hello"),
    ]));

    let agent = build_agent(provider, vec![Box::new(EchoTool)]);
    let response = agent.turn("run echo with hello").await.unwrap();

    assert!(response.contains("Echo: hello"));
}
```

### 多工具调用

```rust
#[tokio::test]
async fn test_multiple_tool_calls() {
    let count = Arc::new(Mutex::new(0));
    let provider = Box::new(MockProvider::new(vec![
        tool_response(vec![
            ToolCall { id: "tc1".into(), name: "counter".into(), arguments: "{}".into() },
            ToolCall { id: "tc2".into(), name: "counter".into(), arguments: "{}".into() },
        ]),
        text_response("Done"),
    ]));

    let agent = build_agent(provider, vec![Box::new(CountingTool::new(count.clone()))]);
    agent.turn("run counter twice").await.unwrap();

    assert_eq!(*count.lock().unwrap(), 2);
}
```

### 工具错误处理

```rust
#[tokio::test]
async fn test_tool_failure_handling() {
    let provider = Box::new(MockProvider::new(vec![
        tool_response(vec![ToolCall {
            id: "tc1".into(),
            name: "failing".into(),
            arguments: "{}".into(),
        }]),
        text_response("Tool failed, but I recovered"),
    ]));

    let agent = build_agent(provider, vec![Box::new(FailingTool)]);
    let response = agent.turn("run failing tool").await.unwrap();

    assert!(response.contains("recovered"));
}
```

### Hook 测试

```rust
#[tokio::test]
async fn test_hook_cancels_tool() {
    let provider = Box::new(MockProvider::new(vec![
        tool_response(vec![ToolCall {
            id: "tc1".into(),
            name: "echo".into(),
            arguments: r#"{"message": "hello"}"#.into(),
        }]),
        text_response("Tool was cancelled"),
    ]));

    let mut hook_runner = HookRunner::new();
    hook_runner.register(Box::new(CancelEchoHook)); // 取消 echo 工具的 Hook

    let mut agent = build_agent(provider, vec![Box::new(EchoTool)]);
    agent.set_hook_runner(hook_runner);

    let response = agent.turn("run echo").await.unwrap();
    assert!(response.contains("cancelled"));
}
```

### 记忆集成测试

```rust
#[tokio::test]
async fn test_memory_store_and_recall() {
    let temp_dir = tempfile::tempdir().unwrap();
    let provider = Box::new(MockProvider::new(vec![
        tool_response(vec![ToolCall {
            id: "tc1".into(),
            name: "memory_store".into(),
            arguments: r#"{"content": "test memory", "category": "context"}"#.into(),
        }]),
        text_response("Stored"),
    ]));

    let agent = build_agent_with_sqlite_memory(provider, memory_tools(), temp_dir.path());
    let response = agent.turn("store a memory").await.unwrap();

    assert!(response.contains("Stored"));
}
```

## 编写单元测试

### 测试工具逻辑

```rust
#[tokio::test]
async fn test_calculator_divide() {
    let tool = CalculatorTool::new();
    let ctx = MockToolContext::new(); // 需要实现 ToolContext

    let result = tool.execute(
        json!({"function": "divide", "a": 10, "b": 2}),
        &ctx,
    ).await.unwrap();

    assert!(result.success);
    assert_eq!(result.output, "5");
}

#[tokio::test]
async fn test_calculator_divide_by_zero() {
    let tool = CalculatorTool::new();
    let ctx = MockToolContext::new();

    let result = tool.execute(
        json!({"function": "divide", "a": 10, "b": 0}),
        &ctx,
    ).await.unwrap();

    assert!(!result.success);
    assert!(result.error.unwrap().contains("zero"));
}
```

### 测试 Hook

```rust
#[test]
fn test_approval_hook_cancels_dangerous_tool() {
    let hook = ApprovalHook::new();
    let mut call = ToolCall {
        id: "1".into(),
        name: "shell".into(),
        arguments: json!({"command": "rm -rf /"}),
    };

    let result = hook.before_tool_call(&mut call);
    assert!(matches!(result, HookResult::Cancel(_)));
}

#[test]
fn test_approval_hook_allows_safe_tool() {
    let hook = ApprovalHook::new();
    let mut call = ToolCall {
        id: "1".into(),
        name: "file_read".into(),
        arguments: json!({"path": "test.txt"}),
    };

    let result = hook.before_tool_call(&mut call);
    assert!(matches!(result, HookResult::Continue));
}
```

## 运行测试

```bash
# 运行所有测试
cargo test

# 运行特定 crate 的测试
cargo test -p clawseed-agent

# 运行特定测试
cargo test -p clawseed-agent test_single_tool_call

# 运行集成测试（需要 --test 标志）
cargo test -p clawseed-agent --test agent_integration

# 显示输出（不截断）
cargo test -- --nocapture

# 运行被忽略的测试
cargo test -- --ignored
```

## 测试分类

| 类型 | 位置 | 特点 |
|------|------|------|
| 单元测试 | `src/` 文件内 `#[cfg(test)] mod tests` | 测试单个函数/结构体 |
| 集成测试 | `tests/agent_integration.rs` | 测试完整 Agent 循环 |
| 健壮性测试 | `tests/agent_robustness.rs` | 测试错误处理和边界情况 |
| 系统测试 | `tests/agent_system.rs` | 测试真实后端（SQLite 等） |

## CI 配置

项目的 CI profile 配置了更快的编译：

```toml
[profile.ci]
inherits = "release"
lto = "thin"
codegen-units = 16
```

CI 中建议使用：

```bash
cargo test --profile ci
```
