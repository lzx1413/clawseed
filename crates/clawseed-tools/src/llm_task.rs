//! Lightweight LLM task tool (stub).
//!
//! The full provider factory is not yet available in this build.
//! This tool registers itself so the LLM gets a clear error rather than
//! a missing-tool confusion.

use async_trait::async_trait;
use serde_json::json;
use clawseed_api::tool::{Tool, ToolResult};
use clawseed_api::tool_context::ToolContext;

/// Tool that runs a single prompt through an LLM (stub -- backend not yet wired).
pub struct LlmTaskTool;

impl LlmTaskTool {
    pub fn new() -> Self {
        Self
    }
}

impl Default for LlmTaskTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for LlmTaskTool {
    fn name(&self) -> &str {
        "llm_task"
    }

    fn description(&self) -> &str {
        "Run a prompt through an LLM with no tool access and return the response. \
         Optionally validates the output against a JSON Schema. Ideal for structured \
         data extraction, classification, summarization, and transformation tasks."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "prompt": {
                    "type": "string",
                    "description": "The prompt to send to the LLM."
                },
                "schema": {
                    "type": "object",
                    "description": "Optional JSON Schema to validate the LLM response against."
                },
                "model": {
                    "type": "string",
                    "description": "Optional model override."
                },
                "temperature": {
                    "type": "number",
                    "description": "Optional temperature override (0.0-2.0)."
                }
            },
            "required": ["prompt"]
        })
    }

    async fn execute(&self, _args: serde_json::Value, _ctx: &dyn ToolContext) -> anyhow::Result<ToolResult> {
        Ok(ToolResult {
            success: false,
            output: String::new(),
            error: Some("llm_task provider backend not available".to_string()),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_metadata() {
        let tool = LlmTaskTool::new();
        assert_eq!(tool.name(), "llm_task");
        assert!(tool.description().contains("LLM"));

        let schema = tool.parameters_schema();
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["prompt"].is_object());

        let required = schema["required"].as_array().unwrap();
        assert_eq!(required.len(), 1);
        assert_eq!(required[0], "prompt");
    }
}
