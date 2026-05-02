use async_trait::async_trait;
use clawseed_api::tool::{Tool, ToolResult};
use clawseed_api::tool_context::ToolContext;
use serde_json::Value;

pub struct CronRunsTool;

impl CronRunsTool {
    pub fn new() -> Self {
        Self
    }
}

impl Default for CronRunsTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for CronRunsTool {
    fn name(&self) -> &str {
        "cron_runs"
    }
    fn description(&self) -> &str {
        "List execution history for a cron job"
    }
    fn parameters_schema(&self) -> Value {
        serde_json::json!({"type": "object", "properties": {"id": {"type": "string"}}, "required": ["id"]})
    }
    async fn execute(&self, args: Value, _ctx: &dyn ToolContext) -> anyhow::Result<ToolResult> {
        let id = args.get("id").and_then(|v| v.as_str()).unwrap_or("unknown");
        Ok(ToolResult {
            success: true,
            output: format!("No runs found for cron job {}", id),
            error: None,
        })
    }
}
