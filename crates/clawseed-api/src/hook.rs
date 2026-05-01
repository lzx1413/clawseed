//! Hook trait — the core extension mechanism.
//!
//! Extensions register as hooks to intercept tool calls without modifying agent code.
//!
//! # Design
//!
//! SecurityPolicy, SOP approval, Trust checks, etc. all register as hooks.
//! The agent code never changes — only the hook list grows.

use serde_json::Value;

/// A tool call to be potentially modified or cancelled by hooks.
#[derive(Debug, Clone)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: Value,
}

/// Result of a tool execution, passed to `after_tool_call`.
#[derive(Debug, Clone)]
pub struct ToolExecutionResult {
    pub id: String,
    pub name: String,
    pub output: String,
    pub success: bool,
}

/// Result of a hook intercepting a tool call.
pub enum HookResult {
    /// Allow the tool call to proceed.
    Continue,
    /// Cancel the tool call with a reason.
    Cancel(String),
    /// Modify the tool call before execution.
    Modify(ToolCall),
}

/// Hook trait — extensions implement this to intercept tool calls.
pub trait Hook: Send + Sync {
    /// Called before a tool is executed. Can cancel or modify the call.
    fn before_tool_call(&self, call: &mut ToolCall) -> HookResult;

    /// Called after a tool is executed. Can observe or react to results.
    fn after_tool_call(&self, result: &ToolExecutionResult) -> HookResult;
}
