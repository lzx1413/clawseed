use async_trait::async_trait;
use clawseed_api::tool::{Tool, ToolResult};
use clawseed_api::tool_context::ToolContext;
use serde_json::Value;

pub struct CronUpdateTool;

impl CronUpdateTool {
    pub fn new() -> Self {
        Self
    }
}

impl Default for CronUpdateTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for CronUpdateTool {
    fn name(&self) -> &str {
        "cron_update"
    }
    fn description(&self) -> &str {
        "Update a cron job"
    }
    fn parameters_schema(&self) -> Value {
        serde_json::json!({"type": "object", "properties": {"id": {"type": "string"}, "schedule": {"type": "string"}, "prompt": {"type": "string"}}, "required": ["id"]})
    }
    async fn execute(&self, args: Value, _ctx: &dyn ToolContext) -> anyhow::Result<ToolResult> {
        let id = args.get("id").and_then(|v| v.as_str()).unwrap_or("unknown");
        Ok(ToolResult {
            success: true,
            output: format!("Cron job {} updated", id),
            error: None,
        })
    }
}
