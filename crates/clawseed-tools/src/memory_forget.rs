use async_trait::async_trait;
use clawseed_api::memory_traits::Memory;
use clawseed_api::tool::{Tool, ToolResult};
use clawseed_api::tool_context::ToolContext;
use clawseed_memory::namespaced::PUBLIC_NAMESPACE;
use serde_json::json;
use std::sync::Arc;

/// Let the agent forget/delete a memory entry
pub struct MemoryForgetTool {
    memory: Arc<dyn Memory>,
}

impl MemoryForgetTool {
    pub fn new(memory: Arc<dyn Memory>) -> Self {
        Self { memory }
    }
}

#[async_trait]
impl Tool for MemoryForgetTool {
    fn name(&self) -> &str {
        "memory_forget"
    }

    fn description(&self) -> &str {
        "Remove a memory by key. Defaults to visible/private memory. Use scope 'public' only when the user explicitly asks to remove a shared public memory. Returns whether the memory was found and removed."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "key": {
                    "type": "string",
                    "description": "The key of the memory to forget"
                },
                "scope": {
                    "type": "string",
                    "enum": ["visible", "public"],
                    "description": "Memory scope to delete from. Defaults to 'visible'. Use 'public' to delete a shared public memory."
                }
            },
            "required": ["key"]
        })
    }

    async fn execute(
        &self,
        args: serde_json::Value,
        _ctx: &dyn ToolContext,
    ) -> anyhow::Result<ToolResult> {
        let key = args
            .get("key")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'key' parameter"))?;

        let scope = args
            .get("scope")
            .and_then(|v| v.as_str())
            .unwrap_or("visible");
        if scope == "public" {
            let visible = self.memory.list(None, None).await?;
            let found_public = visible
                .iter()
                .any(|entry| entry.key == key && entry.namespace == PUBLIC_NAMESPACE);
            if !found_public {
                return Ok(ToolResult {
                    success: true,
                    output: format!("No public memory found with key: {key}"),
                    error: None,
                });
            }
        } else if scope != "visible" {
            return Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some(format!(
                    "Invalid scope '{scope}'. Expected 'visible' or 'public'."
                )),
            });
        }

        match self.memory.forget(key).await {
            Ok(true) => Ok(ToolResult {
                success: true,
                output: if scope == "public" {
                    format!("Forgot public memory: {key}")
                } else {
                    format!("Forgot memory: {key}")
                },
                error: None,
            }),
            Ok(false) => Ok(ToolResult {
                success: true,
                output: format!("No memory found with key: {key}"),
                error: None,
            }),
            Err(e) => Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some(format!("Failed to forget memory: {e}")),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clawseed_api::memory_traits::MemoryCategory;
    use clawseed_memory::sqlite::SqliteMemory;
    use tempfile::TempDir;

    fn test_mem() -> (TempDir, Arc<dyn Memory>) {
        let tmp = TempDir::new().unwrap();
        let mem = SqliteMemory::new(tmp.path()).unwrap();
        (tmp, Arc::new(mem))
    }

    fn test_ctx() -> impl ToolContext {
        struct DummyCtx;
        impl ToolContext for DummyCtx {
            fn workspace_dir(&self) -> &std::path::Path {
                std::path::Path::new("/tmp")
            }
        }
        DummyCtx
    }

    #[test]
    fn name_and_schema() {
        let (_tmp, mem) = test_mem();
        let tool = MemoryForgetTool::new(mem);
        assert_eq!(tool.name(), "memory_forget");
        assert!(tool.parameters_schema()["properties"]["key"].is_object());
    }

    #[tokio::test]
    async fn forget_existing() {
        let (_tmp, mem) = test_mem();
        mem.store("temp", "temporary", MemoryCategory::Conversation, None)
            .await
            .unwrap();

        let tool = MemoryForgetTool::new(mem.clone());
        let result = tool
            .execute(json!({"key": "temp"}), &test_ctx())
            .await
            .unwrap();
        assert!(result.success);
        assert!(result.output.contains("Forgot"));

        assert!(mem.get("temp").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn forget_nonexistent() {
        let (_tmp, mem) = test_mem();
        let tool = MemoryForgetTool::new(mem);
        let result = tool
            .execute(json!({"key": "nope"}), &test_ctx())
            .await
            .unwrap();
        assert!(result.success);
        assert!(result.output.contains("No memory found"));
    }

    #[tokio::test]
    async fn forget_missing_key() {
        let (_tmp, mem) = test_mem();
        let tool = MemoryForgetTool::new(mem);
        let result = tool.execute(json!({}), &test_ctx()).await;
        assert!(result.is_err());
    }
}
