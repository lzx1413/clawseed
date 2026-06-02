use async_trait::async_trait;
use clawseed_api::memory_traits::{Memory, MemoryCategory};
use clawseed_api::tool::{Tool, ToolResult};
use clawseed_api::tool_context::ToolContext;
use clawseed_memory::namespaced::NamespacedMemory;
use serde_json::json;
use std::sync::Arc;

/// Reviewer-specific memory store tool.
///
/// Stores evaluations in the council namespace with a `review_{role}_` key prefix.
/// Holds `Arc<NamespacedMemory>` (not `Arc<dyn Memory>`) as a compile-time guarantee
/// that bare shared memory cannot bypass namespace isolation.
pub struct ReviewerMemoryStoreTool {
    role: String,
    council_memory: Arc<NamespacedMemory>,
}

impl ReviewerMemoryStoreTool {
    pub fn new(role: String, council_memory: Arc<NamespacedMemory>) -> Self {
        Self {
            role,
            council_memory,
        }
    }
}

#[async_trait]
impl Tool for ReviewerMemoryStoreTool {
    fn name(&self) -> &str {
        "reviewer_memory_store"
    }

    fn description(&self) -> &str {
        "Store an evaluation or feedback in council shared memory. The key is auto-prefixed with 'review_{role}_'."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "key": {
                    "type": "string",
                    "description": "Short key for this feedback (auto-prefixed with review_{role}_)"
                },
                "content": {
                    "type": "string",
                    "description": "A complete, self-contained evaluation or feedback text"
                },
                "category": {
                    "type": "string",
                    "description": "Memory category: defaults to 'Review'"
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
        let key_suffix = args
            .get("key")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'key' parameter"))?;

        let content = args
            .get("content")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'content' parameter"))?;

        let key = format!("review_{}_{}", self.role, key_suffix);

        let category = match args.get("category").and_then(|v| v.as_str()) {
            Some("core") => MemoryCategory::Core,
            Some("daily") => MemoryCategory::Daily,
            Some("conversation") => MemoryCategory::Conversation,
            None | Some("Review") | Some("review") => MemoryCategory::Custom("Review".to_string()),
            Some(other) => MemoryCategory::Custom(other.to_string()),
        };

        match self
            .council_memory
            .store(&key, content, category, None)
            .await
        {
            Ok(()) => Ok(ToolResult {
                success: true,
                output: format!("Stored council feedback: {key}"),
                error: None,
            }),
            Err(e) => Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some(format!("Failed to store council feedback: {e}")),
            }),
        }
    }
}
