//! The `skill_create` built-in tool — creates a new skill from conversation.
//!
//! Writes manifest.toml and SKILL.md to the workspace skills directory.
//! The agent loop intercepts skill_create results and refreshes the skill
//! index + optionally activates the new skill (see agent.rs).

use async_trait::async_trait;
use clawseed_api::tool::{Tool, ToolResult};
use clawseed_api::tool_context::ToolContext;
use serde_json::Value;

pub struct SkillCreateTool;

impl Default for SkillCreateTool {
    fn default() -> Self {
        Self::new()
    }
}

impl SkillCreateTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for SkillCreateTool {
    fn name(&self) -> &str {
        "skill_create"
    }

    fn description(&self) -> &str {
        "Create a new skill. Writes manifest.toml and SKILL.md to the workspace \
         skills directory, then the skill becomes available for activation. \
         Use when the user asks to create a skill, or when you identify a reusable \
         pattern worth saving as a skill."
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "description": "Skill name (lowercase, hyphens allowed, no spaces or slashes)"
                },
                "description": {
                    "type": "string",
                    "description": "One-line description of what the skill does"
                },
                "content": {
                    "type": "string",
                    "description": "Full SKILL.md content — the instructions the agent follows when this skill is active"
                },
                "triggers": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Phrases that should trigger this skill activation"
                },
                "permissions": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Tools this skill needs (e.g. calculator, file_read, web_search)"
                },
                "activate": {
                    "type": "boolean",
                    "description": "Whether to activate the skill immediately after creation. Defaults to true."
                }
            },
            "required": ["name", "description", "content"]
        })
    }

    async fn execute(&self, args: Value, ctx: &dyn ToolContext) -> anyhow::Result<ToolResult> {
        let name = args.get("name").and_then(|v| v.as_str()).unwrap_or("");
        let description = args
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let content = args.get("content").and_then(|v| v.as_str()).unwrap_or("");

        if name.is_empty() {
            return Ok(ToolResult {
                success: false,
                output: "Skill name is required.".into(),
                error: Some("missing name".into()),
            });
        }

        // Validate name: no path traversal, no slashes, no spaces
        if name.contains('/') || name.contains('\\') || name.contains("..") || name.contains(' ') {
            return Ok(ToolResult {
                success: false,
                output: format!(
                    "Invalid skill name '{}'. Use lowercase letters, digits, and hyphens only.",
                    name
                ),
                error: Some("invalid name".into()),
            });
        }

        if description.is_empty() {
            return Ok(ToolResult {
                success: false,
                output: "Skill description is required.".into(),
                error: Some("missing description".into()),
            });
        }

        if content.is_empty() {
            return Ok(ToolResult {
                success: false,
                output: "Skill content (SKILL.md body) is required.".into(),
                error: Some("missing content".into()),
            });
        }

        let triggers: Vec<String> = args
            .get("triggers")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();

        let permissions: Vec<String> = args
            .get("permissions")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();

        let activate = args
            .get("activate")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        let skill_dir = ctx
            .workspace_dir()
            .join(".clawseed")
            .join("skills")
            .join(name);

        // Create skill directory
        tokio::fs::create_dir_all(&skill_dir).await?;

        // Write manifest.toml
        let manifest = format!(
            "[skill]\nname = \"{}\"\nversion = \"0.1.0\"\ndescription = \"{}\"\npermissions = [{}]\ntriggers = [{}]\n",
            name,
            description.replace('"', "\\\""),
            permissions
                .iter()
                .map(|p| format!("\"{}\"", p.replace('"', "\\\"")))
                .collect::<Vec<_>>()
                .join(", "),
            triggers
                .iter()
                .map(|t| format!("\"{}\"", t.replace('"', "\\\"")))
                .collect::<Vec<_>>()
                .join(", "),
        );
        let manifest_path = skill_dir.join("manifest.toml");
        tokio::fs::write(&manifest_path, manifest).await?;

        // Write SKILL.md
        let skill_md_path = skill_dir.join("SKILL.md");
        tokio::fs::write(&skill_md_path, content).await?;

        let mut output = format!(
            "Created skill '{}' at {}.\nManifest: {}\nSKILL.md: {} bytes",
            name,
            skill_dir.display(),
            manifest_path.display(),
            content.len(),
        );
        if activate {
            output.push_str("\n\nActivating skill after refresh...");
        }

        Ok(ToolResult {
            success: true,
            output,
            error: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clawseed_api::tool_context::ToolContext;
    use std::path::Path;

    struct TestContext {
        workspace: tempfile::TempDir,
    }

    impl ToolContext for TestContext {
        fn workspace_dir(&self) -> &Path {
            self.workspace.path()
        }
    }

    #[tokio::test]
    async fn create_skill_basic() {
        let tool = SkillCreateTool::new();
        let ctx = TestContext {
            workspace: tempfile::TempDir::new().unwrap(),
        };
        let args = serde_json::json!({
            "name": "test-skill",
            "description": "A test skill",
            "content": "# Test\n\nDo the thing.",
            "triggers": ["测试"],
            "permissions": ["calculator"],
            "activate": false
        });

        let result = tool.execute(args, &ctx).await.unwrap();
        assert!(result.success);
        assert!(result.output.contains("test-skill"));

        // Verify files exist
        let skill_dir = ctx.workspace.path().join(".clawseed/skills/test-skill");
        assert!(skill_dir.join("manifest.toml").exists());
        assert!(skill_dir.join("SKILL.md").exists());

        // Verify manifest content
        let manifest = std::fs::read_to_string(skill_dir.join("manifest.toml")).unwrap();
        assert!(manifest.contains("test-skill"));
        assert!(manifest.contains("calculator"));

        // Verify SKILL.md content
        let md = std::fs::read_to_string(skill_dir.join("SKILL.md")).unwrap();
        assert!(md.contains("# Test"));
    }

    #[tokio::test]
    async fn create_skill_invalid_name() {
        let tool = SkillCreateTool::new();
        let ctx = TestContext {
            workspace: tempfile::TempDir::new().unwrap(),
        };

        // Name with slash
        let result = tool
            .execute(
                serde_json::json!({"name": "bad/name", "description": "x", "content": "x"}),
                &ctx,
            )
            .await
            .unwrap();
        assert!(!result.success);
        assert!(result.output.contains("Invalid skill name"));

        // Name with spaces
        let result = tool
            .execute(
                serde_json::json!({"name": "bad name", "description": "x", "content": "x"}),
                &ctx,
            )
            .await
            .unwrap();
        assert!(!result.success);

        // Empty name
        let result = tool
            .execute(
                serde_json::json!({"name": "", "description": "x", "content": "x"}),
                &ctx,
            )
            .await
            .unwrap();
        assert!(!result.success);
    }

    #[tokio::test]
    async fn create_skill_missing_fields() {
        let tool = SkillCreateTool::new();
        let ctx = TestContext {
            workspace: tempfile::TempDir::new().unwrap(),
        };

        // Missing description
        let result = tool
            .execute(serde_json::json!({"name": "x", "content": "x"}), &ctx)
            .await
            .unwrap();
        assert!(!result.success);

        // Missing content
        let result = tool
            .execute(serde_json::json!({"name": "x", "description": "x"}), &ctx)
            .await
            .unwrap();
        assert!(!result.success);
    }

    #[tokio::test]
    async fn create_skill_overwrite_existing() {
        let tool = SkillCreateTool::new();
        let ctx = TestContext {
            workspace: tempfile::TempDir::new().unwrap(),
        };

        // Create first version
        let args = serde_json::json!({
            "name": "my-skill",
            "description": "Version 1",
            "content": "# V1",
            "activate": false
        });
        tool.execute(args, &ctx).await.unwrap();

        // Create second version (overwrite)
        let args = serde_json::json!({
            "name": "my-skill",
            "description": "Version 2",
            "content": "# V2",
            "activate": false
        });
        let result = tool.execute(args, &ctx).await.unwrap();
        assert!(result.success);

        let md = std::fs::read_to_string(
            ctx.workspace
                .path()
                .join(".clawseed/skills/my-skill/SKILL.md"),
        )
        .unwrap();
        assert!(md.contains("# V2"));
    }
}
