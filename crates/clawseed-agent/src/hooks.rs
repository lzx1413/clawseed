//! Hook system for tool-call interception.
//!
//! Hooks run before and after tool calls. In the minimal clawseed-agent
//! crate, only a no-op runner is provided.

use clawseed_api::hook::{Hook, HookResult, ToolCall as HookToolCall, ToolExecutionResult};

/// Result of the hook runner's before_tool_call pipeline.
///
/// This is separate from `HookResult` because the runner needs to return
/// the potentially-modified (name, arguments) tuple on `Continue`, while
/// the individual hook's `Continue` is a unit variant.
pub enum HookRunnerResult {
    /// All hooks passed — proceed with the (possibly modified) call.
    Continue { name: String, arguments: serde_json::Value },
    /// A hook cancelled the call.
    Cancel(String),
}

/// Simple hook runner that iterates over registered hooks.
pub struct HookRunner {
    hooks: Vec<Box<dyn Hook>>,
}

impl HookRunner {
    pub fn new() -> Self {
        Self { hooks: Vec::new() }
    }

    pub fn register(&mut self, hook: Box<dyn Hook>) {
        self.hooks.push(hook);
    }

    pub async fn run_before_tool_call(
        &self,
        name: String,
        args: serde_json::Value,
    ) -> HookRunnerResult {
        let mut call = HookToolCall {
            id: String::new(),
            name,
            arguments: args,
        };
        for hook in &self.hooks {
            match hook.before_tool_call(&mut call) {
                HookResult::Continue => continue,
                HookResult::Cancel(reason) => return HookRunnerResult::Cancel(reason),
                HookResult::Modify(new_call) => {
                    call = new_call;
                }
            }
        }
        HookRunnerResult::Continue {
            name: call.name,
            arguments: call.arguments,
        }
    }

    pub async fn fire_after_tool_call(
        &self,
        name: &str,
        result: &clawseed_api::tool::ToolResult,
        _duration: std::time::Duration,
    ) {
        let exec_result = ToolExecutionResult {
            id: String::new(),
            name: name.to_string(),
            output: result.output.clone(),
            success: result.success,
        };
        for hook in &self.hooks {
            let _ = hook.after_tool_call(&exec_result);
        }
    }

    /// Fire gateway start hook (stub).
    pub async fn fire_gateway_start(&self, _host: &str, _port: u16) {
        // No-op in minimal crate
    }
}

impl Default for HookRunner {
    fn default() -> Self {
        Self::new()
    }
}
