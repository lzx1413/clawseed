//! The `Skill` built-in tool — loads skill instructions on demand.
//!
//! This tool returns a human-readable pending message. The agent loop
//! intercepts Skill tool calls by inspecting the original call arguments
//! and performs the actual activation/deactivation (which requires `&mut self`).

use async_trait::async_trait;
use clawseed_api::tool::{Tool, ToolResult};
use clawseed_api::tool_context::ToolContext;
use serde_json::Value;

pub struct SkillTool;

impl Default for SkillTool {
    fn default() -> Self {
        Self::new()
    }
}

impl SkillTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for SkillTool {
    fn name(&self) -> &str {
        "Skill"
    }

    fn description(&self) -> &str {
        "Load a skill's full instructions by name. \
         Use when a user request matches a skill from <available_skills>. \
         Once loaded, the skill's instructions are added to your system prompt \
         and persist across turns. Use action \"deactivate\" to remove a skill."
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "skill": {
                    "type": "string",
                    "description": "Exact skill name from <available_skills>"
                },
                "action": {
                    "type": "string",
                    "enum": ["activate", "deactivate"],
                    "description": "Whether to activate or deactivate the skill. Defaults to 'activate'."
                }
            },
            "required": ["skill"]
        })
    }

    async fn execute(&self, args: Value, _ctx: &dyn ToolContext) -> anyhow::Result<ToolResult> {
        let skill_name = args.get("skill").and_then(|v| v.as_str()).unwrap_or("");

        if skill_name.is_empty() {
            return Ok(ToolResult {
                success: false,
                output: "Skill name is required.".into(),
                error: Some("missing skill name".into()),
            });
        }

        let action = args
            .get("action")
            .and_then(|v| v.as_str())
            .unwrap_or("activate");

        match action {
            "activate" => Ok(ToolResult {
                success: true,
                output: format!("Activating skill '{skill_name}'..."),
                error: None,
            }),
            "deactivate" => Ok(ToolResult {
                success: true,
                output: format!("Deactivating skill '{skill_name}'..."),
                error: None,
            }),
            _ => Ok(ToolResult {
                success: false,
                output: format!("Unknown action '{action}'. Use 'activate' or 'deactivate'."),
                error: Some(format!("invalid action: {action}")),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clawseed_api::tool_context::ToolContext;

    struct MockContext;

    impl ToolContext for MockContext {
        fn workspace_dir(&self) -> &std::path::Path {
            std::path::Path::new("/tmp")
        }
    }

    #[tokio::test]
    async fn skill_tool_activate() {
        let tool = SkillTool::new();
        let args = serde_json::json!({"skill": "auto-coder"});
        let ctx = MockContext;
        let result = tool.execute(args, &ctx).await.unwrap();
        assert!(result.success);
        assert!(result.output.contains("auto-coder"));
        assert!(result.output.contains("Activating"));
    }

    #[tokio::test]
    async fn skill_tool_deactivate() {
        let tool = SkillTool::new();
        let args = serde_json::json!({"skill": "auto-coder", "action": "deactivate"});
        let ctx = MockContext;
        let result = tool.execute(args, &ctx).await.unwrap();
        assert!(result.success);
        assert!(result.output.contains("auto-coder"));
        assert!(result.output.contains("Deactivating"));
    }

    #[tokio::test]
    async fn skill_tool_invalid_action() {
        let tool = SkillTool::new();
        let args = serde_json::json!({"skill": "auto-coder", "action": "destroy"});
        let ctx = MockContext;
        let result = tool.execute(args, &ctx).await.unwrap();
        assert!(!result.success);
        assert!(result.output.contains("Unknown action"));
    }

    #[tokio::test]
    async fn skill_tool_missing_name() {
        let tool = SkillTool::new();
        let args = serde_json::json!({});
        let ctx = MockContext;
        let result = tool.execute(args, &ctx).await.unwrap();
        assert!(!result.success);
    }
}
