//! Knowledge management tool (stub).
//!
//! The knowledge graph backend is not yet available in this build.
//! This tool registers itself so the LLM gets a clear error rather than
//! a missing-tool confusion.

use async_trait::async_trait;
use clawseed_api::tool::{Tool, ToolResult};
use clawseed_api::tool_context::ToolContext;
use serde_json::json;

/// Tool for managing a knowledge graph (stub -- backend not yet wired).
pub struct KnowledgeTool;

impl KnowledgeTool {
    pub fn new() -> Self {
        Self
    }
}

impl Default for KnowledgeTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for KnowledgeTool {
    fn name(&self) -> &str {
        "knowledge"
    }

    fn description(&self) -> &str {
        "Manage a knowledge graph of architecture decisions, solution patterns, lessons learned, and experts. Actions: capture, search, relate, suggest, expert_find, lessons_extract, graph_stats."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["capture", "search", "relate", "suggest", "expert_find", "lessons_extract", "graph_stats"],
                    "description": "The action to perform"
                },
                "node_type": {
                    "type": "string",
                    "enum": ["pattern", "decision", "lesson", "expert", "technology"],
                    "description": "Type of knowledge node (for capture)"
                },
                "title": {
                    "type": "string",
                    "description": "Title for the knowledge item (for capture)"
                },
                "content": {
                    "type": "string",
                    "description": "Content body (for capture) or text to extract lessons from (for lessons_extract)"
                },
                "tags": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Tags for filtering and categorization"
                },
                "source_project": {
                    "type": "string",
                    "description": "Source project identifier (for capture)"
                },
                "query": {
                    "type": "string",
                    "description": "Search query text (for search, suggest)"
                },
                "from_id": {
                    "type": "string",
                    "description": "Source node ID (for relate)"
                },
                "to_id": {
                    "type": "string",
                    "description": "Target node ID (for relate)"
                },
                "relation": {
                    "type": "string",
                    "enum": ["uses", "replaces", "extends", "authored_by", "applies_to"],
                    "description": "Relationship type (for relate)"
                },
                "filters": {
                    "type": "object",
                    "properties": {
                        "node_type": { "type": "string" },
                        "tags": { "type": "array", "items": { "type": "string" } },
                        "project": { "type": "string" }
                    },
                    "description": "Optional search filters"
                }
            },
            "required": ["action"]
        })
    }

    async fn execute(
        &self,
        _args: serde_json::Value,
        _ctx: &dyn ToolContext,
    ) -> anyhow::Result<ToolResult> {
        Ok(ToolResult {
            success: false,
            output: String::new(),
            error: Some("knowledge graph backend not available".to_string()),
        })
    }
}

#[test]
fn name_and_schema_are_valid() {
    let tool = KnowledgeTool::new();
    assert_eq!(tool.name(), "knowledge");
    let schema = tool.parameters_schema();
    assert!(schema["properties"]["action"].is_object());
}
