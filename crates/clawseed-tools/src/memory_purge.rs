use async_trait::async_trait;
use clawseed_api::memory_traits::Memory;
use clawseed_api::tool::{Tool, ToolResult};
use clawseed_api::tool_context::ToolContext;
use serde_json::json;
use std::sync::Arc;

/// Let the agent bulk-delete memories by namespace or session
pub struct MemoryPurgeTool {
    memory: Arc<dyn Memory>,
}

impl MemoryPurgeTool {
    pub fn new(memory: Arc<dyn Memory>) -> Self {
        Self { memory }
    }
}

#[async_trait]
impl Tool for MemoryPurgeTool {
    fn name(&self) -> &str {
        "memory_purge"
    }

    fn description(&self) -> &str {
        "Remove all memories in a namespace (category) or session. Use to bulk-delete conversation context or category-scoped data. Returns the number of deleted entries. WARNING: This operation cannot be undone."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "namespace": {
                    "type": "string",
                    "description": "The namespace (category) to purge. Deletes all memories in this category."
                },
                "session_id": {
                    "type": "string",
                    "description": "The session ID to purge. Deletes all memories in this session."
                }
            },
            "minProperties": 1
        })
    }

    async fn execute(
        &self,
        args: serde_json::Value,
        _ctx: &dyn ToolContext,
    ) -> anyhow::Result<ToolResult> {
        let namespace = args.get("namespace").and_then(|v| v.as_str());
        let session_id = args.get("session_id").and_then(|v| v.as_str());

        if namespace.is_none() && session_id.is_none() {
            return Err(anyhow::anyhow!(
                "Must provide either 'namespace' or 'session_id' parameter"
            ));
        }

        let mut total_purged = 0;
        let mut output_parts = Vec::new();

        if let Some(ns) = namespace {
            match self.memory.purge_namespace(ns).await {
                Ok(count) => {
                    total_purged += count;
                    output_parts.push(format!("Purged {count} memories from namespace '{ns}'"));
                }
                Err(e) => {
                    return Ok(ToolResult {
                        success: false,
                        output: String::new(),
                        error: Some(format!("Failed to purge namespace: {e}")),
                    });
                }
            }
        }

        if let Some(sid) = session_id {
            match self.memory.purge_session(sid).await {
                Ok(count) => {
                    total_purged += count;
                    output_parts.push(format!("Purged {count} memories from session '{sid}'"));
                }
                Err(e) => {
                    return Ok(ToolResult {
                        success: false,
                        output: String::new(),
                        error: Some(format!("Failed to purge session: {e}")),
                    });
                }
            }
        }

        Ok(ToolResult {
            success: true,
            output: if output_parts.is_empty() {
                format!("Purged {total_purged} memories")
            } else {
                output_parts.join("; ")
            },
            error: None,
        })
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
            fn get_any(
                &self,
                _type_id: std::any::TypeId,
            ) -> Option<&(dyn std::any::Any + Send + Sync)> {
                None
            }
        }
        DummyCtx
    }

    #[test]
    fn name_and_schema() {
        let (_tmp, mem) = test_mem();
        let tool = MemoryPurgeTool::new(mem);
        assert_eq!(tool.name(), "memory_purge");
        assert!(tool.parameters_schema()["properties"]["namespace"].is_object());
        assert!(tool.parameters_schema()["properties"]["session_id"].is_object());
    }

    #[tokio::test]
    async fn purge_namespace_removes_all_memories() {
        let (_tmp, mem) = test_mem();
        mem.store_with_metadata(
            "a1",
            "data1",
            MemoryCategory::Core,
            None,
            Some("test_ns"),
            None,
        )
        .await
        .unwrap();
        mem.store_with_metadata(
            "a2",
            "data2",
            MemoryCategory::Core,
            None,
            Some("test_ns"),
            None,
        )
        .await
        .unwrap();
        mem.store("b1", "data3", MemoryCategory::Core, None)
            .await
            .unwrap();

        let tool = MemoryPurgeTool::new(mem.clone());
        let result = tool
            .execute(json!({"namespace": "test_ns"}), &test_ctx())
            .await
            .unwrap();
        assert!(result.success);
        assert!(result.output.contains("2 memories"));

        assert_eq!(mem.count().await.unwrap(), 1);
    }

    #[tokio::test]
    async fn purge_session_removes_all_memories() {
        let (_tmp, mem) = test_mem();
        mem.store("a1", "data1", MemoryCategory::Core, Some("sess-x"))
            .await
            .unwrap();
        mem.store("a2", "data2", MemoryCategory::Core, Some("sess-x"))
            .await
            .unwrap();
        mem.store("b1", "data3", MemoryCategory::Core, Some("sess-y"))
            .await
            .unwrap();

        let tool = MemoryPurgeTool::new(mem.clone());
        let result = tool
            .execute(json!({"session_id": "sess-x"}), &test_ctx())
            .await
            .unwrap();
        assert!(result.success);
        assert!(result.output.contains("2 memories"));

        assert_eq!(mem.count().await.unwrap(), 1);
    }

    #[tokio::test]
    async fn purge_namespace_nonexistent_is_noop() {
        let (_tmp, mem) = test_mem();
        mem.store("a", "data", MemoryCategory::Core, None)
            .await
            .unwrap();

        let tool = MemoryPurgeTool::new(mem.clone());
        let result = tool
            .execute(json!({"namespace": "nonexistent"}), &test_ctx())
            .await
            .unwrap();
        assert!(result.success);
        assert!(result.output.contains("0 memories"));

        assert_eq!(mem.count().await.unwrap(), 1);
    }

    #[tokio::test]
    async fn purge_session_nonexistent_is_noop() {
        let (_tmp, mem) = test_mem();
        mem.store("a", "data", MemoryCategory::Core, Some("sess"))
            .await
            .unwrap();

        let tool = MemoryPurgeTool::new(mem.clone());
        let result = tool
            .execute(json!({"session_id": "nonexistent"}), &test_ctx())
            .await
            .unwrap();
        assert!(result.success);
        assert!(result.output.contains("0 memories"));

        assert_eq!(mem.count().await.unwrap(), 1);
    }

    #[tokio::test]
    async fn purge_missing_parameter() {
        let (_tmp, mem) = test_mem();
        let tool = MemoryPurgeTool::new(mem);
        let result = tool.execute(json!({}), &test_ctx()).await;
        assert!(result.is_err());
    }
}
