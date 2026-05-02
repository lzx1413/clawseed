# Hook Tutorial

This tutorial covers how to use ClawSeed's Hook system to intercept tool calls.

## Hook Trait

```rust
pub trait Hook: Send + Sync {
    fn before_tool_call(&self, call: &mut ToolCall) -> HookResult;
    fn after_tool_call(&self, result: &ToolExecutionResult) -> HookResult;
}
```

### Related Types

```rust
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: Value,
}

pub struct ToolExecutionResult {
    pub id: String,
    pub name: String,
    pub output: String,
    pub success: bool,
}

pub enum HookResult {
    Continue,              // Allow execution to proceed
    Cancel(String),        // Cancel execution with a reason
    Modify(ToolCall),      // Modify the tool call's name or arguments
}
```

## Hook Execution Flow

```
Tool call request
    ↓
Hook 1: before_tool_call()  → Continue ──→ Hook 2: before_tool_call()  → Continue ──→ Execute tool
                            → Cancel(reason) → return cancel reason       → Modify(call) → continue with modified call
    ↓
Tool execution complete
    ↓
Hook 1: after_tool_call()   → Continue ──→ Hook 2: after_tool_call()   → Continue ──→ Return result
```

**Key rules**:
- Hooks execute in registration order
- The first `Cancel` stops the entire pipeline
- `Modify` passes the modified call to the next hook
- `after_tool_call` is observation-only, typically returns `Continue`

## Example 1: Audit Logging Hook

Log all tool calls:

```rust
use clawseed_api::{Hook, HookResult, ToolCall, ToolExecutionResult};
use log::info;

pub struct AuditHook;

impl Hook for AuditHook {
    fn before_tool_call(&self, call: &mut ToolCall) -> HookResult {
        info!("Tool call started: name={}, args={}", call.name, call.arguments);
        HookResult::Continue
    }

    fn after_tool_call(&self, result: &ToolExecutionResult) -> HookResult {
        info!(
            "Tool call completed: name={}, success={}, output_len={}",
            result.name,
            result.success,
            result.output.len()
        );
        HookResult::Continue
    }
}
```

## Example 2: Security Approval Hook

Require approval for dangerous operations:

```rust
use clawseed_api::{Hook, HookResult, ToolCall, ToolExecutionResult};
use std::collections::HashSet;

pub struct ApprovalHook {
    dangerous_tools: HashSet<String>,
}

impl ApprovalHook {
    pub fn new() -> Self {
        Self {
            dangerous_tools: vec!["shell", "file_write", "file_edit"]
                .into_iter()
                .map(String::from)
                .collect(),
        }
    }
}

impl Hook for ApprovalHook {
    fn before_tool_call(&self, call: &mut ToolCall) -> HookResult {
        if self.dangerous_tools.contains(&call.name) {
            // In Supervised mode, user approval is required
            // This example simply cancels; a real implementation would have an approval flow
            HookResult::Cancel(format!(
                "Tool '{}' requires approval. Please confirm to proceed.",
                call.name
            ))
        } else {
            HookResult::Continue
        }
    }

    fn after_tool_call(&self, _result: &ToolExecutionResult) -> HookResult {
        HookResult::Continue
    }
}
```

## Example 3: Parameter Modification Hook

Modify arguments before tool execution:

```rust
use clawseed_api::{Hook, HookResult, ToolCall, ToolExecutionResult};
use serde_json::Value;

pub struct PathPrefixHook {
    prefix: String,
}

impl PathPrefixHook {
    pub fn new(prefix: String) -> Self {
        Self { prefix }
    }
}

impl Hook for PathPrefixHook {
    fn before_tool_call(&self, call: &mut ToolCall) -> HookResult {
        if call.name == "file_read" || call.name == "file_write" {
            let mut modified = call.clone();
            if let Some(path) = modified.arguments.get("path").and_then(|v| v.as_str()) {
                if !path.starts_with('/') {
                    modified.arguments = serde_json::json!({
                        "path": format!("{}/{}", self.prefix.trim_end_matches('/'), path)
                    });
                }
            }
            HookResult::Modify(modified)
        } else {
            HookResult::Continue
        }
    }

    fn after_tool_call(&self, _result: &ToolExecutionResult) -> HookResult {
        HookResult::Continue
    }
}
```

## Example 4: Rate Limiting Hook

Limit tool call frequency:

```rust
use clawseed_api::{Hook, HookResult, ToolCall, ToolExecutionResult};
use std::sync::Mutex;
use std::time::Instant;

pub struct RateLimitHook {
    max_per_minute: usize,
    timestamps: Mutex<Vec<Instant>>,
}

impl RateLimitHook {
    pub fn new(max_per_minute: usize) -> Self {
        Self {
            max_per_minute,
            timestamps: Mutex::new(Vec::new()),
        }
    }
}

impl Hook for RateLimitHook {
    fn before_tool_call(&self, _call: &mut ToolCall) -> HookResult {
        let now = Instant::now();
        let mut timestamps = self.timestamps.lock().unwrap();

        // Remove entries older than 60 seconds
        timestamps.retain(|t| now.duration_since(*t).as_secs() < 60);

        if timestamps.len() >= self.max_per_minute {
            HookResult::Cancel(format!(
                "Rate limit exceeded: max {} calls per minute",
                self.max_per_minute
            ))
        } else {
            timestamps.push(now);
            HookResult::Continue
        }
    }

    fn after_tool_call(&self, _result: &ToolExecutionResult) -> HookResult {
        HookResult::Continue
    }
}
```

## Example 5: Sensitive Data Redaction Hook

Detect sensitive information in tool output:

```rust
use clawseed_api::{Hook, HookResult, ToolCall, ToolExecutionResult};
use regex::Regex;

pub struct RedactionHook {
    patterns: Vec<Regex>,
}

impl RedactionHook {
    pub fn new() -> Self {
        Self {
            patterns: vec![
                Regex::new(r"\b\d{16}\b").unwrap(),            // Credit card number
                Regex::new(r"\b\d{3}-\d{2}-\d{4}\b").unwrap(), // SSN
            ],
        }
    }
}

impl Hook for RedactionHook {
    fn before_tool_call(&self, _call: &mut ToolCall) -> HookResult {
        HookResult::Continue
    }

    fn after_tool_call(&self, result: &ToolExecutionResult) -> HookResult {
        // Note: after_tool_call cannot modify results
        // Here we only log detected sensitive information
        for pattern in &self.patterns {
            if pattern.is_match(&result.output) {
                log::warn!(
                    "Sensitive data detected in output of tool '{}'",
                    result.name
                );
            }
        }
        HookResult::Continue
    }
}
```

## Registering Hooks

Hooks are registered during Agent construction:

```rust
use clawseed_agent::Agent;
use clawseed_agent::hooks::HookRunner;

let mut hook_runner = HookRunner::new();
hook_runner.register(Box::new(AuditHook));
hook_runner.register(Box::new(RateLimitHook::new(60)));
hook_runner.register(Box::new(ApprovalHook::new()));

let agent = Agent::builder()
    .provider(provider)
    .tools(tools)
    .hook_runner(Some(Arc::new(hook_runner)))
    .build()?;
```

**Registration order matters**: Hooks execute in registration order. When building via `from_config()`, `SecurityPolicy` is always auto-registered as the first hook in the pipeline.

## Declarative Hook Chain

In addition to code registration, hooks can be created declaratively from config:

```toml
[hooks]
enabled = true

[[hooks.chain]]
type = "security_policy"

[[hooks.chain]]
type = "audit_log"
config = { level = "info" }
```

This relies on the `HookFactory` trait and `HookFactoryRegistry`:

```rust
pub trait HookFactory: Send + Sync {
    fn hook_type(&self) -> &str;
    fn create(&self, config: &serde_json::Value) -> Option<Box<dyn Hook>>;
}

pub struct HookFactoryRegistry {
    factories: HashMap<String, Box<dyn HookFactory>>,
}
```

`Agent::from_config()` iterates over `hooks.chain` during construction, using registered factories to create Hook instances and add them to the pipeline.

## Hooks vs. SecurityPolicy

`SecurityPolicy` intercepts all tool calls by implementing the `Hook` trait directly, rather than being injected as a Capability:

| Check | Location |
|-------|----------|
| Autonomy level (ReadOnly blocks all writes) | `before_tool_call()` |
| Rate limiting (`max_actions_per_hour`) | `before_tool_call()` |
| Command allowlists (shell/exec tools) | `before_tool_call()` |
| Path guards (`/etc/passwd`, etc.) | `before_tool_call()` |
| Action counting | `after_tool_call()` |

```rust
impl Hook for SecurityPolicy {
    fn before_tool_call(&self, call: &mut ToolCall) -> HookResult {
        if !self.can_act() {
            return HookResult::Cancel("Autonomy level is read-only".into());
        }
        if self.is_rate_limited() {
            return HookResult::Cancel("Action rate limit exceeded".into());
        }
        if call.name == "shell" || call.name == "exec" {
            if let Some(cmd) = call.arguments.get("command").and_then(|v| v.as_str()) {
                if let Some(forbidden) = self.forbidden_path_argument(cmd) {
                    return HookResult::Cancel(format!("Forbidden path: {forbidden}"));
                }
                if !self.is_command_allowed(cmd) {
                    return HookResult::Cancel(format!("Command not allowed: {cmd}"));
                }
            }
        }
        HookResult::Continue
    }

    fn after_tool_call(&self, _result: &ToolExecutionResult) -> HookResult {
        self.record_action();
        HookResult::Continue
    }
}
```

In `from_config()`, SecurityPolicy is always auto-registered as the first hook in the pipeline, ensuring security checks run before any user hooks.

**When to use Hooks**:
- Global interception (e.g., auditing, rate limiting)
- Need to modify tool call arguments
- Cross-tool policies (e.g., approval workflows)

**When to use Capability injection**:
- Tools need access to runtime services (e.g., Memory, Provider)
- Tools need to perform their own fine-grained checks

## Best Practices

1. **Hooks should be fast**: Don't perform expensive operations in hooks, especially in `before_tool_call`
2. **Avoid side effects**: `Modify` in `before_tool_call` should only change arguments, not execute operations
3. **Include a reason with Cancel**: Users need to know why an operation was cancelled
4. **Use after_tool_call for auditing**: Record execution results with after, not before
5. **Register security hooks first**: Ensure security-related hooks execute first
6. **Keep after_tool_call as Continue**: Unless you have a specific need, after hooks should always return Continue
