use async_trait::async_trait;
use clawseed_api::tool::{Tool, ToolResult};
use clawseed_api::tool_context::ToolContext;
use serde_json::Value;

const SHELL_TIMEOUT_SECS: u64 = 30;
const MAX_OUTPUT_BYTES: usize = 1_048_576;

#[derive(Default)]
pub struct ShellTool;

impl ShellTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for ShellTool {
    fn name(&self) -> &str {
        "shell"
    }
    fn description(&self) -> &str {
        "Execute a shell command"
    }
    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "command": { "type": "string", "description": "The command to execute" }
            },
            "required": ["command"]
        })
    }
    async fn execute(&self, args: Value, ctx: &dyn ToolContext) -> anyhow::Result<ToolResult> {
        let command = args.get("command").and_then(|v| v.as_str()).unwrap_or("");
        #[cfg(unix)]
        let output = {
            use tokio::process::Command;
            use tokio::time::{Duration, timeout};

            let mut cmd = Command::new("sh");
            cmd.arg("-c")
                .arg(command)
                .current_dir(ctx.workspace_dir())
                .kill_on_drop(true);

            let result = match timeout(Duration::from_secs(SHELL_TIMEOUT_SECS), cmd.output()).await
            {
                Ok(result) => result?,
                Err(_) => {
                    return Ok(ToolResult {
                        success: false,
                        output: String::new(),
                        error: Some(format!(
                            "Command timed out after {SHELL_TIMEOUT_SECS} seconds"
                        )),
                    });
                }
            };
            let stdout = truncate_output(&result.stdout);
            let stderr = truncate_output(&result.stderr);
            if result.status.success() {
                ToolResult {
                    success: true,
                    output: stdout,
                    error: None,
                }
            } else {
                ToolResult {
                    success: false,
                    output: stdout,
                    error: Some(stderr),
                }
            }
        };
        #[cfg(not(unix))]
        let output = ToolResult {
            success: false,
            output: String::new(),
            error: Some("Shell not supported on this platform".to_string()),
        };
        Ok(output)
    }
}

fn truncate_output(bytes: &[u8]) -> String {
    if bytes.len() <= MAX_OUTPUT_BYTES {
        return String::from_utf8_lossy(bytes).to_string();
    }

    let mut output = String::from_utf8_lossy(&bytes[..MAX_OUTPUT_BYTES]).to_string();
    output.push_str("\n[Output truncated]");
    output
}
