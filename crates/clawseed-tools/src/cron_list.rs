use async_trait::async_trait;
use clawseed_api::tool::{Tool, ToolResult};
use clawseed_api::tool_context::ToolContext;
use serde_json::Value;

pub struct CronListTool;

impl CronListTool {
    pub fn new() -> Self {
        Self
    }
}

impl Default for CronListTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for CronListTool {
    fn name(&self) -> &str {
        "cron_list"
    }
    fn description(&self) -> &str {
        "List all cron jobs"
    }
    fn parameters_schema(&self) -> Value {
        serde_json::json!({"type": "object", "properties": {}})
    }
    async fn execute(&self, _args: Value, _ctx: &dyn ToolContext) -> anyhow::Result<ToolResult> {
        Ok(ToolResult {
            success: true,
            output: "No cron jobs found".to_string(),
            error: None,
        })
    }
}
