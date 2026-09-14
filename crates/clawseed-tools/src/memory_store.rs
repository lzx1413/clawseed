use async_trait::async_trait;
use clawseed_api::memory_traits::{Memory, MemoryCategory, MemoryScope};
use clawseed_api::tool::{Tool, ToolResult};
use clawseed_api::tool_context::ToolContext;
use clawseed_memory::namespaced::PUBLIC_NAMESPACE;
use serde_json::json;
use std::sync::Arc;

use crate::memory_scope::private_namespace;

/// Let the agent store memories -- its own brain writes
pub struct MemoryStoreTool {
    memory: Arc<dyn Memory>,
}

impl MemoryStoreTool {
    pub fn new(memory: Arc<dyn Memory>) -> Self {
        Self { memory }
    }
}

#[async_trait]
impl Tool for MemoryStoreTool {
    fn name(&self) -> &str {
        "memory_store"
    }

    fn description(&self) -> &str {
        "Create a new event, project fact, decision, or task result with cross-session value. Call memory_recall first when a related record may already exist; use memory_update with its exact key instead of creating a duplicate. Do not use this tool for stable user identity, preferences, goals, constraints, or accessibility needs; those belong in user-profile management. By default stores in this identity's private memory. Use scope 'public' only when the user explicitly asks all identities/personas to share it. The content must be a complete, self-contained sentence with enough context for later retrieval."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "key": {
                    "type": "string",
                    "description": "Unique key for this memory (e.g. 'user_lang', 'project_stack')"
                },
                "content": {
                    "type": "string",
                    "description": "A complete, self-contained event, project fact, decision, or task result. Include relevant context so it can be found later."
                },
                "category": {
                    "type": "string",
                    "description": "Memory category: 'core' (permanent), 'daily' (session), 'conversation' (chat), or a custom category name. Defaults to 'core'."
                },
                "scope": {
                    "type": "string",
                    "enum": ["private", "public"],
                    "description": "Where to store this memory. Defaults to 'private'. Use 'public' only when the user explicitly says everyone/all identities/personas should remember or share it."
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
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'key' parameter"))?;

        let content = args
            .get("content")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'content' parameter"))?;
        if content.trim().is_empty() {
            return Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some("Memory content must not be empty".into()),
                presentation: None,
            });
        }

        let category = match args.get("category").and_then(|v| v.as_str()) {
            Some("core") | None => MemoryCategory::Core,
            Some("daily") => MemoryCategory::Daily,
            Some("conversation") => MemoryCategory::Conversation,
            Some(other) => MemoryCategory::Custom(other.to_string()),
        };

        let scope = args
            .get("scope")
            .and_then(|value| value.as_str())
            .unwrap_or("private");
        let namespace = match scope {
            "public" => PUBLIC_NAMESPACE.into(),
            "private" => private_namespace(self.memory.as_ref()),
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

        if self
            .memory
            .get_scoped(
                MemoryScope {
                    namespace: &namespace,
                    session_id: None,
                },
                key,
            )
            .await?
            .is_some()
        {
            return Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some(format!(
                    "Memory '{key}' already exists in {scope} scope. Use memory_update to change it."
                )),
                presentation: None,
            });
        }

        match self
            .memory
            .store_with_metadata(key, content, category, None, Some(&namespace), None)
            .await
        {
            Ok(()) => Ok(ToolResult {
                success: true,
                output: format!("Stored {scope} memory: {key}"),
                error: None,
                presentation: None,
            }),
            Err(e) => Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some(format!("Failed to store memory: {e}")),
                presentation: None,
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
        let tool = MemoryStoreTool::new(mem);
        assert_eq!(tool.name(), "memory_store");
        let schema = tool.parameters_schema();
        assert!(schema["properties"]["key"].is_object());
        assert!(schema["properties"]["content"].is_object());
    }

    #[tokio::test]
    async fn store_core() {
        let (_tmp, mem) = test_mem();
        let tool = MemoryStoreTool::new(mem.clone());
        let result = tool
            .execute(
                json!({"key": "lang", "content": "Prefers Rust"}),
                &test_ctx(),
            )
            .await
            .unwrap();
        assert!(result.success);
        assert!(result.output.contains("lang"));

        let entry = mem.get("lang").await.unwrap();
        assert!(entry.is_some());
        assert_eq!(entry.unwrap().content, "Prefers Rust");
    }

    #[tokio::test]
    async fn store_with_category() {
        let (_tmp, mem) = test_mem();
        let tool = MemoryStoreTool::new(mem.clone());
        let result = tool
            .execute(
                json!({"key": "note", "content": "Fixed bug", "category": "daily"}),
                &test_ctx(),
            )
            .await
            .unwrap();
        assert!(result.success);
    }

    #[tokio::test]
    async fn store_with_custom_category() {
        let (_tmp, mem) = test_mem();
        let tool = MemoryStoreTool::new(mem.clone());
        let result = tool
            .execute(
                json!({"key": "proj_note", "content": "Uses async runtime", "category": "project"}),
                &test_ctx(),
            )
            .await
            .unwrap();
        assert!(result.success);

        let entry = mem.get("proj_note").await.unwrap().unwrap();
        assert_eq!(entry.content, "Uses async runtime");
        assert_eq!(entry.category, MemoryCategory::Custom("project".into()));
    }

    #[tokio::test]
    async fn store_public_scope_uses_public_namespace() {
        let (_tmp, mem) = test_mem();
        let tool = MemoryStoreTool::new(mem.clone());
        let result = tool
            .execute(
                json!({
                    "key": "shared_lang",
                    "content": "All identities should know the project uses Rust",
                    "scope": "public"
                }),
                &test_ctx(),
            )
            .await
            .unwrap();
        assert!(result.success);
        assert!(result.output.contains("public"));

        let hits = mem
            .recall_namespaced("public", "Rust", 10, None, None, None, None)
            .await
            .unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].key, "shared_lang");
    }

    #[tokio::test]
    async fn store_rejects_an_existing_key_without_overwriting_it() {
        let (_tmp, mem) = test_mem();
        mem.store_with_metadata(
            "project_stack",
            "The project uses Rust",
            MemoryCategory::Custom("project".into()),
            Some("session-1"),
            Some("default"),
            Some(0.9),
        )
        .await
        .unwrap();

        let result = MemoryStoreTool::new(mem.clone())
            .execute(
                json!({"key": "project_stack", "content": "The project uses Kotlin"}),
                &test_ctx(),
            )
            .await
            .unwrap();

        assert!(!result.success);
        assert!(result.error.unwrap().contains("memory_update"));
        let existing = mem.get("project_stack").await.unwrap().unwrap();
        assert_eq!(existing.content, "The project uses Rust");
        assert_eq!(existing.importance, Some(0.9));
    }

    #[tokio::test]
    async fn store_missing_key() {
        let (_tmp, mem) = test_mem();
        let tool = MemoryStoreTool::new(mem);
        let result = tool
            .execute(json!({"content": "no key"}), &test_ctx())
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn store_missing_content() {
        let (_tmp, mem) = test_mem();
        let tool = MemoryStoreTool::new(mem);
        let result = tool
            .execute(json!({"key": "no_content"}), &test_ctx())
            .await;
        assert!(result.is_err());
    }
}
