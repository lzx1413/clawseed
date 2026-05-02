use async_trait::async_trait;
use clawseed_api::tool::{Tool, ToolResult};
use clawseed_api::tool_context::ToolContext;
use serde_json::json;

/// Edit a file by replacing an exact string match with new content.
pub struct FileEditTool;

impl FileEditTool {
    pub fn new() -> Self {
        Self
    }
}

impl Default for FileEditTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for FileEditTool {
    fn name(&self) -> &str {
        "file_edit"
    }

    fn description(&self) -> &str {
        "Edit a file by replacing an exact string match with new content"
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path to the file to edit (relative to workspace)"
                },
                "old_string": {
                    "type": "string",
                    "description": "The exact string to find and replace"
                },
                "new_string": {
                    "type": "string",
                    "description": "The replacement string"
                }
            },
            "required": ["path", "old_string", "new_string"]
        })
    }

    async fn execute(
        &self,
        args: serde_json::Value,
        ctx: &dyn ToolContext,
    ) -> anyhow::Result<ToolResult> {
        let path = args
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'path' parameter"))?;

        let old_string = args
            .get("old_string")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'old_string' parameter"))?;

        let new_string = args
            .get("new_string")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'new_string' parameter"))?;

        // Security: reject path traversal
        if path.contains("../") || path.contains("..\\") || path == ".." {
            return Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some("Path traversal ('..') is not allowed.".into()),
            });
        }

        let full_path = ctx.workspace_dir().join(path);

        let content = match tokio::fs::read_to_string(&full_path).await {
            Ok(c) => c,
            Err(e) => {
                return Ok(ToolResult {
                    success: false,
                    output: String::new(),
                    error: Some(format!("Failed to read file: {e}")),
                });
            }
        };

        let count = content.matches(old_string).count();
        if count == 0 {
            return Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some("old_string not found in file".into()),
            });
        }
        if count > 1 {
            return Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some(format!("old_string found {count} times; must be unique")),
            });
        }

        let new_content = content.replacen(old_string, new_string, 1);

        match tokio::fs::write(&full_path, &new_content).await {
            Ok(()) => Ok(ToolResult {
                success: true,
                output: format!("Edited {path}"),
                error: None,
            }),
            Err(e) => Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some(format!("Failed to write file: {e}")),
            }),
        }
    }
}
