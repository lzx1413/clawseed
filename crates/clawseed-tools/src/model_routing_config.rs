//! Model routing config tool (stub).
//!
//! The full config management backend is not yet available in this build.
//! This tool registers itself so the LLM gets a clear error rather than
//! a missing-tool confusion.

use async_trait::async_trait;
use clawseed_api::tool::{Tool, ToolResult};
use clawseed_api::tool_context::ToolContext;
use serde_json::json;

/// Tool for managing model routing config (stub -- backend not yet wired).
pub struct ModelRoutingConfigTool;

impl ModelRoutingConfigTool {
    pub fn new() -> Self {
        Self
    }
}

impl Default for ModelRoutingConfigTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for ModelRoutingConfigTool {
    fn name(&self) -> &str {
        "model_routing_config"
    }

    fn description(&self) -> &str {
        "Manage default model settings, scenario-based provider/model routes, classification rules, and delegate sub-agent profiles"
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": [
                        "get",
                        "list_hints",
                        "set_default",
                        "upsert_scenario",
                        "remove_scenario",
                        "upsert_agent",
                        "remove_agent"
                    ],
                    "default": "get"
                }
            }
        })
    }

    async fn execute(&self, args: serde_json::Value, _ctx: &dyn ToolContext) -> anyhow::Result<ToolResult> {
        let action = args
            .get("action")
            .and_then(|v| v.as_str())
            .unwrap_or("get");
        Ok(ToolResult {
            success: false,
            output: String::new(),
            error: Some(format!(
                "model_routing_config '{action}' is not yet available in this build"
            )),
        })
    }
}
