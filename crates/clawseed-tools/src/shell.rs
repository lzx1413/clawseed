use async_trait::async_trait;
use clawseed_api::tool::{Tool, ToolResult};
use clawseed_api::tool_context::ToolContext;
use serde_json::Value;

#[derive(Default)]
pub struct ShellTool;

impl ShellTool {
    pub fn new() -> Self { Self }
}

#[async_trait]
impl Tool for ShellTool {
    fn name(&self) -> &str { "shell" }
    fn description(&self) -> &str { "Execute a shell command" }
    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "command": { "type": "string", "description": "The command to execute" }
            },
            "required": ["command"]
        })
    }
    async fn execute(&self, args: Value, _ctx: &dyn ToolContext) -> anyhow::Result<ToolResult> {
        let command = args.get("command").and_then(|v| v.as_str()).unwrap_or("");
        #[cfg(unix)]
        let output = {
            use std::process::Command;
            let result = Command::new("sh").arg("-c").arg(command).output()?;
            let stdout = String::from_utf8_lossy(&result.stdout).to_string();
            let stderr = String::from_utf8_lossy(&result.stderr).to_string();
            if result.status.success() {
                ToolResult { success: true, output: stdout, error: None }
            } else {
                ToolResult { success: false, output: stdout, error: Some(stderr) }
            }
        };
        #[cfg(not(unix))]
        let output = ToolResult { success: false, output: String::new(), error: Some("Shell not supported on this platform".to_string()) };
        Ok(output)
    }
}
