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
        "Execute a workspace command on host (Android/native shell) or linux (PRoot with Python/Node.js); defaults to host. Use background_run for asynchronous tasks."
    }
    fn parameters_schema(&self) -> Value {
        crate::command::command_schema(crate::command::MAX_SYNC_TIMEOUT_SECS)
    }
    async fn execute(&self, args: Value, ctx: &dyn ToolContext) -> anyhow::Result<ToolResult> {
        crate::command::execute_sync(
            args,
            ctx,
            crate::command::CommandBackend::Host,
            SHELL_TIMEOUT_SECS,
            None,
        )
        .await
    }
}

#[cfg(unix)]
pub(crate) async fn run_shell_command(
    command: &str,
    workspace: &Path,
    timeout: Duration,
    max_output_bytes: usize,
    cancel_rx: Option<watch::Receiver<bool>>,
) -> anyhow::Result<CommandOutput> {
    use tokio::process::Command;

    let mut cmd = Command::new("sh");
    cmd.arg("-c").arg(command).current_dir(workspace);
    run_command(cmd, timeout, max_output_bytes, cancel_rx).await
}

/// Run a prepared Unix command with the same timeout, cancellation, bounded
/// output, and process-group cleanup semantics used by the host shell tool.
#[cfg(unix)]
pub(crate) async fn run_command(
    cmd: tokio::process::Command,
    timeout: Duration,
    max_output_bytes: usize,
    cancel_rx: Option<watch::Receiver<bool>>,
) -> anyhow::Result<CommandOutput> {
    run_command_with_cleanup(cmd, timeout, max_output_bytes, cancel_rx, false).await
}

#[cfg(unix)]
pub(crate) async fn run_proot_command(
    mut cmd: tokio::process::Command,
    timeout: Duration,
    max_output_bytes: usize,
    cancel_rx: Option<watch::Receiver<bool>>,
) -> anyhow::Result<CommandOutput> {
    // ART blocks SIGQUIT for its signal catcher. Child executables inherit the
    // mask; PRoot then cannot receive our cancellation signal unless unblocked.
    // SAFETY: pre_exec runs only async-signal-safe libc operations in the child.
    unsafe {
        cmd.pre_exec(|| {
            let mut signals: libc::sigset_t = std::mem::zeroed();
            libc::sigemptyset(&mut signals);
            libc::sigaddset(&mut signals, libc::SIGQUIT);
            if libc::sigprocmask(libc::SIG_UNBLOCK, &signals, std::ptr::null_mut()) == -1
                || libc::signal(libc::SIGQUIT, libc::SIG_DFL) == libc::SIG_ERR
            {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    run_command_with_cleanup(cmd, timeout, max_output_bytes, cancel_rx, true).await
}

#[cfg(unix)]
async fn run_command_with_cleanup(
    mut cmd: tokio::process::Command,
    timeout: Duration,
    max_output_bytes: usize,
    mut cancel_rx: Option<watch::Receiver<bool>>,
    proot: bool,
) -> anyhow::Result<CommandOutput> {
    #[cfg(any(target_os = "linux", target_os = "android"))]
    {
        let parent = unsafe { libc::getpid() };
        let signal = if proot { libc::SIGQUIT } else { libc::SIGKILL };
        // SAFETY: only async-signal-safe operations run between fork and exec.
        unsafe {
            cmd.pre_exec(move || {
                if libc::prctl(libc::PR_SET_PDEATHSIG, signal) == -1 {
                    return Err(std::io::Error::last_os_error());
                }
                if libc::getppid() != parent {
                    libc::raise(signal);
                }
                Ok(())
            });
        }
    }
    let deadline = tokio::time::Instant::now() + timeout;
    cmd.stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    cmd.process_group(0);

    let mut child = cmd.spawn()?;
    let process_group = child.id().expect("spawned command has a PID");
    let stdout = child.stdout.take().expect("stdout was piped");
    let stderr = child.stderr.take().expect("stderr was piped");
    let mut stdout_task = tokio::spawn(read_bounded(stdout, max_output_bytes));
    let mut stderr_task = tokio::spawn(read_bounded(stderr, max_output_bytes));

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
            terminate_command(&mut child, proot, process_group).await?;
            let status = child.wait().await?;
            (CommandTermination::TimedOut, status.code())
        }
        WaitOutcome::Cancelled => {
            terminate_command(&mut child, proot, process_group).await?;
            let status = child.wait().await?;
            (CommandTermination::Cancelled, status.code())
        }
    };

    // A shell can exit while descendants still own its pipes. Include draining
    // in the timeout, otherwise even a successful exit can occupy a job forever.
    let drain_deadline = if termination == CommandTermination::Exited {
        deadline
    } else {
        tokio::time::Instant::now()
    } + Duration::from_secs(2);
    let drained = tokio::time::timeout_at(drain_deadline, async {
        tokio::join!(&mut stdout_task, &mut stderr_task)
    })
    .await;
    let (stdout, stderr) = match drained {
        Ok((stdout, stderr)) => (stdout??, stderr??),
        Err(_) => {
            terminate_command(&mut child, proot, process_group).await?;
            stdout_task.abort();
            stderr_task.abort();
            return Ok(CommandOutput {
                termination: if termination == CommandTermination::Cancelled {
                    CommandTermination::Cancelled
                } else {
                    CommandTermination::TimedOut
                },
                exit_code,
                stdout: String::new(),
                stderr: "Command output pipes did not close before the deadline".into(),
            });
        }
    };
    Ok(CommandOutput {
        termination,
        exit_code,
        stdout,
        stderr,
    })
}

#[cfg(unix)]
async fn terminate_command(
    child: &mut tokio::process::Child,
    proot: bool,
    process_group: u32,
) -> std::io::Result<()> {
    if proot {
        // Let PRoot stop its tracees first; force-killing only the tracer can orphan them.
        let sent = unsafe { libc::kill(process_group as i32, libc::SIGQUIT) };
        if sent == 0 {
            let _ = tokio::time::timeout(Duration::from_secs(2), child.wait()).await;
        }
    }
    // Use the original group ID even if wait() has already reaped the leader.
    // This also closes output pipes held by surviving descendants.
    let result = unsafe { libc::kill(-(process_group as i32), libc::SIGKILL) };
    if result == 0 {
        return Ok(());
    }
    let error = std::io::Error::last_os_error();
    if error.raw_os_error() == Some(libc::ESRCH) {
        Ok(())
    } else {
        Err(error)
    }
}

#[cfg(unix)]
pub(crate) async fn wait_for_cancel(cancel_rx: &mut watch::Receiver<bool>) {
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
pub(crate) async fn read_bounded<R: AsyncRead + Unpin>(
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

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn sigquit_mask_probe() {
        if std::env::var_os("CLAWSEED_SIGQUIT_PROBE").is_none() {
            return;
        }
        // SAFETY: read the calling thread's signal mask into valid storage.
        unsafe {
            let mut mask: libc::sigset_t = std::mem::zeroed();
            assert_eq!(
                libc::sigprocmask(libc::SIG_SETMASK, std::ptr::null(), &mut mask),
                0
            );
            assert_eq!(libc::sigismember(&mask, libc::SIGQUIT), 0);
            assert_eq!(libc::sigismember(&mask, libc::SIGUSR1), 1);
        }
    }

    #[tokio::test]
    async fn proot_unblocks_cancellation_signal_in_child_only() {
        let mut command = tokio::process::Command::new(std::env::current_exe().unwrap());
        command
            .args(["--exact", "shell::tests::sigquit_mask_probe"])
            .env("CLAWSEED_SIGQUIT_PROBE", "1");
        // SAFETY: simulate ART's inherited mask using async-signal-safe calls.
        unsafe {
            command.pre_exec(|| {
                let mut mask: libc::sigset_t = std::mem::zeroed();
                libc::sigemptyset(&mut mask);
                libc::sigaddset(&mut mask, libc::SIGQUIT);
                libc::sigaddset(&mut mask, libc::SIGUSR1);
                if libc::sigprocmask(libc::SIG_BLOCK, &mask, std::ptr::null_mut()) == -1 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
        let result = run_proot_command(command, Duration::from_secs(5), 4096, None)
            .await
            .unwrap();
        assert_eq!(
            result.exit_code,
            Some(0),
            "{} {}",
            result.stdout,
            result.stderr
        );
    }

    #[tokio::test]
    async fn exited_shell_with_inherited_output_pipes_cannot_bypass_timeout() {
        let workspace = tempfile::tempdir().unwrap();
        let result = tokio::time::timeout(
            Duration::from_secs(4),
            run_shell_command(
                "sleep 30 & printf parent-exited",
                workspace.path(),
                Duration::from_millis(20),
                1024,
                None,
            ),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(result.termination, CommandTermination::TimedOut);
    }

    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn cancellation_bounds_pipe_drain_even_with_a_detached_descendant() {
        let workspace = tempfile::tempdir().unwrap();
        let path = workspace.path().to_path_buf();
        let (cancel, receiver) = watch::channel(false);
        let job = tokio::spawn(async move {
            run_shell_command(
                "setsid sh -c 'echo $$ > escaped.pid; sleep 30' & wait",
                &path,
                Duration::from_secs(30),
                1024,
                Some(receiver),
            )
            .await
        });
        tokio::time::timeout(Duration::from_secs(3), async {
            while !std::fs::read_to_string(workspace.path().join("escaped.pid"))
                .is_ok_and(|s| !s.trim().is_empty())
            {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        let pid: i32 = std::fs::read_to_string(workspace.path().join("escaped.pid"))
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        struct Descendant(i32);
        impl Drop for Descendant {
            fn drop(&mut self) {
                // SAFETY: the isolated process group belongs to this test.
                unsafe {
                    libc::kill(-self.0, libc::SIGKILL);
                }
            }
        }
        let _cleanup = Descendant(pid);
        cancel.send(true).unwrap();
        let result = tokio::time::timeout(Duration::from_secs(5), job)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert_eq!(result.termination, CommandTermination::Cancelled);
    }
}
