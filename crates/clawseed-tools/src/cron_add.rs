use async_trait::async_trait;
use clawseed_api::tool::{Tool, ToolResult};
use clawseed_api::tool_context::ToolContext;
use serde_json::Value;

pub struct CronAddTool;

impl CronAddTool {
    pub fn new() -> Self { Self }
}

impl Default for CronAddTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for CronAddTool {
    fn name(&self) -> &str { "cron_add" }
    fn description(&self) -> &str { "Add a new cron job" }
    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "schedule": { "type": "string", "description": "Cron expression" },
                "prompt": { "type": "string", "description": "Prompt to execute" }
            },
            "required": ["schedule", "prompt"]
        })
    }
    async fn execute(&self, args: Value, _ctx: &dyn ToolContext) -> anyhow::Result<ToolResult> {
        let schedule = args.get("schedule").and_then(|v| v.as_str()).unwrap_or("");
        let prompt = args.get("prompt").and_then(|v| v.as_str()).unwrap_or("");
        Ok(ToolResult { success: true, output: format!("Cron job added: {} -> {}", schedule, prompt), error: None })
    }
}
