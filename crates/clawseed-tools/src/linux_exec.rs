//! Linux user-space execution through an installed PRoot runtime.

use async_trait::async_trait;
use clawseed_api::tool::{Tool, ToolResult};
use clawseed_api::tool_context::ToolContext;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

#[cfg(unix)]
use crate::shell::{CommandTermination, run_command};
use tokio::sync::Semaphore;

const RUNTIME_DIR_ENV: &str = "CLAWSEED_PROOT_RUNTIME_DIR";
const DEFAULT_TIMEOUT_SECS: u64 = 60;
const MAX_TIMEOUT_SECS: u64 = 300;
const DEFAULT_MAX_OUTPUT_BYTES: usize = 1_048_576;
const MAX_OUTPUT_BYTES: usize = 4 * 1_024 * 1_024;
const MAX_CONCURRENT_LINUX_EXEC: usize = 2;

fn linux_exec_limit() -> Arc<Semaphore> {
    static LIMIT: OnceLock<Arc<Semaphore>> = OnceLock::new();
    LIMIT
        .get_or_init(|| Arc::new(Semaphore::new(MAX_CONCURRENT_LINUX_EXEC)))
        .clone()
}

#[derive(Debug, Clone)]
struct RuntimeLayout {
    root: PathBuf,
    proot: PathBuf,
    rootfs: PathBuf,
    loader: Option<PathBuf>,
    library_dir: Option<PathBuf>,
}

impl RuntimeLayout {
    fn load(runtime_dir: Option<PathBuf>) -> Result<Self, String> {
        let root = runtime_dir
            .or_else(|| std::env::var_os(RUNTIME_DIR_ENV).map(PathBuf::from))
            .ok_or_else(|| {
                format!(
                    "PRoot runtime is not configured; set {RUNTIME_DIR_ENV} after installing it"
                )
            })?;
        let proot = std::env::var_os("CLAWSEED_PROOT_BINARY")
            .map(PathBuf::from)
            .unwrap_or_else(|| root.join("proot"));
        let rootfs = root.join("rootfs");
        if !proot.is_file() {
            return Err(format!("PRoot binary is missing: {}", proot.display()));
        }
        if !is_executable(&proot) {
            return Err(format!(
                "PRoot binary is not executable: {}",
                proot.display()
            ));
        }
        if !rootfs.is_dir() {
            return Err(format!("PRoot rootfs is missing: {}", rootfs.display()));
        }
        let loader = std::env::var_os("CLAWSEED_PROOT_LOADER")
            .map(PathBuf::from)
            .unwrap_or_else(|| root.join("proot-loader"));
        let loader = loader.is_file().then_some(loader);
        let library_dir = std::env::var_os("CLAWSEED_PROOT_LIBRARY_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| root.join("proot-libs"));
        let library_dir = library_dir.is_dir().then_some(library_dir);
        Ok(Self {
            root,
            proot,
            rootfs,
            loader,
            library_dir,
        })
    }

    fn status(&self) -> String {
        let manifest = self.root.join("manifest.json");
        let metadata = std::fs::read_to_string(&manifest)
            .ok()
            .and_then(|content| serde_json::from_str::<Value>(&content).ok())
            .map(|value| {
                let version = value
                    .get("runtime_version")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown");
                let runtime_id = value
                    .get("runtime_id")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown");
                format!(", version={version}, runtime_id={runtime_id}")
            })
            .unwrap_or_default();
        format!(
            "ready (runtime={}, proot={}, rootfs={}{})",
            self.root.display(),
            self.proot.display(),
            self.rootfs.display(),
            metadata,
        )
    }
}

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    path.metadata()
        .map(|metadata| metadata.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

#[cfg(not(unix))]
fn is_executable(path: &Path) -> bool {
    path.is_file()
}

#[derive(Default)]
pub struct LinuxExecTool {
    runtime_dir: Option<PathBuf>,
}

impl LinuxExecTool {
    pub fn new() -> Self {
        Self::default()
    }

    #[cfg(test)]
    fn with_runtime_dir(runtime_dir: PathBuf) -> Self {
        Self {
            runtime_dir: Some(runtime_dir),
        }
    }
}

#[async_trait]
impl Tool for LinuxExecTool {
    fn name(&self) -> &str {
        "linux_exec"
    }

    fn description(&self) -> &str {
        "Execute a command inside the installed PRoot Linux environment"
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": "Shell command to execute inside the Linux runtime"
                },
                "timeout_secs": {
                    "type": "integer",
                    "minimum": 1,
                    "maximum": MAX_TIMEOUT_SECS,
                    "description": "Maximum execution time in seconds"
                },
                "max_output_bytes": {
                    "type": "integer",
                    "minimum": 1024,
                    "maximum": MAX_OUTPUT_BYTES,
                    "description": "Maximum bytes retained from stdout and stderr"
                }
            },
            "required": ["command"]
        })
    }

    async fn execute(&self, args: Value, ctx: &dyn ToolContext) -> anyhow::Result<ToolResult> {
        let _permit = linux_exec_limit()
            .acquire_owned()
            .await
            .map_err(|_| anyhow::anyhow!("Linux execution capacity is unavailable"))?;
        let command = args.get("command").and_then(Value::as_str).unwrap_or("");
        if command.trim().is_empty() {
            return Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some("command must not be empty".to_string()),
                presentation: None,
            });
        }

        let timeout_secs = args
            .get("timeout_secs")
            .and_then(Value::as_u64)
            .unwrap_or(DEFAULT_TIMEOUT_SECS)
            .clamp(1, MAX_TIMEOUT_SECS);
        let max_output_bytes = args
            .get("max_output_bytes")
            .and_then(Value::as_u64)
            .and_then(|value| usize::try_from(value).ok())
            .unwrap_or(DEFAULT_MAX_OUTPUT_BYTES)
            .clamp(1024, MAX_OUTPUT_BYTES);

        #[cfg(unix)]
        {
            let layout = match RuntimeLayout::load(self.runtime_dir.clone()) {
                Ok(layout) => layout,
                Err(error) => {
                    return Ok(ToolResult {
                        success: false,
                        output: String::new(),
                        error: Some(error),
                        presentation: None,
                    });
                }
            };

            let workspace = ctx.workspace_dir();
            if !workspace.is_dir() {
                return Ok(ToolResult {
                    success: false,
                    output: String::new(),
                    error: Some(format!(
                        "workspace is not a directory: {}",
                        workspace.display()
                    )),
                    presentation: None,
                });
            }

            let task_tmp = tempfile::Builder::new()
                .prefix("clawseed-linux-")
                .tempdir()
                .map_err(|error| {
                    anyhow::anyhow!("could not create task temp directory: {error}")
                })?;
            let proot_tmp = task_tmp.path().join("proot-tmp");
            let proot_l2s = task_tmp.path().join("proot-l2s");
            std::fs::create_dir(&proot_tmp)?;
            std::fs::create_dir(&proot_l2s)?;

            let mut cmd = tokio::process::Command::new(&layout.proot);
            cmd.args(["-0", "-r"])
                .arg(&layout.rootfs)
                .args(["-b"])
                .arg(format!("{}:/workspace", workspace.display()))
                .args(["-b"])
                .arg(format!("{}:/tmp", task_tmp.path().display()))
                .args(["-w", "/workspace", "/bin/sh", "-c"])
                .arg(command)
                .current_dir(workspace)
                .env_clear()
                .env("HOME", "/root")
                .env(
                    "PATH",
                    "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin",
                )
                .env("LANG", "C.UTF-8")
                .env("TMPDIR", "/tmp")
                .env("TERM", "dumb");
            if let Some(loader) = &layout.loader {
                cmd.env("PROOT_LOADER", loader);
            }
            if let Some(library_dir) = &layout.library_dir {
                cmd.env("LD_LIBRARY_PATH", library_dir);
            }
            cmd.env("PROOT_TMP_DIR", &proot_tmp)
                .env("PROOT_L2S_DIR", &proot_l2s);

            let result = run_command(
                cmd,
                Duration::from_secs(timeout_secs),
                max_output_bytes,
                None,
            )
            .await?;
            let timeout_error = format!("Linux command timed out after {timeout_secs} seconds");
            let cancelled_error = "Linux command was cancelled".to_string();
            let error = match result.termination {
                CommandTermination::TimedOut => Some(timeout_error),
                CommandTermination::Cancelled => Some(cancelled_error),
                CommandTermination::Exited if result.exit_code != Some(0) => {
                    Some(if result.stderr.is_empty() {
                        format!("Linux command exited with code {:?}", result.exit_code)
                    } else {
                        result.stderr.clone()
                    })
                }
                CommandTermination::Exited => None,
            };

            return Ok(ToolResult {
                success: error.is_none(),
                output: result.stdout,
                error,
                presentation: None,
            });
        }

        #[cfg(not(unix))]
        {
            let _ = (timeout_secs, max_output_bytes, ctx);
            Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some(
                    "PRoot Linux execution is only supported on Unix platforms".to_string(),
                ),
                presentation: None,
            })
        }
    }
}

#[derive(Default)]
pub struct RuntimeStatusTool {
    runtime_dir: Option<PathBuf>,
}

impl RuntimeStatusTool {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl Tool for RuntimeStatusTool {
    fn name(&self) -> &str {
        "runtime_status"
    }

    fn description(&self) -> &str {
        "Report whether the PRoot Linux runtime is installed and ready"
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({"type": "object", "properties": {}})
    }

    async fn execute(&self, _args: Value, _ctx: &dyn ToolContext) -> anyhow::Result<ToolResult> {
        match RuntimeLayout::load(self.runtime_dir.clone()) {
            Ok(layout) => Ok(ToolResult {
                success: true,
                output: layout.status(),
                error: None,
                presentation: None,
            }),
            Err(error) => Ok(ToolResult {
                success: false,
                output: "not_ready".to_string(),
                error: Some(error),
                presentation: None,
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clawseed_api::tool_context::ToolContext;
    use std::os::unix::fs::PermissionsExt;
    use tempfile::TempDir;

    struct Context {
        workspace: PathBuf,
    }

    impl ToolContext for Context {
        fn workspace_dir(&self) -> &Path {
            &self.workspace
        }
    }

    fn fake_runtime() -> (TempDir, Context) {
        let runtime = tempfile::tempdir().unwrap();
        let rootfs = runtime.path().join("rootfs");
        std::fs::create_dir(&rootfs).unwrap();
        let proot = runtime.path().join("proot");
        std::fs::write(&proot, b"#!/bin/sh\nshift 2\nwhile [ \"$1\" != \"/bin/sh\" ]; do shift; done\nshift\nshift\nexec /bin/sh -c \"$1\"\n").unwrap();
        let mut permissions = std::fs::metadata(&proot).unwrap().permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&proot, permissions).unwrap();
        let workspace = runtime.path().join("workspace");
        std::fs::create_dir(&workspace).unwrap();
        let context = Context { workspace };
        (runtime, context)
    }

    #[tokio::test]
    async fn missing_runtime_is_reported_without_fallback() {
        let tool = LinuxExecTool::with_runtime_dir(PathBuf::from("/definitely/missing"));
        let workspace = tempfile::tempdir().unwrap();
        let context = Context {
            workspace: workspace.path().to_path_buf(),
        };
        let result = tool
            .execute(serde_json::json!({"command": "echo no"}), &context)
            .await
            .unwrap();
        assert!(!result.success);
        assert!(result.error.unwrap().contains("PRoot binary is missing"));
    }

    #[tokio::test]
    async fn executes_inside_runtime_and_returns_output() {
        let (runtime, context) = fake_runtime();
        let tool = LinuxExecTool::with_runtime_dir(runtime.path().to_path_buf());
        let result = tool
            .execute(serde_json::json!({"command": "printf ready"}), &context)
            .await
            .unwrap();
        assert!(result.success, "{result:?}");
        assert_eq!(result.output, "ready");
    }
}
