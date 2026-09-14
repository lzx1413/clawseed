use crate::shell::{CommandTermination, run_shell_command};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use clawseed_api::tool::{Tool, ToolResult};
use clawseed_api::tool_context::ToolContext;
use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;
use tokio::sync::{Notify, Semaphore, watch};
use uuid::Uuid;

const DEFAULT_MAX_RUNNING: usize = 2;
const DEFAULT_MAX_QUEUED: usize = 16;
const DEFAULT_TIMEOUT_SECS: u64 = 300;
const DEFAULT_MAX_OUTPUT_BYTES: usize = 1_048_576;

pub type BackgroundEventSink = Arc<dyn Fn(BackgroundJobEvent) + Send + Sync>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BackgroundJobStatus {
    Queued,
    Running,
    Succeeded,
    Failed,
    Cancelled,
}

impl BackgroundJobStatus {
    fn is_terminal(self) -> bool {
        matches!(self, Self::Succeeded | Self::Failed | Self::Cancelled)
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct BackgroundJobSnapshot {
    pub job_id: String,
    pub status: BackgroundJobStatus,
    pub created_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BackgroundJobEvent {
    pub job_id: String,
    pub status: BackgroundJobStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Clone)]
struct BackgroundLimits {
    max_running: usize,
    max_queued: usize,
    timeout: Duration,
    max_output_bytes: usize,
}

impl Default for BackgroundLimits {
    fn default() -> Self {
        Self {
            max_running: DEFAULT_MAX_RUNNING,
            max_queued: DEFAULT_MAX_QUEUED,
            timeout: Duration::from_secs(DEFAULT_TIMEOUT_SECS),
            max_output_bytes: DEFAULT_MAX_OUTPUT_BYTES,
        }
    }
}

struct JobRecord {
    snapshot: BackgroundJobSnapshot,
    cancel_tx: watch::Sender<bool>,
    terminal: Arc<Notify>,
}

pub struct BackgroundJobManager {
    jobs: Mutex<HashMap<String, JobRecord>>,
    permits: Arc<Semaphore>,
    limits: BackgroundLimits,
    event_sink: Option<BackgroundEventSink>,
}

impl BackgroundJobManager {
    pub fn new(event_sink: Option<BackgroundEventSink>) -> Arc<Self> {
        Self::with_limits(BackgroundLimits::default(), event_sink)
    }

    fn with_limits(limits: BackgroundLimits, event_sink: Option<BackgroundEventSink>) -> Arc<Self> {
        Arc::new(Self {
            jobs: Mutex::new(HashMap::new()),
            permits: Arc::new(Semaphore::new(limits.max_running)),
            limits,
            event_sink,
        })
    }

    fn jobs(&self) -> MutexGuard<'_, HashMap<String, JobRecord>> {
        self.jobs.lock().unwrap_or_else(|error| error.into_inner())
    }

    pub fn start(
        self: &Arc<Self>,
        command: String,
        workspace: PathBuf,
    ) -> Result<BackgroundJobSnapshot, &'static str> {
        if command.trim().is_empty() {
            return Err("command_required");
        }

        let (cancel_tx, cancel_rx) = watch::channel(false);
        let terminal = Arc::new(Notify::new());
        let job_id = Uuid::new_v4().to_string();
        let snapshot = BackgroundJobSnapshot {
            job_id: job_id.clone(),
            status: BackgroundJobStatus::Queued,
            created_at: Utc::now(),
            started_at: None,
            finished_at: None,
            exit_code: None,
            stdout: String::new(),
            stderr: String::new(),
            error: None,
        };

        {
            let mut jobs = self.jobs();
            let active = jobs
                .values()
                .filter(|job| !job.snapshot.status.is_terminal())
                .count();
            if active >= self.limits.max_running + self.limits.max_queued {
                return Err("queue_full");
            }
            jobs.insert(
                job_id.clone(),
                JobRecord {
                    snapshot: snapshot.clone(),
                    cancel_tx,
                    terminal,
                },
            );
        }

        let manager = Arc::clone(self);
        tokio::spawn(async move {
            manager.run_job(job_id, command, workspace, cancel_rx).await;
        });
        Ok(snapshot)
    }

    pub fn status(&self, job_id: &str) -> Option<BackgroundJobSnapshot> {
        self.jobs().get(job_id).map(|job| job.snapshot.clone())
    }

    pub async fn cancel(&self, job_id: &str) -> Result<BackgroundJobSnapshot, &'static str> {
        let terminal = {
            let mut jobs = self.jobs();
            let job = jobs.get_mut(job_id).ok_or("job_not_found")?;
            if job.snapshot.status.is_terminal() {
                return Ok(job.snapshot.clone());
            }
            let terminal = Arc::clone(&job.terminal);
            let _ = job.cancel_tx.send(true);
            if job.snapshot.status == BackgroundJobStatus::Queued {
                job.snapshot.status = BackgroundJobStatus::Cancelled;
                job.snapshot.finished_at = Some(Utc::now());
                let snapshot = job.snapshot.clone();
                terminal.notify_one();
                drop(jobs);
                self.emit_terminal(&snapshot);
                return Ok(snapshot);
            }
            terminal
        };

        loop {
            let notified = terminal.notified();
            if let Some(snapshot) = self.status(job_id)
                && snapshot.status.is_terminal()
            {
                return Ok(snapshot);
            }
            tokio::time::timeout(Duration::from_secs(10), notified)
                .await
                .map_err(|_| "cancel_timeout")?;
        }
    }

    async fn run_job(
        self: Arc<Self>,
        job_id: String,
        command: String,
        workspace: PathBuf,
        mut cancel_rx: watch::Receiver<bool>,
    ) {
        let permit = tokio::select! {
            permit = Arc::clone(&self.permits).acquire_owned() => match permit {
                Ok(permit) => permit,
                Err(_) => return,
            },
            _ = wait_for_cancel(&mut cancel_rx) => return,
        };

        {
            let mut jobs = self.jobs();
            let Some(job) = jobs.get_mut(&job_id) else {
                return;
            };
            if job.snapshot.status == BackgroundJobStatus::Cancelled {
                return;
            }
            job.snapshot.status = BackgroundJobStatus::Running;
            job.snapshot.started_at = Some(Utc::now());
        }

        #[cfg(unix)]
        let result = run_shell_command(
            &command,
            &workspace,
            self.limits.timeout,
            self.limits.max_output_bytes,
            Some(cancel_rx),
        )
        .await;
        #[cfg(not(unix))]
        let result: anyhow::Result<crate::shell::CommandOutput> =
            Err(anyhow::anyhow!("shell_not_supported"));

        drop(permit);

        let snapshot = {
            let mut jobs = self.jobs();
            let Some(job) = jobs.get_mut(&job_id) else {
                return;
            };
            match result {
                Ok(output) => {
                    job.snapshot.exit_code = output.exit_code;
                    job.snapshot.stdout = output.stdout;
                    job.snapshot.stderr = output.stderr;
                    match output.termination {
                        CommandTermination::Cancelled => {
                            job.snapshot.status = BackgroundJobStatus::Cancelled;
                        }
                        CommandTermination::TimedOut => {
                            job.snapshot.status = BackgroundJobStatus::Failed;
                            job.snapshot.error = Some("timed_out".into());
                        }
                        CommandTermination::Exited if output.exit_code == Some(0) => {
                            job.snapshot.status = BackgroundJobStatus::Succeeded;
                        }
                        CommandTermination::Exited => {
                            job.snapshot.status = BackgroundJobStatus::Failed;
                            job.snapshot.error = Some("non_zero_exit".into());
                        }
                    }
                }
                Err(error) => {
                    job.snapshot.status = BackgroundJobStatus::Failed;
                    job.snapshot.error = Some(error.to_string());
                }
            }
            job.snapshot.finished_at = Some(Utc::now());
            job.terminal.notify_one();
            job.snapshot.clone()
        };
        self.emit_terminal(&snapshot);
    }

    fn emit_terminal(&self, snapshot: &BackgroundJobSnapshot) {
        if let Some(sink) = &self.event_sink {
            sink(BackgroundJobEvent {
                job_id: snapshot.job_id.clone(),
                status: snapshot.status,
                exit_code: snapshot.exit_code,
                error: snapshot.error.clone(),
            });
        }
    }
}

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

fn success_json(snapshot: &BackgroundJobSnapshot) -> anyhow::Result<ToolResult> {
    Ok(ToolResult {
        success: true,
        output: serde_json::to_string(snapshot)?,
        error: None,
        presentation: None,
    })
}

fn failure(code: &str) -> ToolResult {
    ToolResult {
        success: false,
        output: String::new(),
        error: Some(code.to_string()),
        presentation: None,
    }
}

pub struct BackgroundRunTool {
    manager: Arc<BackgroundJobManager>,
}

impl BackgroundRunTool {
    pub fn new(manager: Arc<BackgroundJobManager>) -> Self {
        Self { manager }
    }
}

#[async_trait]
impl Tool for BackgroundRunTool {
    fn name(&self) -> &str {
        "background_run"
    }

    fn description(&self) -> &str {
        "Start a shell command in the workspace and return its background job ID immediately"
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "command": { "type": "string", "description": "The shell command to execute" }
            },
            "required": ["command"],
            "additionalProperties": false
        })
    }

    async fn execute(&self, args: Value, ctx: &dyn ToolContext) -> anyhow::Result<ToolResult> {
        let Some(command) = args.get("command").and_then(Value::as_str) else {
            return Ok(failure("command_required"));
        };
        match self
            .manager
            .start(command.to_string(), ctx.workspace_dir().to_path_buf())
        {
            Ok(snapshot) => success_json(&snapshot),
            Err(code) => Ok(failure(code)),
        }
    }
}

pub struct BackgroundStatusTool {
    manager: Arc<BackgroundJobManager>,
}

impl BackgroundStatusTool {
    pub fn new(manager: Arc<BackgroundJobManager>) -> Self {
        Self { manager }
    }
}

#[async_trait]
impl Tool for BackgroundStatusTool {
    fn name(&self) -> &str {
        "background_status"
    }

    fn description(&self) -> &str {
        "Get status, exit code, and bounded output for a background command"
    }

    fn parameters_schema(&self) -> Value {
        job_id_schema()
    }

    async fn execute(&self, args: Value, _ctx: &dyn ToolContext) -> anyhow::Result<ToolResult> {
        let Some(job_id) = args.get("job_id").and_then(Value::as_str) else {
            return Ok(failure("job_id_required"));
        };
        match self.manager.status(job_id) {
            Some(snapshot) => success_json(&snapshot),
            None => Ok(failure("job_not_found")),
        }
    }
}

pub struct BackgroundCancelTool {
    manager: Arc<BackgroundJobManager>,
}

impl BackgroundCancelTool {
    pub fn new(manager: Arc<BackgroundJobManager>) -> Self {
        Self { manager }
    }
}

#[async_trait]
impl Tool for BackgroundCancelTool {
    fn name(&self) -> &str {
        "background_cancel"
    }

    fn description(&self) -> &str {
        "Cancel a queued or running background command and wait for it to stop"
    }

    fn parameters_schema(&self) -> Value {
        job_id_schema()
    }

    async fn execute(&self, args: Value, _ctx: &dyn ToolContext) -> anyhow::Result<ToolResult> {
        let Some(job_id) = args.get("job_id").and_then(Value::as_str) else {
            return Ok(failure("job_id_required"));
        };
        match self.manager.cancel(job_id).await {
            Ok(snapshot) => success_json(&snapshot),
            Err(code) => Ok(failure(code)),
        }
    }
}

fn job_id_schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "job_id": { "type": "string", "description": "ID returned by background_run" }
        },
        "required": ["job_id"],
        "additionalProperties": false
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn wait_terminal(manager: &BackgroundJobManager, job_id: &str) -> BackgroundJobSnapshot {
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let snapshot = manager.status(job_id).unwrap();
                if snapshot.status.is_terminal() {
                    return snapshot;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap()
    }

    fn test_manager(
        max_running: usize,
        max_queued: usize,
        timeout: Duration,
        max_output_bytes: usize,
    ) -> Arc<BackgroundJobManager> {
        BackgroundJobManager::with_limits(
            BackgroundLimits {
                max_running,
                max_queued,
                timeout,
                max_output_bytes,
            },
            None,
        )
    }

    #[tokio::test]
    async fn succeeds_and_captures_output() {
        let workspace = tempfile::TempDir::new().unwrap();
        let manager = test_manager(1, 1, Duration::from_secs(2), 1024);
        let started = manager
            .start("printf hello".into(), workspace.path().into())
            .unwrap();
        let completed = wait_terminal(&manager, &started.job_id).await;
        assert_eq!(completed.status, BackgroundJobStatus::Succeeded);
        assert_eq!(completed.exit_code, Some(0));
        assert_eq!(completed.stdout, "hello");
    }

    #[tokio::test]
    async fn emits_one_terminal_event() {
        let workspace = tempfile::TempDir::new().unwrap();
        let (event_tx, mut event_rx) = tokio::sync::mpsc::unbounded_channel();
        let sink: BackgroundEventSink = Arc::new(move |event| {
            let _ = event_tx.send(event);
        });
        let manager = BackgroundJobManager::with_limits(
            BackgroundLimits {
                max_running: 1,
                max_queued: 1,
                timeout: Duration::from_secs(2),
                max_output_bytes: 1024,
            },
            Some(sink),
        );
        let started = manager
            .start("true".into(), workspace.path().into())
            .unwrap();

        let event = tokio::time::timeout(Duration::from_secs(2), event_rx.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(event.job_id, started.job_id);
        assert_eq!(event.status, BackgroundJobStatus::Succeeded);
        assert!(event_rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn reports_non_zero_exit() {
        let workspace = tempfile::TempDir::new().unwrap();
        let manager = test_manager(1, 1, Duration::from_secs(2), 1024);
        let started = manager
            .start("exit 7".into(), workspace.path().into())
            .unwrap();
        let completed = wait_terminal(&manager, &started.job_id).await;
        assert_eq!(completed.status, BackgroundJobStatus::Failed);
        assert_eq!(completed.exit_code, Some(7));
        assert_eq!(completed.error.as_deref(), Some("non_zero_exit"));
    }

    #[tokio::test]
    async fn times_out_and_truncates_output() {
        let workspace = tempfile::TempDir::new().unwrap();
        let manager = test_manager(1, 1, Duration::from_millis(100), 4);
        let started = manager
            .start("printf 123456; sleep 2".into(), workspace.path().into())
            .unwrap();
        let completed = wait_terminal(&manager, &started.job_id).await;
        assert_eq!(completed.status, BackgroundJobStatus::Failed);
        assert_eq!(completed.error.as_deref(), Some("timed_out"));
        assert_eq!(completed.stdout, "1234\n[Output truncated]");
    }

    #[tokio::test]
    async fn cancels_running_and_queued_jobs() {
        let workspace = tempfile::TempDir::new().unwrap();
        let manager = test_manager(1, 1, Duration::from_secs(5), 1024);
        let running = manager
            .start("sleep 5".into(), workspace.path().into())
            .unwrap();
        tokio::time::sleep(Duration::from_millis(50)).await;
        let queued = manager
            .start("printf never".into(), workspace.path().into())
            .unwrap();

        assert_eq!(
            manager.cancel(&queued.job_id).await.unwrap().status,
            BackgroundJobStatus::Cancelled
        );
        assert_eq!(
            manager.cancel(&running.job_id).await.unwrap().status,
            BackgroundJobStatus::Cancelled
        );
    }

    #[tokio::test]
    async fn enforces_queue_limit() {
        let workspace = tempfile::TempDir::new().unwrap();
        let manager = test_manager(1, 1, Duration::from_secs(5), 1024);
        manager
            .start("sleep 5".into(), workspace.path().into())
            .unwrap();
        manager
            .start("sleep 5".into(), workspace.path().into())
            .unwrap();
        assert_eq!(
            manager
                .start("true".into(), workspace.path().into())
                .unwrap_err(),
            "queue_full"
        );
    }
}
