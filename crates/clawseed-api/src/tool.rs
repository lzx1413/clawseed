//! Tool trait and related types.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::tool_context::ToolContext;

/// Result of a tool execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    pub success: bool,
    pub output: String,
    pub error: Option<String>,
}

/// Description of a tool for the LLM.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

/// Core tool trait — implement for any capability.
///
/// Tools only depend on `clawseed-api` traits. Runtime dependencies
/// (memory, etc.) are received via constructor injection.
#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn parameters_schema(&self) -> serde_json::Value;

    /// Execute the tool with given arguments and context.
    async fn execute(
        &self,
        args: serde_json::Value,
        ctx: &dyn ToolContext,
    ) -> anyhow::Result<ToolResult>;

    /// Get the full spec for LLM registration.
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: self.name().to_string(),
            description: self.description().to_string(),
            parameters: self.parameters_schema(),
        }
    }
}
