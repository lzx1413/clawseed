use async_trait::async_trait;
use clawseed_api::tool::{Tool, ToolResult};
use clawseed_api::tool_context::ToolContext;
use serde_json::json;
use std::path::{Path, PathBuf};

const MAX_RESULTS: usize = 1000;

/// Search for files by glob pattern within the workspace.
pub struct GlobSearchTool;

impl GlobSearchTool {
    pub fn new() -> Self {
        Self
    }
}

impl Default for GlobSearchTool {
    fn default() -> Self {
        Self::new()
    }
}

/// Resolve a glob pattern relative to the workspace. Rejects path traversal and absolute paths.
fn resolve_glob_pattern(pattern: &str, workspace: &Path) -> anyhow::Result<PathBuf> {
    if pattern.starts_with('/') || pattern.starts_with('\\') {
        anyhow::bail!("Absolute paths are not allowed. Use a relative glob pattern.");
    }
    if pattern.contains("../") || pattern.contains("..\\") || pattern == ".." {
        anyhow::bail!("Path traversal ('..') is not allowed in glob patterns.");
    }
    // Handle tilde expansion
    if pattern.starts_with('~') {
        if let Some(home) = std::env::var("HOME").ok().map(PathBuf::from) {
            let expanded = pattern.replacen('~', &home.to_string_lossy(), 1);
            return Ok(PathBuf::from(expanded));
        }
    }
    Ok(workspace.join(pattern))
}

/// Check that a resolved canonical path is within the workspace.
fn is_within_workspace(resolved: &Path, workspace_canon: &Path) -> bool {
    resolved.starts_with(workspace_canon)
}

#[async_trait]
impl Tool for GlobSearchTool {
    fn name(&self) -> &str {
        "glob_search"
    }

    fn description(&self) -> &str {
        "Search for files matching a glob pattern within the workspace. \
         Returns a sorted list of matching file paths relative to the workspace root. \
         Examples: '**/*.rs' (all Rust files), 'src/**/mod.rs' (all mod.rs in src)."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "pattern": {
                    "type": "string",
                    "description": "Glob pattern to match files, e.g. '**/*.rs', 'src/**/mod.rs'"
                }
            },
            "required": ["pattern"]
        })
    }

    async fn execute(
        &self,
        args: serde_json::Value,
        ctx: &dyn ToolContext,
    ) -> anyhow::Result<ToolResult> {
        let pattern = args
            .get("pattern")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'pattern' parameter"))?;

        let workspace = ctx.workspace_dir();

        // Security: reject absolute paths
        if pattern.starts_with('/') || pattern.starts_with('\\') {
            return Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some("Absolute paths are not allowed. Use a relative glob pattern.".into()),
            });
        }

        // Security: reject path traversal
        if pattern.contains("../") || pattern.contains("..\\") || pattern == ".." {
            return Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some("Path traversal ('..') is not allowed in glob patterns.".into()),
            });
        }

        // Build full pattern: resolve relative to workspace
        let full_pattern = resolve_glob_pattern(pattern, workspace)
            .map_err(|e| anyhow::anyhow!("{e}"))?
            .to_string_lossy()
            .to_string();

        let entries = match glob::glob(&full_pattern) {
            Ok(paths) => paths,
            Err(e) => {
                return Ok(ToolResult {
                    success: false,
                    output: String::new(),
                    error: Some(format!("Invalid glob pattern: {e}")),
                });
            }
        };

        let workspace_canon = match std::fs::canonicalize(workspace) {
            Ok(p) => p,
            Err(_) => {
                // Workspace dir may not exist yet (e.g. fresh install on Android).
                // Use the non-canonicalized path for comparison instead of failing.
                let workspace_canon = workspace.to_path_buf();
                let mut results = Vec::new();
                let mut truncated = false;

                for entry in entries {
                    let path = match entry {
                        Ok(p) => p,
                        Err(_) => continue,
                    };

                    if !path.starts_with(&workspace_canon) {
                        continue;
                    }

                    if path.is_dir() {
                        continue;
                    }

                    if let Ok(rel) = path.strip_prefix(&workspace_canon) {
                        results.push(rel.to_string_lossy().to_string());
                    }

                    if results.len() >= MAX_RESULTS {
                        truncated = true;
                        break;
                    }
                }

                results.sort();

                let output = if results.is_empty() {
                    format!("No files matching pattern '{pattern}' found in workspace.")
                } else {
                    use std::fmt::Write;
                    let mut buf = results.join("\n");
                    if truncated {
                        let _ = write!(
                            buf,
                            "\n\n[Results truncated: showing first {MAX_RESULTS} of more matches]"
                        );
                    }
                    let _ = write!(buf, "\n\nTotal: {} files", results.len());
                    buf
                };

                return Ok(ToolResult {
                    success: true,
                    output,
                    error: None,
                });
            }
        };

        let mut results = Vec::new();
        let mut truncated = false;

        for entry in entries {
            let path = match entry {
                Ok(p) => p,
                Err(_) => continue,
            };

            // Canonicalize to resolve symlinks, then verify still inside workspace
            let resolved = match std::fs::canonicalize(&path) {
                Ok(p) => p,
                Err(_) => continue,
            };

            if !is_within_workspace(&resolved, &workspace_canon) {
                continue;
            }

            // Only include files, not directories
            if resolved.is_dir() {
                continue;
            }

            // Convert to workspace-relative path
            if let Ok(rel) = resolved.strip_prefix(&workspace_canon) {
                results.push(rel.to_string_lossy().to_string());
            }

            if results.len() >= MAX_RESULTS {
                truncated = true;
                break;
            }
        }

        results.sort();

        let output = if results.is_empty() {
            format!("No files matching pattern '{pattern}' found in workspace.")
        } else {
            use std::fmt::Write;
            let mut buf = results.join("\n");
            if truncated {
                let _ = write!(
                    buf,
                    "\n\n[Results truncated: showing first {MAX_RESULTS} of more matches]"
                );
            }
            let _ = write!(buf, "\n\nTotal: {} files", results.len());
            buf
        };

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
    use tempfile::TempDir;

    struct TestToolContext {
        workspace: PathBuf,
    }

    impl ToolContext for TestToolContext {
        fn workspace_dir(&self) -> &Path {
            &self.workspace
        }
        fn get_any(
            &self,
            _type_id: std::any::TypeId,
        ) -> Option<&(dyn std::any::Any + Send + Sync)> {
            None
        }
    }

    fn ctx(workspace: PathBuf) -> TestToolContext {
        TestToolContext { workspace }
    }

    #[test]
    fn glob_search_name_and_schema() {
        let tool = GlobSearchTool::new();
        assert_eq!(tool.name(), "glob_search");

        let schema = tool.parameters_schema();
        assert!(schema["properties"]["pattern"].is_object());
        assert!(
            schema["required"]
                .as_array()
                .unwrap()
                .contains(&json!("pattern"))
        );
    }

    #[tokio::test]
    async fn glob_search_single_file() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("hello.txt"), "content").unwrap();

        let tool = GlobSearchTool::new();
        let result = tool
            .execute(
                json!({"pattern": "hello.txt"}),
                &ctx(dir.path().to_path_buf()),
            )
            .await
            .unwrap();

        assert!(result.success);
        assert!(result.output.contains("hello.txt"));
    }

    #[tokio::test]
    async fn glob_search_multiple_files() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("a.txt"), "").unwrap();
        std::fs::write(dir.path().join("b.txt"), "").unwrap();
        std::fs::write(dir.path().join("c.rs"), "").unwrap();

        let tool = GlobSearchTool::new();
        let result = tool
            .execute(json!({"pattern": "*.txt"}), &ctx(dir.path().to_path_buf()))
            .await
            .unwrap();

        assert!(result.success);
        assert!(result.output.contains("a.txt"));
        assert!(result.output.contains("b.txt"));
        assert!(!result.output.contains("c.rs"));
    }

    #[tokio::test]
    async fn glob_search_no_matches() {
        let dir = TempDir::new().unwrap();

        let tool = GlobSearchTool::new();
        let result = tool
            .execute(
                json!({"pattern": "*.nonexistent"}),
                &ctx(dir.path().to_path_buf()),
            )
            .await
            .unwrap();

        assert!(result.success);
        assert!(result.output.contains("No files matching pattern"));
    }

    #[tokio::test]
    async fn glob_search_missing_param() {
        let tool = GlobSearchTool::new();
        let result = tool.execute(json!({}), &ctx(std::env::temp_dir())).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn glob_search_rejects_absolute_path() {
        let tool = GlobSearchTool::new();
        let result = tool
            .execute(json!({"pattern": "/etc/**/*"}), &ctx(std::env::temp_dir()))
            .await
            .unwrap();

        assert!(!result.success);
        assert!(result.error.as_ref().unwrap().contains("Absolute paths"));
    }

    #[tokio::test]
    async fn glob_search_rejects_path_traversal() {
        let tool = GlobSearchTool::new();
        let result = tool
            .execute(
                json!({"pattern": "../../../etc/passwd"}),
                &ctx(std::env::temp_dir()),
            )
            .await
            .unwrap();

        assert!(!result.success);
        assert!(result.error.as_ref().unwrap().contains("Path traversal"));
    }

    #[tokio::test]
    async fn glob_search_rejects_dotdot_only() {
        let tool = GlobSearchTool::new();
        let result = tool
            .execute(json!({"pattern": ".."}), &ctx(std::env::temp_dir()))
            .await
            .unwrap();

        assert!(!result.success);
        assert!(result.error.as_ref().unwrap().contains("Path traversal"));
    }

    #[tokio::test]
    async fn glob_search_results_sorted() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("c.txt"), "").unwrap();
        std::fs::write(dir.path().join("a.txt"), "").unwrap();
        std::fs::write(dir.path().join("b.txt"), "").unwrap();

        let tool = GlobSearchTool::new();
        let result = tool
            .execute(json!({"pattern": "*.txt"}), &ctx(dir.path().to_path_buf()))
            .await
            .unwrap();

        assert!(result.success);
        let lines: Vec<&str> = result.output.lines().collect();
        assert!(lines.len() >= 3);
        assert_eq!(lines[0], "a.txt");
        assert_eq!(lines[1], "b.txt");
        assert_eq!(lines[2], "c.txt");
    }

    #[tokio::test]
    async fn glob_search_excludes_directories() {
        let dir = TempDir::new().unwrap();
        std::fs::create_dir(dir.path().join("subdir")).unwrap();
        std::fs::write(dir.path().join("file.txt"), "").unwrap();

        let tool = GlobSearchTool::new();
        let result = tool
            .execute(json!({"pattern": "*"}), &ctx(dir.path().to_path_buf()))
            .await
            .unwrap();

        assert!(result.success);
        assert!(result.output.contains("file.txt"));
        assert!(!result.output.contains("subdir"));
    }

    #[tokio::test]
    async fn glob_search_invalid_pattern() {
        let dir = TempDir::new().unwrap();

        let tool = GlobSearchTool::new();
        let result = tool
            .execute(
                json!({"pattern": "[invalid"}),
                &ctx(dir.path().to_path_buf()),
            )
            .await
            .unwrap();

        assert!(!result.success);
        assert!(
            result
                .error
                .as_ref()
                .unwrap()
                .contains("Invalid glob pattern")
        );
    }
}
