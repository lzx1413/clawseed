use async_trait::async_trait;
use clawseed_api::memory_traits::{Memory, MemoryScope};
use clawseed_api::tool::{Tool, ToolResult};
use clawseed_api::tool_context::ToolContext;
use clawseed_memory::namespaced::PUBLIC_NAMESPACE;
use serde_json::json;
use std::sync::Arc;

use crate::memory_scope::private_namespace;

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
        "Remove one memory by its exact key. Call memory_recall first and ask the user when the intended record is ambiguous. Defaults to private memory. Use scope 'public' only when the user explicitly asks to remove a shared public memory."
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
                    "enum": ["private", "public", "visible"],
                    "description": "Memory scope to delete from. Defaults to private. 'visible' is a deprecated alias for private."
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
            .unwrap_or("private");
        let (scope_label, namespace) = match scope {
            "private" | "visible" => ("private", private_namespace(self.memory.as_ref())),
            "public" => ("public", PUBLIC_NAMESPACE.into()),
            other => {
                return Ok(ToolResult {
                    success: false,
                    output: String::new(),
                    error: Some(format!(
                        "Invalid scope '{other}'. Expected 'private' or 'public'."
                    )),
                    presentation: None,
                });
            }
        };

        match self
            .memory
            .forget_scoped(
                MemoryScope {
                    namespace: &namespace,
                    session_id: None,
                },
                key,
            )
            .await
        {
            Ok(true) => Ok(ToolResult {
                success: true,
                output: format!("Forgot {scope_label} memory: {key}"),
                error: None,
                presentation: None,
            }),
            Ok(false) => Ok(ToolResult {
                success: true,
                output: format!("No {scope_label} memory found with key: {key}"),
                error: None,
                presentation: None,
            }),
            Err(e) => Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some(format!("Failed to forget memory: {e}")),
                presentation: None,
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clawseed_api::memory_traits::MemoryCategory;
    use clawseed_memory::namespaced::NamespacedMemory;
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
        assert!(result.output.contains("No private memory found"));
    }

    #[tokio::test]
    async fn forget_public_does_not_delete_private_record_with_same_key() {
        let (_tmp, inner) = test_mem();
        for (namespace, content) in [
            ("persona-a", "Private value"),
            (PUBLIC_NAMESPACE, "Public value"),
        ] {
            inner
                .store_with_metadata(
                    "shared_key",
                    content,
                    MemoryCategory::Core,
                    None,
                    Some(namespace),
                    None,
                )
                .await
                .unwrap();
        }
        let memory = Arc::new(NamespacedMemory::new(inner.clone(), "persona-a".into()));

        let result = MemoryForgetTool::new(memory)
            .execute(json!({"key": "shared_key", "scope": "public"}), &test_ctx())
            .await
            .unwrap();

        assert!(result.success, "{:?}", result.error);
        assert!(
            inner
                .get_scoped(
                    MemoryScope {
                        namespace: PUBLIC_NAMESPACE,
                        session_id: None,
                    },
                    "shared_key",
                )
                .await
                .unwrap()
                .is_none()
        );
        assert_eq!(
            inner
                .get_scoped(
                    MemoryScope {
                        namespace: "persona-a",
                        session_id: None,
                    },
                    "shared_key",
                )
                .await
                .unwrap()
                .unwrap()
                .content,
            "Private value"
        );
    }

    #[tokio::test]
    async fn visible_scope_remains_a_private_compatibility_alias() {
        let (_tmp, mem) = test_mem();
        mem.store("temp", "temporary", MemoryCategory::Core, None)
            .await
            .unwrap();

        let result = MemoryForgetTool::new(mem.clone())
            .execute(json!({"key": "temp", "scope": "visible"}), &test_ctx())
            .await
            .unwrap();

        assert!(result.success);
        assert!(result.output.contains("private"));
        assert!(mem.get("temp").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn forget_missing_key() {
        let (_tmp, mem) = test_mem();
        let tool = MemoryForgetTool::new(mem);
        let result = tool.execute(json!({}), &test_ctx()).await;
        assert!(result.is_err());
    }
}
