//! Command options and execution shared by synchronous and background tools.

use crate::shell::{CommandOutput, CommandTermination};
use clawseed_api::tool::ToolResult;
use clawseed_api::tool_context::ToolContext;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::sync::watch;

pub const MAX_SYNC_TIMEOUT_SECS: u64 = 300;
pub const MAX_BACKGROUND_TIMEOUT_SECS: u64 = 3600;
const DEFAULT_MAX_OUTPUT_BYTES: usize = 1_048_576;
const MAX_OUTPUT_BYTES: usize = 4 * 1_024 * 1_024;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandBackend {
    #[default]
    #[serde(alias = "android")]
    Host,
    Linux,
}

#[derive(Debug, Clone, Copy)]
pub struct CommandOptions {
    pub backend: CommandBackend,
    pub timeout: Duration,
    pub max_output_bytes: usize,
}

impl CommandOptions {
    pub(crate) fn from_args(
        args: &Value,
        default_backend: CommandBackend,
        default_timeout_secs: u64,
        max_timeout_secs: u64,
    ) -> Result<Self, &'static str> {
        let backend = match args.get("backend") {
            None => default_backend,
            Some(value) => serde_json::from_value(value.clone())
                .map_err(|_| "backend must be host, android, or linux")?,
        };
        let timeout = match args.get("timeout_secs") {
            None => default_timeout_secs,
            Some(value) => value
                .as_u64()
                .filter(|value| (1..=max_timeout_secs).contains(value))
                .ok_or("timeout_secs is outside the supported range")?,
        };
        let max_output_bytes = match args.get("max_output_bytes") {
            None => DEFAULT_MAX_OUTPUT_BYTES,
            Some(value) => value
                .as_u64()
                .and_then(|value| usize::try_from(value).ok())
                .filter(|value| (1024..=MAX_OUTPUT_BYTES).contains(value))
                .ok_or("max_output_bytes must be between 1024 and 4194304")?,
        };
        Ok(Self {
            backend,
            timeout: Duration::from_secs(timeout),
            max_output_bytes,
        })
    }
}

pub(crate) fn command_schema(max_timeout_secs: u64) -> Value {
    json!({
        "type": "object",
        "properties": {
            "command": {"type": "string", "description": "Shell command to run in the workspace"},
            "backend": {
                "type": "string", "enum": ["host", "android", "linux"], "default": "host",
                "description": "host (or android): native shell; linux: installed PRoot environment with Python and Node.js. No fallback if Linux is unavailable."
            },
            "timeout_secs": {"type": "integer", "minimum": 1, "maximum": max_timeout_secs},
            "max_output_bytes": {"type": "integer", "minimum": 1024, "maximum": MAX_OUTPUT_BYTES}
        },
        "required": ["command"], "additionalProperties": false
    })
}

pub(crate) fn failure(message: impl Into<String>) -> ToolResult {
    ToolResult {
        success: false,
        output: String::new(),
        error: Some(message.into()),
        presentation: None,
    }
}

pub(crate) async fn execute_sync(
    args: Value,
    ctx: &dyn ToolContext,
    default_backend: CommandBackend,
    default_timeout_secs: u64,
    runtime_dir: Option<PathBuf>,
) -> anyhow::Result<ToolResult> {
    let Some(command) = args
        .get("command")
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
    else {
        return Ok(failure("command_required"));
    };
    let options = match CommandOptions::from_args(
        &args,
        default_backend,
        default_timeout_secs,
        MAX_SYNC_TIMEOUT_SECS,
    ) {
        Ok(options) => options,
        Err(error) => return Ok(failure(error)),
    };
    let result =
        match run_workspace_command(command, ctx.workspace_dir(), options, None, runtime_dir).await
        {
            Ok(result) => result,
            Err(error) => return Ok(failure(error.to_string())),
        };
    let error = match result.termination {
        CommandTermination::TimedOut => Some(format!(
            "Command timed out after {} seconds",
            options.timeout.as_secs()
        )),
        CommandTermination::Cancelled => Some("Command was cancelled".into()),
        CommandTermination::Exited if result.exit_code != Some(0) => {
            Some(if result.stderr.is_empty() {
                format!("Command exited with code {:?}", result.exit_code)
            } else {
                result.stderr
            })
        }
        CommandTermination::Exited => None,
    };
    Ok(ToolResult {
        success: error.is_none(),
        output: result.stdout,
        error,
        presentation: None,
    })
}

pub(crate) async fn run_workspace_command(
    command: &str,
    workspace: &Path,
    options: CommandOptions,
    cancel_rx: Option<watch::Receiver<bool>>,
    runtime_dir: Option<PathBuf>,
) -> anyhow::Result<CommandOutput> {
    if cancel_rx.as_ref().is_some_and(|cancel| *cancel.borrow()) {
        return Ok(CommandOutput {
            termination: CommandTermination::Cancelled,
            exit_code: None,
            stdout: String::new(),
            stderr: String::new(),
        });
    }
    anyhow::ensure!(
        workspace.is_dir(),
        "workspace is not a directory: {}",
        workspace.display()
    );
    #[cfg(unix)]
    match options.backend {
        CommandBackend::Host => {
            crate::shell::run_shell_command(
                command,
                workspace,
                options.timeout,
                options.max_output_bytes,
                cancel_rx,
            )
            .await
        }
        CommandBackend::Linux => {
            crate::linux_exec::run_linux_command(
                command,
                workspace,
                options.timeout,
                options.max_output_bytes,
                cancel_rx,
                runtime_dir,
            )
            .await
        }
    }
    #[cfg(not(unix))]
    {
        let _ = (command, options, cancel_rx, runtime_dir);
        anyhow::bail!("command execution is only supported on Unix platforms")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_backend_and_limits_without_falling_back() {
        let parse = |args| CommandOptions::from_args(&args, CommandBackend::Host, 30, 300);
        assert_eq!(parse(json!({})).unwrap().backend, CommandBackend::Host);
        assert_eq!(
            parse(json!({"backend":"android"})).unwrap().backend,
            CommandBackend::Host
        );
        assert_eq!(
            parse(json!({"backend":"linux"})).unwrap().backend,
            CommandBackend::Linux
        );
        for args in [
            json!({"backend":"unknown"}),
            json!({"backend":null}),
            json!({"timeout_secs":0}),
            json!({"timeout_secs":301}),
            json!({"timeout_secs":1.5}),
            json!({"max_output_bytes":1023}),
        ] {
            assert!(parse(args).is_err());
        }
    }
    #[cfg(unix)]
    #[tokio::test]
    async fn synchronous_tools_share_backend_routing_and_do_not_fall_back() {
        use clawseed_api::tool::Tool;
        let (runtime, context) = crate::linux_exec::tests::fake_runtime();
        let shell = crate::shell::ShellTool::new();
        for backend in [None, Some("host"), Some("android")] {
            let mut args = json!({"command":"printf host"});
            if let Some(backend) = backend {
                args["backend"] = json!(backend);
            }
            let result = shell.execute(args, &context).await.unwrap();
            assert!(result.success, "{result:?}");
            assert_eq!(result.output, "host");
            assert!(!runtime.path().join("launched").exists());
        }
        let result = execute_sync(
            json!({"command":"printf linux", "backend":"linux"}),
            &context,
            CommandBackend::Host,
            30,
            Some(runtime.path().into()),
        )
        .await
        .unwrap();
        assert!(result.success, "{result:?}");
        assert_eq!(result.output, "linux");
        assert!(runtime.path().join("launched").exists());
        let missing = tempfile::tempdir().unwrap();
        let result = execute_sync(
            json!({"command":"touch should-not-run", "backend":"linux"}),
            &context,
            CommandBackend::Host,
            30,
            Some(missing.path().into()),
        )
        .await
        .unwrap();
        assert!(!result.success);
        assert!(!context.workspace_dir().join("should-not-run").exists());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn cancellation_before_dispatch_never_starts_a_process() {
        let workspace = tempfile::tempdir().unwrap();
        for backend in [CommandBackend::Host, CommandBackend::Linux] {
            let (_sender, receiver) = watch::channel(true);
            let output = run_workspace_command(
                "touch should-not-run",
                workspace.path(),
                CommandOptions {
                    backend,
                    timeout: Duration::from_secs(1),
                    max_output_bytes: 1024,
                },
                Some(receiver),
                None,
            )
            .await
            .unwrap();
            assert_eq!(output.termination, CommandTermination::Cancelled);
            assert!(!workspace.path().join("should-not-run").exists());
        }
    }
}
