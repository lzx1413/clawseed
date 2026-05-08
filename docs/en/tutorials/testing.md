# Testing Tutorial

This tutorial covers how to write and run tests for ClawSeed.

## Test Infrastructure

ClawSeed's test infrastructure lives in `crates/clawseed-agent/tests/common/`:

```
tests/
├── common/
│   ├── mod.rs            # Shared module declarations
│   ├── helpers.rs        # Test builders
│   ├── mock_provider.rs  # Mock Provider
│   └── mock_tools.rs     # Mock Tool
├── agent_integration.rs  # Integration tests
├── agent_robustness.rs   # Robustness tests
└── agent_system.rs       # System-level tests
```

## Mock Provider

### MockProvider — Scripted Responses

Returns predefined responses in FIFO order:

```rust
use clawseed_agent::tests::common::mock_provider::{MockProvider, text_response, tool_response};
use clawseed_api::{ChatResponse, ToolCall};

// Create a Mock Provider that returns a sequence of responses
let provider = Box::new(MockProvider::new(vec![
    // First call: return a tool call
    tool_response(vec![ToolCall {
        id: "tc1".into(),
        name: "echo".into(),
        arguments: r#"{"message": "hello"}"#.into(),
    }]),
    // Second call: return text-only
    text_response("Tool executed successfully"),
]));
```

### RecordingProvider — Request Recording

In addition to returning scripted responses, it records all received requests for assertion:

```rust
use clawseed_agent::tests::common::mock_provider::RecordingProvider;

let recorded = Arc::new(Mutex::new(Vec::new()));
let provider = Box::new(RecordingProvider::new(
    vec![text_response("Hello")],
    recorded.clone(),
));

// ... run test ...

// Assert messages sent to the Provider
let requests = recorded.lock().unwrap();
assert_eq!(requests.len(), 1);
```

### Helper Functions

```rust
// Create a text-only response
pub fn text_response(text: &str) -> ChatResponse { ... }

// Create a tool call response
pub fn tool_response(calls: Vec<ToolCall>) -> ChatResponse { ... }
```

## Mock Tools

### EchoTool — Echo Tool

Returns arguments as-is:

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

### CountingTool — Counting Tool

Tracks how many times the tool was called:

```rust
pub struct CountingTool {
    count: Arc<Mutex<usize>>,
}

impl CountingTool {
    pub fn new(count: Arc<Mutex<usize>>) -> Self { Self { count } }
    pub fn count(&self) -> usize { *self.count.lock().unwrap() }
}
```

### RecordingTool — Argument Recording

Captures tool call arguments for assertion:

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

### FailingTool — Simulated Failure

Always returns an error, for testing error-handling paths:

```rust
pub struct FailingTool;

impl Tool for FailingTool {
    fn name(&self) -> &str { "failing" }
    async fn execute(&self, _args: Value, _ctx: &dyn ToolContext) -> anyhow::Result<ToolResult> {
        Ok(ToolResult { success: false, output: String::new(), error: Some("Intentional failure".into()) })
    }
}
```

## Agent Builders

### Basic Builder

```rust
use clawseed_agent::tests::common::helpers::build_agent;

let agent = build_agent(provider, vec![Box::new(EchoTool)]);
```

`build_agent` internally creates `NoneMemory` and `NoopObserver`, using `NativeToolDispatcher`.

### With SQLite Memory

```rust
use clawseed_agent::tests::common::helpers::build_agent_with_sqlite_memory;

let temp_dir = tempfile::tempdir().unwrap();
let agent = build_agent_with_sqlite_memory(provider, vec![Box::new(EchoTool)], temp_dir.path());
```

Uses a real SQLite backend, suitable for testing memory-related integrations.

### Manual Builder

For finer control, use `Agent::builder()` directly:

```rust
let agent = Agent::builder()
    .provider(provider)
    .tools(vec![Box::new(EchoTool), Box::new(CountingTool::new(count))])
    .memory(Arc::new(NoneMemory))
    .observer(Arc::new(crate::observer::NoopObserver))
    .tool_dispatcher(Box::new(NativeToolDispatcher))
    .workspace_dir(std::env::temp_dir())
    .build()
    .unwrap();
```

## Writing Integration Tests

### Single Tool Call Cycle

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

### Multiple Tool Calls

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

### Tool Error Handling

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

### Hook Testing

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
    hook_runner.register(Box::new(CancelEchoHook)); // Hook that cancels the echo tool

    let mut agent = build_agent(provider, vec![Box::new(EchoTool)]);
    agent.set_hook_runner(hook_runner);

    let response = agent.turn("run echo").await.unwrap();
    assert!(response.contains("cancelled"));
}
```

### Memory Integration Test

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

## Writing Unit Tests

### Testing Tool Logic

```rust
#[tokio::test]
async fn test_calculator_divide() {
    let tool = CalculatorTool::new();
    let ctx = MockToolContext::new(); // Need to implement ToolContext

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

### Testing Hooks

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

## Running Tests

```bash
# Run all tests
cargo test

# Run tests for a specific crate
cargo test -p clawseed-agent

# Run a specific test
cargo test -p clawseed-agent test_single_tool_call

# Run integration tests (requires --test flag)
cargo test -p clawseed-agent --test agent_integration

# Show output (no truncation)
cargo test -- --nocapture

# Run ignored tests
cargo test -- --ignored
```

## Test Categories

| Type | Location | Characteristics |
|------|----------|-----------------|
| Unit tests | `#[cfg(test)] mod tests` within `src/` files | Test individual functions/structs |
| Integration tests | `tests/agent_integration.rs` | Test full agent cycles |
| Robustness tests | `tests/agent_robustness.rs` | Test error handling and edge cases |
| System tests | `tests/agent_system.rs` | Test with real backends (SQLite, etc.) |

## CI Configuration

The project's CI profile is configured for faster compilation:

```toml
[profile.ci]
inherits = "release"
lto = "thin"
codegen-units = 16
```

Recommended for CI:

```bash
cargo test --profile ci
```
