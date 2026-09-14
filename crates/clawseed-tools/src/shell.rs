use async_trait::async_trait;
use clawseed_api::tool::{Tool, ToolResult};
use clawseed_api::tool_context::ToolContext;
use serde_json::Value;
use std::path::Path;
use std::process::Stdio;
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::sync::watch;

const SHELL_TIMEOUT_SECS: u64 = 30;
const MAX_OUTPUT_BYTES: usize = 1_048_576;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CommandTermination {
    Exited,
    TimedOut,
    Cancelled,
}

pub(crate) struct CommandOutput {
    pub termination: CommandTermination,
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

#[derive(Default)]
pub struct ShellTool;

impl ShellTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for ShellTool {
    fn name(&self) -> &str {
        "shell"
    }
    fn description(&self) -> &str {
        "Execute a shell command"
    }
    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "command": { "type": "string", "description": "The command to execute" }
            },
            "required": ["command"]
        })
    }
    async fn execute(&self, args: Value, ctx: &dyn ToolContext) -> anyhow::Result<ToolResult> {
        let command = args.get("command").and_then(|v| v.as_str()).unwrap_or("");
        #[cfg(unix)]
        let output = {
            let result = run_shell_command(
                command,
                ctx.workspace_dir(),
                Duration::from_secs(SHELL_TIMEOUT_SECS),
                MAX_OUTPUT_BYTES,
                None,
            )
            .await?;
            if result.termination == CommandTermination::TimedOut {
                ToolResult {
                    success: false,
                    output: result.stdout,
                    error: Some(format!(
                        "Command timed out after {SHELL_TIMEOUT_SECS} seconds"
                    )),
                    presentation: None,
                }
            } else if result.exit_code == Some(0) {
                ToolResult {
                    success: true,
                    output: result.stdout,
                    error: None,
                    presentation: None,
                }
            } else {
                ToolResult {
                    success: false,
                    output: result.stdout,
                    error: Some(result.stderr),
                    presentation: None,
                }
            }
        };
        #[cfg(not(unix))]
        let output = ToolResult {
            success: false,
            output: String::new(),
            error: Some("Shell not supported on this platform".to_string()),
            presentation: None,
        };
        Ok(output)
    }
}

#[cfg(unix)]
pub(crate) async fn run_shell_command(
    command: &str,
    workspace: &Path,
    timeout: Duration,
    max_output_bytes: usize,
    mut cancel_rx: Option<watch::Receiver<bool>>,
) -> anyhow::Result<CommandOutput> {
    use tokio::process::Command;

    let mut cmd = Command::new("sh");
    cmd.arg("-c")
        .arg(command)
        .current_dir(workspace)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    cmd.process_group(0);

    let mut child = cmd.spawn()?;
    let stdout = child.stdout.take().expect("stdout was piped");
    let stderr = child.stderr.take().expect("stderr was piped");
    let stdout_task = tokio::spawn(read_bounded(stdout, max_output_bytes));
    let stderr_task = tokio::spawn(read_bounded(stderr, max_output_bytes));

    enum WaitOutcome {
        Exited(std::io::Result<std::process::ExitStatus>),
        TimedOut,
        Cancelled,
    }

    let outcome = {
        let wait = child.wait();
        tokio::pin!(wait);
        if let Some(cancel) = cancel_rx.as_mut() {
            tokio::select! {
                status = &mut wait => WaitOutcome::Exited(status),
                _ = tokio::time::sleep(timeout) => WaitOutcome::TimedOut,
                _ = wait_for_cancel(cancel) => WaitOutcome::Cancelled,
            }
        } else {
            tokio::select! {
                status = &mut wait => WaitOutcome::Exited(status),
                _ = tokio::time::sleep(timeout) => WaitOutcome::TimedOut,
            }
        }
    };

    let (termination, exit_code) = match outcome {
        WaitOutcome::Exited(status) => {
            let status = status?;
            (CommandTermination::Exited, status.code())
        }
        WaitOutcome::TimedOut => {
            kill_process_group(&mut child).await?;
            let status = child.wait().await?;
            (CommandTermination::TimedOut, status.code())
        }
        WaitOutcome::Cancelled => {
            kill_process_group(&mut child).await?;
            let status = child.wait().await?;
            (CommandTermination::Cancelled, status.code())
        }
    };

    Ok(CommandOutput {
        termination,
        exit_code,
        stdout: stdout_task.await??,
        stderr: stderr_task.await??,
    })
}

#[cfg(unix)]
async fn kill_process_group(child: &mut tokio::process::Child) -> std::io::Result<()> {
    if let Some(pid) = child.id() {
        // The child is the leader of a process group created above. A negative
        // PID targets that group, including grandchildren spawned by the shell.
        let result = unsafe { libc::kill(-(pid as i32), libc::SIGKILL) };
        if result == 0 {
            return Ok(());
        }
        let error = std::io::Error::last_os_error();
        if error.raw_os_error() == Some(libc::ESRCH) {
            return Ok(());
        }
        return Err(error);
    }
    child.start_kill()
}

#[cfg(unix)]
async fn wait_for_cancel(cancel_rx: &mut watch::Receiver<bool>) {
    if *cancel_rx.borrow() {
        return;
    }
    while cancel_rx.changed().await.is_ok() {
        if *cancel_rx.borrow() {
            return;
        }
    }
}

#[cfg(unix)]
async fn read_bounded<R: AsyncRead + Unpin>(
    mut reader: R,
    max_output_bytes: usize,
) -> std::io::Result<String> {
    let mut kept = Vec::with_capacity(max_output_bytes.min(64 * 1024));
    let mut buffer = [0_u8; 8192];
    let mut truncated = false;
    loop {
        let read = reader.read(&mut buffer).await?;
        if read == 0 {
            break;
        }
        let remaining = max_output_bytes.saturating_sub(kept.len());
        let to_keep = remaining.min(read);
        kept.extend_from_slice(&buffer[..to_keep]);
        truncated |= to_keep < read;
    }
    let mut output = String::from_utf8_lossy(&kept).into_owned();
    if truncated {
        output.push_str("\n[Output truncated]");
    }
    Ok(output)
}
