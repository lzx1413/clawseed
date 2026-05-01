use async_trait::async_trait;
use clawseed_api::tool::{Tool, ToolResult};
use clawseed_api::tool_context::ToolContext;
use serde_json::Value;

const MAX_FILE_SIZE_BYTES: u64 = 10 * 1024 * 1024;

pub struct FileReadTool;

impl FileReadTool {
    pub fn new() -> Self { Self }
}

impl Default for FileReadTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for FileReadTool {
    fn name(&self) -> &str { "file_read" }
    fn description(&self) -> &str { "Read the contents of a file" }
    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "Path to the file" },
                "offset": { "type": "integer", "description": "Line offset to start from" },
                "limit": { "type": "integer", "description": "Maximum number of lines to read" }
            },
            "required": ["path"]
        })
    }
    async fn execute(&self, args: Value, ctx: &dyn ToolContext) -> anyhow::Result<ToolResult> {
        let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
        let workspace = ctx.workspace_dir();
        let full_path = workspace.join(path);

        // Sandbox: path must be within workspace
        let canonical = std::fs::canonicalize(&full_path)
            .map_err(|e| anyhow::anyhow!("Cannot read path {}: {}", path, e))?;
        if !canonical.starts_with(workspace) {
            return Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some(format!("Path {} is outside workspace", path)),
            });
        }

        let metadata = std::fs::metadata(&canonical)?;
        if metadata.len() > MAX_FILE_SIZE_BYTES {
            return Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some(format!("File too large: {} bytes (max {})", metadata.len(), MAX_FILE_SIZE_BYTES)),
            });
        }

        let content = std::fs::read_to_string(&canonical)?;
        Ok(ToolResult { success: true, output: content, error: None })
    }
}
