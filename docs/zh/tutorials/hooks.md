# Hook 教程

本教程介绍如何使用 ClawSeed 的 Hook 系统拦截工具调用。

## Hook Trait

```rust
pub trait Hook: Send + Sync {
    fn before_tool_call(&self, call: &mut ToolCall) -> HookResult;
    fn after_tool_call(&self, result: &ToolExecutionResult) -> HookResult;
}
```

### 相关类型

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
    Continue,              // 放行，继续执行
    Cancel(String),        // 取消执行，附带原因
    Modify(ToolCall),      // 修改工具调用的名称或参数
}
```

## Hook 执行流程

```
工具调用请求
    ↓
Hook 1: before_tool_call()  → Continue ──→ Hook 2: before_tool_call()  → Continue ──→ 执行工具
                            → Cancel(reason) → 返回取消原因               → Modify(call) → 用修改后的 call 继续管线
    ↓
工具执行完成
    ↓
Hook 1: after_tool_call()   → Continue ──→ Hook 2: after_tool_call()   → Continue ──→ 返回结果
```

**关键规则**：
- Hook 按注册顺序执行
- 第一个 `Cancel` 停止整个管线
- `Modify` 修改后的调用传递给下一个 Hook
- `after_tool_call` 仅观察，通常返回 `Continue`

## 示例一：审计日志 Hook

记录所有工具调用的日志：

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

## 示例二：安全审批 Hook

对危险操作要求审批：

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
            // 在 Supervised 模式下，需要用户审批
            // 这里简单演示取消，实际实现会有审批流程
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

## 示例三：参数修改 Hook

在工具执行前修改参数：

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

## 示例四：速率限制 Hook

限制工具调用频率：

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

        // 清理超过 60 秒的记录
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

## 示例五：敏感信息脱敏 Hook

在工具输出中脱敏敏感信息：

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
                Regex::new(r"\b\d{16}\b").unwrap(),           // 信用卡号
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
        // 注意：after_tool_call 无法修改结果
        // 此处仅记录检测到的敏感信息
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

## 注册 Hook

Hook 在 Agent 构建时注册：

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
    .hook_runner(hook_runner)
    .build()?;
```

**注册顺序很重要**：Hook 按注册顺序执行。安全相关的 Hook 应该优先注册。

## Hook 与 SecurityPolicy 的关系

ClawSeed 内置的 `SecurityPolicy` 是通过能力注入实现的，而非 Hook。两者的区别：

| 机制 | 作用 | 实现方式 |
|------|------|---------|
| SecurityPolicy | 命令白名单、路径守卫、自主等级 | 通过 `ctx.get::<SecurityPolicy>()` 在工具内部检查 |
| Hook | 拦截、取消、修改工具调用 | 在工具执行前后全局拦截 |

**何时用 Hook**：
- 全局性拦截（如审计、速率限制）
- 需要修改工具调用参数
- 跨工具的策略（如审批流程）

**何时用 SecurityPolicy**：
- 工具内部的权限检查
- 命令白名单验证
- 路径安全守卫

## 最佳实践

1. **Hook 应该快速**：不要在 Hook 中执行耗时操作，特别是 `before_tool_call`
2. **避免副作用**：`before_tool_call` 的 `Modify` 应该只修改参数，不执行操作
3. **Cancel 要附带原因**：用户需要知道为什么操作被取消
4. **审计用 after_tool_call**：记录执行结果时用 after 而非 before
5. **安全 Hook 优先注册**：确保安全相关的 Hook 最先执行
6. **after_tool_call 保持 Continue**：除非有特殊需求，after hook 总是返回 Continue
