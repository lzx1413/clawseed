//! Tools module stub — re-exports from clawseed-tools.
//!
//! The gateway references `clawseed_agent::tools::CanvasStore` and
//! `clawseed_agent::tools::all_tools_with_runtime` etc.
//! CanvasStore lives in clawseed-tools; the rest are stubs.

// Re-export CanvasStore from clawseed-tools
pub use clawseed_tools::canvas::CanvasStore;

use clawseed_api::tool::{Tool, ToolResult, ToolSpec};
use clawseed_api::tool_context::ToolContext;
use std::sync::Arc;

/// Wire built-in tools from clawseed_tools into the gateway.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn all_tools_with_runtime(
    _config: Arc<clawseed_config::schema::Config>,
    _security: &crate::security::SecurityPolicy,
    _runtime: Box<dyn std::any::Any>,
    _memory: Arc<dyn clawseed_api::memory_traits::Memory>,
    _composio_key: Option<&str>,
    _composio_entity_id: Option<&str>,
    _browser_config: &clawseed_config::schema::Config,
    _http_request_config: &clawseed_config::schema::Config,
    _web_fetch_config: &clawseed_config::schema::Config,
    workspace_dir: &std::path::Path,
    _agents: &clawseed_config::schema::Config,
    _api_key: Option<&str>,
    _cfg: &clawseed_config::schema::Config,
    _canvas_store: Option<CanvasStore>,
) -> (
    Vec<Box<dyn Tool>>,
    Option<Arc<parking_lot::RwLock<Vec<Arc<dyn DynTool>>>>>,
    Option<Arc<parking_lot::RwLock<Vec<Arc<dyn clawseed_api::channel::Channel>>>>>,
    Option<Arc<parking_lot::RwLock<Vec<Arc<dyn DynTool>>>>>,
    Option<Arc<parking_lot::RwLock<Vec<Arc<dyn DynTool>>>>>,
    Option<Arc<parking_lot::RwLock<Vec<Arc<dyn DynTool>>>>>,
) {
    let tools =
        clawseed_tools::registry::all_tools(workspace_dir.to_path_buf(), &_config, _memory);
    (tools, None, None, None, None, None)
}

/// Dynamic tool trait (object-safe version for Arc usage).
pub trait DynTool: Tool + Send + Sync {}
impl<T: Tool + Send + Sync> DynTool for T {}

/// MCP registry stub.
pub struct McpRegistry;

impl McpRegistry {
    pub async fn connect_all(_servers: &[clawseed_config::schema::Config]) -> anyhow::Result<Self> {
        Ok(Self)
    }

    pub fn tool_names(&self) -> Vec<String> {
        Vec::new()
    }

    pub async fn get_tool_def(&self, _name: &str) -> Option<ToolSpec> {
        None
    }

    pub fn server_count(&self) -> usize {
        0
    }
}

/// Deferred MCP tool set stub.
pub struct DeferredMcpToolSet;

impl DeferredMcpToolSet {
    pub async fn from_registry(_registry: Arc<McpRegistry>) -> Self {
        Self
    }

    pub fn len(&self) -> usize {
        0
    }

    pub fn is_empty(&self) -> bool {
        true
    }
}

/// Activated tool set stub.
pub struct ActivatedToolSet;

impl ActivatedToolSet {
    pub fn new() -> Self {
        Self
    }
}

impl Default for ActivatedToolSet {
    fn default() -> Self {
        Self::new()
    }
}

/// MCP tool wrapper stub.
pub struct McpToolWrapper {
    name: String,
}

impl McpToolWrapper {
    pub fn new(name: String, _def: ToolSpec, _registry: Arc<McpRegistry>) -> Self {
        Self { name }
    }
}

#[async_trait::async_trait]
impl Tool for McpToolWrapper {
    fn name(&self) -> &str {
        &self.name
    }
    fn description(&self) -> &str {
        "MCP stub"
    }
    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({"type": "object"})
    }
    async fn execute(
        &self,
        _args: serde_json::Value,
        _ctx: &dyn ToolContext,
    ) -> anyhow::Result<ToolResult> {
        Ok(ToolResult {
            success: false,
            output: String::new(),
            error: Some("MCP stub".into()),
        })
    }
}

/// Tool search tool stub.
pub struct ToolSearchTool;

impl ToolSearchTool {
    pub fn new(
        _deferred: DeferredMcpToolSet,
        _activated: Arc<std::sync::Mutex<ActivatedToolSet>>,
    ) -> Self {
        Self
    }
}

#[async_trait::async_trait]
impl Tool for ToolSearchTool {
    fn name(&self) -> &str {
        "tool_search_stub"
    }
    fn description(&self) -> &str {
        "Tool search stub"
    }
    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({"type": "object"})
    }
    async fn execute(
        &self,
        _args: serde_json::Value,
        _ctx: &dyn ToolContext,
    ) -> anyhow::Result<ToolResult> {
        Ok(ToolResult {
            success: false,
            output: String::new(),
            error: Some("Tool search stub".into()),
        })
    }
}

/// Arc tool reference stub.
pub struct ArcToolRef(pub Arc<dyn DynTool>);

#[async_trait::async_trait]
impl Tool for ArcToolRef {
    fn name(&self) -> &str {
        self.0.name()
    }
    fn description(&self) -> &str {
        self.0.description()
    }
    fn parameters_schema(&self) -> serde_json::Value {
        self.0.parameters_schema()
    }
    async fn execute(
        &self,
        args: serde_json::Value,
        ctx: &dyn ToolContext,
    ) -> anyhow::Result<ToolResult> {
        self.0.execute(args, ctx).await
    }
}

/// CLI tool discovery stub.
pub mod cli_discovery {
    pub fn discover_cli_tools(_extra: &[&str], _exclude: &[&str]) -> Vec<serde_json::Value> {
        Vec::new()
    }
}

/// Claude Code runner stub.
pub mod claude_code_runner {
    use serde::Deserialize;

    #[derive(Debug, Deserialize)]
    pub struct ClaudeCodeHookEvent {
        pub session_id: String,
        pub event_type: String,
        pub tool_name: Option<String>,
        pub summary: Option<String>,
    }
}
