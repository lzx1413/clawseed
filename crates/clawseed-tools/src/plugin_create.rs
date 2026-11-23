//! Tools for scaffolding and validating local CETP plugins.

use crate::plugins::{PluginManifest, valid_plugin_id, validate_plugin_root};
use anyhow::Result;
use async_trait::async_trait;
use clawseed_api::tool::{Tool, ToolResult};
use clawseed_api::tool_context::ToolContext;
use clawseed_config::schema::PluginsConfig;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::PathBuf;

#[derive(Debug, Deserialize, Serialize)]
struct PluginToolInput {
    name: String,
    description: String,
    #[serde(default = "default_parameters")]
    parameters: Value,
}

fn default_parameters() -> Value {
    serde_json::json!({"type": "object", "properties": {}, "additionalProperties": false})
}

pub struct PluginCreateTool;

impl PluginCreateTool {
    pub fn new() -> Self {
        Self
    }
}

impl Default for PluginCreateTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for PluginCreateTool {
    fn name(&self) -> &str {
        "plugin_create"
    }

    fn description(&self) -> &str {
        "Create a Python or Node.js CETP plugin scaffold in the workspace"
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "plugin_id": {"type": "string", "description": "Stable lowercase plugin ID"},
                "language": {"type": "string", "enum": ["python", "node"]},
                "description": {"type": "string"},
                "tools": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "name": {"type": "string"},
                            "description": {"type": "string"},
                            "parameters": {"type": "object"}
                        },
                        "required": ["name", "description"]
                    }
                },
                "overwrite": {"type": "boolean", "default": false}
            },
            "required": ["plugin_id", "language", "description"]
        })
    }

    async fn execute(&self, args: Value, ctx: &dyn ToolContext) -> Result<ToolResult> {
        let plugin_id = args.get("plugin_id").and_then(Value::as_str).unwrap_or("");
        let language = args.get("language").and_then(Value::as_str).unwrap_or("");
        let description = args
            .get("description")
            .and_then(Value::as_str)
            .unwrap_or("");
        if !valid_plugin_id(plugin_id) {
            return Ok(failure(
                "plugin_id must use lowercase letters, digits, '.', '_' or '-'",
            ));
        }
        if !matches!(language, "python" | "node") {
            return Ok(failure("language must be python or node"));
        }
        if description.trim().is_empty() {
            return Ok(failure("description is required"));
        }
        let tools = match args.get("tools") {
            None => vec![PluginToolInput {
                name: "hello".into(),
                description: "Return a greeting".into(),
                parameters: default_parameters(),
            }],
            Some(value) => match serde_json::from_value::<Vec<PluginToolInput>>(value.clone()) {
                Ok(tools) if !tools.is_empty() => tools,
                _ => return Ok(failure("tools must be a non-empty array")),
            },
        };
        for tool in &tools {
            if !valid_tool_name(&tool.name) || tool.description.trim().is_empty() {
                return Ok(failure(
                    "tool names must use lowercase letters, digits and '_' and descriptions are required",
                ));
            }
        }
        let root = ctx
            .workspace_dir()
            .join(".clawseed/plugins")
            .join(plugin_id);
        if root.exists() {
            let overwrite = args
                .get("overwrite")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            if !overwrite {
                return Ok(failure(
                    "plugin already exists; pass overwrite=true to replace it",
                ));
            }
        }
        tokio::fs::create_dir_all(&root).await?;
        let entry_name = if language == "python" {
            "plugin.py"
        } else {
            "plugin.js"
        };
        let manifest = serde_json::json!({
            "schema_version": 1,
            "plugin_id": plugin_id,
            "name": plugin_id,
            "version": "0.1.0",
            "description": description,
            "entry": entry_name,
            "protocol": {"versions": [2], "transport": "stdio"},
            "runtime": {"language": language, "components": [language]},
            "permissions": {"workspace": "none", "network": "deny", "environment": []},
            "limits": {
                "startup_timeout_ms": 5000,
                "request_timeout_ms": 30000,
                "max_concurrent_requests": 1,
                "max_output_bytes": 1048576
            }
        });
        let manifest_path = root.join("manifest.json");
        tokio::fs::write(&manifest_path, serde_json::to_vec_pretty(&manifest)?).await?;
        let entry_path = root.join(entry_name);
        let entry = if language == "python" {
            python_template(plugin_id, &tools)?
        } else {
            node_template(plugin_id, &tools)?
        };
        tokio::fs::write(&entry_path, entry).await?;
        tokio::fs::write(
            root.join("README.md"),
            format!("# {plugin_id}\n\nCETP {language} plugin. Run `plugin_validate` after editing the entrypoint.\n"),
        )
        .await?;
        Ok(ToolResult {
            success: true,
            output: format!(
                "Created {language} plugin at {}.\nFiles:\n- .clawseed/plugins/{plugin_id}/manifest.json\n- .clawseed/plugins/{plugin_id}/{entry_name}\n- .clawseed/plugins/{plugin_id}/README.md\nEnable [plugins].enabled after review.",
                root.display()
            ),
            error: None,
            presentation: None,
        })
    }
}

pub struct PluginValidateTool;

impl PluginValidateTool {
    pub fn new() -> Self {
        Self
    }
}

/// Reports discovered plugin manifests without starting plugin processes.
pub struct PluginListTool {
    workspace_dir: PathBuf,
    config: PluginsConfig,
}

impl PluginListTool {
    pub fn new(workspace_dir: PathBuf, config: PluginsConfig) -> Self {
        Self {
            workspace_dir,
            config,
        }
    }
}

#[async_trait]
impl Tool for PluginListTool {
    fn name(&self) -> &str {
        "plugin_list"
    }

    fn description(&self) -> &str {
        "List local Python and Node.js plugins with manifest and load status"
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {},
            "additionalProperties": false
        })
    }

    async fn execute(&self, _args: Value, ctx: &dyn ToolContext) -> Result<ToolResult> {
        let workspace_dir = if ctx.workspace_dir().as_os_str().is_empty() {
            &self.workspace_dir
        } else {
            ctx.workspace_dir()
        };
        let mut roots = vec![workspace_dir.join(".clawseed/plugins")];
        roots.extend(self.config.extra_roots.iter().map(PathBuf::from));
        let mut entries = Vec::new();
        for root in roots {
            let Ok(mut directory) = tokio::fs::read_dir(&root).await else {
                continue;
            };
            while let Some(entry) = directory.next_entry().await? {
                let plugin_dir = entry.path();
                if !plugin_dir.is_dir() {
                    continue;
                }
                let manifest_path = plugin_dir.join("manifest.json");
                let manifest = match tokio::fs::read(&manifest_path)
                    .await
                    .ok()
                    .and_then(|bytes| serde_json::from_slice::<PluginManifest>(&bytes).ok())
                {
                    Some(manifest) => manifest,
                    None => continue,
                };
                let validation_error = manifest
                    .validate(&plugin_dir)
                    .err()
                    .map(|error| error.to_string());
                let enabled = self.config.enabled
                    && (self.config.allowed.is_empty()
                        || self
                            .config
                            .allowed
                            .iter()
                            .any(|id| id == &manifest.plugin_id))
                    && !self
                        .config
                        .denied
                        .iter()
                        .any(|id| id == &manifest.plugin_id)
                    && validation_error.is_none();
                entries.push(serde_json::json!({
                    "plugin_id": manifest.plugin_id,
                    "name": manifest.name,
                    "version": manifest.version,
                    "language": manifest.runtime.language,
                    "enabled": enabled,
                    "status": if validation_error.is_some() { "invalid" } else if enabled { "enabled" } else { "disabled" },
                    "validation_error": validation_error,
                }));
            }
        }
        Ok(ToolResult {
            success: true,
            output: serde_json::to_string_pretty(&entries)?,
            error: None,
            presentation: None,
        })
    }
}

impl Default for PluginValidateTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for PluginValidateTool {
    fn name(&self) -> &str {
        "plugin_validate"
    }

    fn description(&self) -> &str {
        "Validate a local Python or Node.js plugin manifest and CETP handshake"
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {"plugin_id": {"type": "string"}},
            "required": ["plugin_id"],
            "additionalProperties": false
        })
    }

    async fn execute(&self, args: Value, ctx: &dyn ToolContext) -> Result<ToolResult> {
        let plugin_id = args.get("plugin_id").and_then(Value::as_str).unwrap_or("");
        if !valid_plugin_id(plugin_id) {
            return Ok(failure("invalid plugin_id"));
        }
        let root = ctx
            .workspace_dir()
            .join(".clawseed/plugins")
            .join(plugin_id);
        match validate_plugin_root(&root, ctx.workspace_dir()).await {
            Ok((manifest, tools)) => Ok(ToolResult {
                success: true,
                output: format!(
                    "Plugin '{}' is valid. language={}, version={}, tools={}",
                    manifest.plugin_id,
                    manifest.runtime.language,
                    manifest.version,
                    tools
                        .iter()
                        .map(|tool| tool.name.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
                error: None,
                presentation: None,
            }),
            Err(error) => Ok(failure(format!("plugin validation failed: {error:#}"))),
        }
    }
}

fn failure(message: impl Into<String>) -> ToolResult {
    let message = message.into();
    ToolResult {
        success: false,
        output: message.clone(),
        error: Some(message),
        presentation: None,
    }
}

fn valid_tool_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value.as_bytes()[0].is_ascii_lowercase()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
}

fn python_template(plugin_id: &str, tools: &[PluginToolInput]) -> Result<String> {
    let tools = serde_json::to_string(tools)?;
    Ok(format!(
        r#"import json
import sys

PLUGIN_ID = {plugin_id:?}
TOOLS = json.loads({tools:?})

for line in sys.stdin:
    request = json.loads(line)
    method = request.get("method")
    if method == "negotiate":
        data = {{"selected_version": 2, "session_token": "local-session"}}
    elif method == "get_provider_info":
        data = {{"provider_id": PLUGIN_ID, "provider_name": PLUGIN_ID}}
    elif method == "list_tools":
        data = {{"revision": "tools-1", "tools": TOOLS}}
    elif method == "execute_tool":
        data = {{"tool_name": request["params"].get("tool_name"), "args": request["params"].get("args")}}
    else:
        print(json.dumps({{"id": request.get("id"), "status": "error", "error": {{"code": "METHOD_NOT_FOUND", "message": method}}}}), flush=True)
        continue
    print(json.dumps({{"id": request.get("id"), "status": "success", "data": data}}), flush=True)
"#
    ))
}

fn node_template(plugin_id: &str, tools: &[PluginToolInput]) -> Result<String> {
    let tools = serde_json::to_string(tools)?;
    Ok(format!(
        r#"const readline = require('readline');
const PLUGIN_ID = {plugin_id:?};
const TOOLS = {tools};
const input = readline.createInterface({{ input: process.stdin }});
input.on('line', (line) => {{
  const request = JSON.parse(line);
  const method = request.method;
  let data;
  if (method === 'negotiate') data = {{ selected_version: 2, session_token: 'local-session' }};
  else if (method === 'get_provider_info') data = {{ provider_id: PLUGIN_ID, provider_name: PLUGIN_ID }};
  else if (method === 'list_tools') data = {{ revision: 'tools-1', tools: TOOLS }};
  else if (method === 'execute_tool') data = {{ tool_name: request.params.tool_name, args: request.params.args }};
  else {{ process.stdout.write(JSON.stringify({{ id: request.id, status: 'error', error: {{ code: 'METHOD_NOT_FOUND', message: method }} }}) + '\n'); return; }}
  process.stdout.write(JSON.stringify({{ id: request.id, status: 'success', data }}) + '\n');
}});
"#
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugins::PluginManifest;
    use clawseed_api::tool_context::ToolContext;
    use std::path::{Path, PathBuf};
    use tempfile::tempdir;

    struct TestContext(PathBuf);

    impl ToolContext for TestContext {
        fn workspace_dir(&self) -> &Path {
            &self.0
        }
    }

    #[tokio::test]
    async fn creates_python_plugin_scaffold() {
        let workspace = tempdir().unwrap();
        let result = PluginCreateTool::new()
            .execute(
                serde_json::json!({
                    "plugin_id": "example.echo",
                    "language": "python",
                    "description": "Echo values"
                }),
                &TestContext(workspace.path().to_path_buf()),
            )
            .await
            .unwrap();
        assert!(result.success);
        let root = workspace.path().join(".clawseed/plugins/example.echo");
        let manifest: PluginManifest =
            serde_json::from_slice(&std::fs::read(root.join("manifest.json")).unwrap()).unwrap();
        assert_eq!(manifest.runtime.language, "python");
        assert!(root.join("plugin.py").is_file());
    }

    #[tokio::test]
    async fn rejects_plugin_overwrite_by_default() {
        let workspace = tempdir().unwrap();
        let context = TestContext(workspace.path().to_path_buf());
        let tool = PluginCreateTool::new();
        let args = serde_json::json!({
            "plugin_id": "example.echo",
            "language": "node",
            "description": "Echo values"
        });
        assert!(tool.execute(args.clone(), &context).await.unwrap().success);
        let result = tool.execute(args, &context).await.unwrap();
        assert!(!result.success);
        assert!(result.output.contains("already exists"));
    }

    #[tokio::test]
    async fn creates_node_plugin_scaffold() {
        let workspace = tempdir().unwrap();
        let context = TestContext(workspace.path().to_path_buf());
        let result = PluginCreateTool::new()
            .execute(
                serde_json::json!({
                    "plugin_id": "example.node",
                    "language": "node",
                    "description": "Node echo"
                }),
                &context,
            )
            .await
            .unwrap();
        assert!(result.success);
        assert!(
            workspace
                .path()
                .join(".clawseed/plugins/example.node/plugin.js")
                .is_file()
        );
    }
}
