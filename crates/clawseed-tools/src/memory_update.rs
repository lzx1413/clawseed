use async_trait::async_trait;
use clawseed_api::memory_traits::{Memory, MemoryCategory, MemoryScope};
use clawseed_api::tool::{Tool, ToolResult};
use clawseed_api::tool_context::ToolContext;
use serde_json::json;
use std::sync::Arc;

use crate::memory_scope::private_namespace;
use clawseed_memory::namespaced::PUBLIC_NAMESPACE;

/// Update an existing memory without discarding its lifecycle metadata.
pub struct MemoryUpdateTool {
    memory: Arc<dyn Memory>,
}

impl MemoryUpdateTool {
    pub fn new(memory: Arc<dyn Memory>) -> Self {
        Self { memory }
    }
}

#[async_trait]
impl Tool for MemoryUpdateTool {
    fn name(&self) -> &str {
        "memory_update"
    }

    fn description(&self) -> &str {
        "Update an existing memory by its exact key while preserving its importance, session, and namespace metadata. Call memory_recall first to identify the exact record. Defaults to this identity's private memory; use scope 'public' only when the user explicitly asks to update shared memory. Use memory_store instead when no relevant record exists."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "key": {
                    "type": "string",
                    "description": "Exact key of the existing memory"
                },
                "content": {
                    "type": "string",
                    "description": "Complete replacement content for the memory"
                },
                "category": {
                    "type": "string",
                    "description": "Optional replacement category: core, daily, conversation, or a custom category"
                },
                "scope": {
                    "type": "string",
                    "enum": ["private", "public"],
                    "description": "Memory scope to update. Defaults to private."
                }
            },
            "required": ["key", "content"]
        })
    }

    async fn execute(
        &self,
        args: serde_json::Value,
        _ctx: &dyn ToolContext,
    ) -> anyhow::Result<ToolResult> {
        let key = args
            .get("key")
            .and_then(|value| value.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'key' parameter"))?;
        let content = args
            .get("content")
            .and_then(|value| value.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'content' parameter"))?;
        if content.trim().is_empty() {
            return Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some("Memory content must not be empty".into()),
                presentation: None,
            });
        }

        let scope = args
            .get("scope")
            .and_then(|value| value.as_str())
            .unwrap_or("private");
        let namespace = match scope {
            "private" => private_namespace(self.memory.as_ref()),
            "public" => PUBLIC_NAMESPACE.into(),
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
        let memory_scope = MemoryScope {
            namespace: &namespace,
            session_id: None,
        };
        let Some(existing) = self.memory.get_scoped(memory_scope, key).await? else {
            return Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some(format!(
                    "Memory '{key}' was not found in {scope} scope. Recall memories first, or use memory_store to create it."
                )),
                presentation: None,
            });
        };
        if existing.superseded_by.is_some() {
            return Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some(format!(
                    "Memory '{key}' has been superseded and cannot be updated"
                )),
                presentation: None,
            });
        }

        let category = match args.get("category").and_then(|value| value.as_str()) {
            Some("core") => MemoryCategory::Core,
            Some("daily") => MemoryCategory::Daily,
            Some("conversation") => MemoryCategory::Conversation,
            Some(other) => MemoryCategory::Custom(other.to_string()),
            None => existing.category.clone(),
        };
        match self
            .memory
            .store_with_metadata(
                key,
                content,
                category,
                existing.session_id.as_deref(),
                Some(&namespace),
                existing.importance,
            )
            .await
        {
            Ok(()) => Ok(ToolResult {
                success: true,
                output: format!("Updated {scope} memory: {key}"),
                error: None,
                presentation: None,
            }),
            Err(error) => Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some(format!("Failed to update memory: {error}")),
                presentation: None,
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clawseed_memory::namespaced::NamespacedMemory;
    use clawseed_memory::sqlite::SqliteMemory;
    use tempfile::TempDir;

    fn test_context() -> impl ToolContext {
        struct DummyContext;
        impl ToolContext for DummyContext {
            fn workspace_dir(&self) -> &std::path::Path {
                std::path::Path::new("/tmp")
            }
        }
        DummyContext
    }

    fn test_memory() -> (TempDir, Arc<dyn Memory>) {
        let temp = TempDir::new().unwrap();
        let memory = Arc::new(SqliteMemory::new(temp.path()).unwrap()) as Arc<dyn Memory>;
        (temp, memory)
    }

    #[test]
    fn schema_requires_key_and_content() {
        let (_temp, memory) = test_memory();
        let tool = MemoryUpdateTool::new(memory);
        assert_eq!(tool.name(), "memory_update");
        assert_eq!(
            tool.parameters_schema()["required"],
            json!(["key", "content"])
        );
    }

    #[tokio::test]
    async fn update_preserves_existing_metadata() {
        let (_temp, memory) = test_memory();
        memory
            .store_with_metadata(
                "project_stack",
                "The project uses Rust",
                MemoryCategory::Custom("project".into()),
                Some("session-1"),
                Some("default"),
                Some(0.9),
            )
            .await
            .unwrap();

        let result = MemoryUpdateTool::new(memory.clone())
            .execute(
                json!({"key": "project_stack", "content": "The project uses Kotlin"}),
                &test_context(),
            )
            .await
            .unwrap();

        assert!(result.success, "{:?}", result.error);
        let updated = memory.get("project_stack").await.unwrap().unwrap();
        assert_eq!(updated.content, "The project uses Kotlin");
        assert_eq!(updated.category, MemoryCategory::Custom("project".into()));
        assert_eq!(updated.session_id.as_deref(), Some("session-1"));
        assert_eq!(updated.importance, Some(0.9));
    }

    #[tokio::test]
    async fn update_does_not_create_missing_memory() {
        let (_temp, memory) = test_memory();
        let result = MemoryUpdateTool::new(memory.clone())
            .execute(
                json!({"key": "missing", "content": "new content"}),
                &test_context(),
            )
            .await
            .unwrap();

        assert!(!result.success);
        assert!(memory.get("missing").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn public_update_preserves_private_record_with_same_key() {
        let (_temp, inner) = test_memory();
        inner
            .store_with_metadata(
                "project_stack",
                "Private stack",
                MemoryCategory::Core,
                None,
                Some("persona-a"),
                Some(0.8),
            )
            .await
            .unwrap();
        inner
            .store_with_metadata(
                "project_stack",
                "Shared stack",
                MemoryCategory::Core,
                None,
                Some(PUBLIC_NAMESPACE),
                Some(0.7),
            )
            .await
            .unwrap();
        let memory = Arc::new(NamespacedMemory::new(inner.clone(), "persona-a".into()));

        let result = MemoryUpdateTool::new(memory)
            .execute(
                json!({
                    "key": "project_stack",
                    "content": "Updated shared stack",
                    "scope": "public"
                }),
                &test_context(),
            )
            .await
            .unwrap();

        assert!(result.success, "{:?}", result.error);
        assert_eq!(
            inner
                .get_scoped(
                    MemoryScope {
                        namespace: "persona-a",
                        session_id: None,
                    },
                    "project_stack",
                )
                .await
                .unwrap()
                .unwrap()
                .content,
            "Private stack"
        );
        assert_eq!(
            inner
                .get_scoped(
                    MemoryScope {
                        namespace: PUBLIC_NAMESPACE,
                        session_id: None,
                    },
                    "project_stack",
                )
                .await
                .unwrap()
                .unwrap()
                .content,
            "Updated shared stack"
        );
    }
}
