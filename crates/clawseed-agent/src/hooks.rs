//! Hook system for tool-call interception.
//!
//! Hooks run before and after tool calls. SecurityPolicy is always registered
//! as a hook to enforce autonomy, rate limiting, and command allowlists.
//!
//! Hook factories allow declarative hook creation from config.

use std::collections::HashMap;

use clawseed_api::hook::{Hook, HookResult, ToolCall as HookToolCall, ToolExecutionResult};
use clawseed_config::schema::AutonomyConfig;

/// Result of the hook runner's before_tool_call pipeline.
///
/// This is separate from `HookResult` because the runner needs to return
/// the potentially-modified (name, arguments) tuple on `Continue`, while
/// the individual hook's `Continue` is a unit variant.
pub enum HookRunnerResult {
    /// All hooks passed — proceed with the (possibly modified) call.
    Continue {
        name: String,
        arguments: serde_json::Value,
    },
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
            match hook.after_tool_call(&exec_result) {
                HookResult::Continue => {}
                HookResult::Cancel(reason) => {
                    tracing::debug!(hook_name = %name, %reason, "after_tool_call returned Cancel (ignored, tool already executed)");
                }
                HookResult::Modify(_) => {
                    tracing::debug!(hook_name = %name, "after_tool_call returned Modify (ignored, tool already executed)");
                }
            }
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

/// Factory trait for creating hooks from config declarations.
pub trait HookFactory: Send + Sync {
    /// The hook type identifier this factory produces (e.g. "security_policy").
    fn hook_type(&self) -> &str;

    /// Create a hook from a config declaration.
    fn create(&self, config: &serde_json::Value) -> Option<Box<dyn Hook>>;
}

/// Registry of hook factories, keyed by hook type.
pub struct HookFactoryRegistry {
    factories: HashMap<String, Box<dyn HookFactory>>,
}

impl HookFactoryRegistry {
    pub fn new() -> Self {
        Self {
            factories: HashMap::new(),
        }
    }

    pub fn register(&mut self, factory: Box<dyn HookFactory>) {
        self.factories
            .insert(factory.hook_type().to_string(), factory);
    }

    /// Create a hook from a declaration. Returns None if the type is unknown.
    pub fn create_hook(
        &self,
        hook_type: &str,
        config: &serde_json::Value,
    ) -> Option<Box<dyn Hook>> {
        self.factories.get(hook_type).and_then(|f| f.create(config))
    }
}

impl Default for HookFactoryRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Built-in factory that creates SecurityPolicy as a Hook.
pub struct SecurityPolicyHookFactory;

impl HookFactory for SecurityPolicyHookFactory {
    fn hook_type(&self) -> &str {
        "security_policy"
    }

    fn create(&self, config: &serde_json::Value) -> Option<Box<dyn Hook>> {
        let autonomy: AutonomyConfig = if config.is_null() {
            AutonomyConfig::default()
        } else {
            serde_json::from_value(config.clone()).ok()?
        };
        let policy =
            crate::security::SecurityPolicy::from_config(&autonomy, std::path::Path::new("."));
        Some(Box::new(policy))
    }
}
