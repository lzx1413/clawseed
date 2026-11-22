use crate::command::{CommandBackend, CommandOptions, run_workspace_command};
use crate::shell::CommandTermination;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use clawseed_api::tool::{Tool, ToolResult};
use clawseed_api::tool_context::ToolContext;
use serde::{Deserialize, Serialize};
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
const MAX_COMPLETED_JOBS: usize = 64;
const MAX_RETAINED_OUTPUT_BYTES: usize = 16 * 1024 * 1024;
const HISTORY_TTL_HOURS: i64 = 24;

mod store;
use store::JobStore;

pub type BackgroundEventSink = Arc<dyn Fn(BackgroundJobEvent) + Send + Sync>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackgroundJobSnapshot {
    pub job_id: String,
    #[serde(default)]
    pub backend: CommandBackend,
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
    pub backend: CommandBackend,
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
    store: Option<JobStore>,
    storage_error: bool,
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
            store: None,
            storage_error: false,
        })
    }

    /// Keep bounded results across gateway restarts. Never replay commands.
    pub fn persistent(directory: PathBuf, event_sink: Option<BackgroundEventSink>) -> Arc<Self> {
        let mut manager = Self::with_limits(BackgroundLimits::default(), event_sink);
        let inner = Arc::get_mut(&mut manager).expect("new manager is exclusively owned");
        match JobStore::open(directory) {
            Ok(store) => {
                for mut snapshot in store.load() {
                    if !snapshot.status.is_terminal() {
                        snapshot.status = BackgroundJobStatus::Failed;
                        snapshot.error = Some("interrupted_by_restart".into());
                        snapshot.finished_at = Some(Utc::now());
                        if let Err(error) = store.save(&snapshot) {
                            tracing::warn!(%error, "Could not persist interrupted background job");
                        }
                    }
                    inner.jobs.get_mut().unwrap().insert(
                        snapshot.job_id.clone(),
                        JobRecord {
                            snapshot,
                            cancel_tx: watch::channel(false).0,
                            terminal: Arc::new(Notify::new()),
                        },
                    );
                }
                inner.store = Some(store);
                inner.prune(&mut inner.jobs());
            }
            Err(error) => {
                tracing::error!(%error, "Background job storage unavailable; new jobs are disabled");
                inner.storage_error = true;
            }
        }
        manager
    }

    fn prune(&self, jobs: &mut HashMap<String, JobRecord>) {
        let cutoff = Utc::now() - chrono::Duration::hours(HISTORY_TTL_HOURS);
        let mut completed: Vec<_> = jobs
            .values()
            .filter(|job| job.snapshot.status.is_terminal())
            .map(|job| {
                (
                    job.snapshot.finished_at.unwrap_or(job.snapshot.created_at),
                    job.snapshot.job_id.clone(),
                )
            })
            .collect();
        completed.sort_unstable_by(|a, b| b.cmp(a));
        let mut bytes = 0usize;
        for (index, (finished, id)) in completed.into_iter().enumerate() {
            let snapshot = &jobs[&id].snapshot;
            bytes += snapshot.stdout.len() + snapshot.stderr.len();
            if index >= MAX_COMPLETED_JOBS || finished < cutoff || bytes > MAX_RETAINED_OUTPUT_BYTES
            {
                jobs.remove(&id);
                if let Some(store) = &self.store {
                    store.remove(&id);
                }
            }
        }
    }

    fn save(&self, snapshot: &BackgroundJobSnapshot) -> std::io::Result<()> {
        if let Some(store) = &self.store {
            store.save(snapshot)?;
        }
        Ok(())
    }

    /// Metadata only: polling does not copy captured command output.
    pub fn summary(&self) -> Value {
        let mut jobs = self.jobs();
        self.prune(&mut jobs);
        let active_count = jobs
            .values()
            .filter(|job| !job.snapshot.status.is_terminal())
            .count();
        let mut entries: Vec<_> = jobs.values().map(|job| {
            let s = &job.snapshot;
            serde_json::json!({"job_id":s.job_id, "backend":s.backend, "status":s.status,
                "created_at":s.created_at,"finished_at":s.finished_at,"exit_code":s.exit_code,"error":s.error})
        }).collect();
        entries.sort_unstable_by(|a, b| b["created_at"].as_str().cmp(&a["created_at"].as_str()));
        serde_json::json!({"active_count":active_count,"jobs":entries,"storage_available":!self.storage_error})
    }

    fn jobs(&self) -> MutexGuard<'_, HashMap<String, JobRecord>> {
        self.jobs.lock().unwrap_or_else(|error| error.into_inner())
    }

    pub fn start(
        self: &Arc<Self>,
        command: String,
        workspace: PathBuf,
    ) -> Result<BackgroundJobSnapshot, &'static str> {
        self.start_with_options(
            command,
            workspace,
            CommandOptions {
                backend: CommandBackend::Host,
                timeout: self.limits.timeout,
                max_output_bytes: self.limits.max_output_bytes,
            },
        )
    }

    pub fn start_with_options(
        self: &Arc<Self>,
        command: String,
        workspace: PathBuf,
        options: CommandOptions,
    ) -> Result<BackgroundJobSnapshot, &'static str> {
        self.start_with_runtime(command, workspace, options, None)
    }

    fn start_with_runtime(
        self: &Arc<Self>,
        command: String,
        workspace: PathBuf,
        options: CommandOptions,
        runtime_dir: Option<PathBuf>,
    ) -> Result<BackgroundJobSnapshot, &'static str> {
        if command.trim().is_empty() {
            return Err("command_required");
        }
        if self.storage_error {
            return Err("background_storage_unavailable");
        }

        let (cancel_tx, cancel_rx) = watch::channel(false);
        let terminal = Arc::new(Notify::new());
        let job_id = Uuid::new_v4().to_string();
        let snapshot = BackgroundJobSnapshot {
            job_id: job_id.clone(),
            backend: options.backend,
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
            self.prune(&mut jobs);
            let active = jobs
                .values()
                .filter(|job| !job.snapshot.status.is_terminal())
                .count();
            if active >= self.limits.max_running + self.limits.max_queued {
                return Err("queue_full");
            }
            self.save(&snapshot).map_err(|error| {
                tracing::error!(%error, "Could not persist new background job");
                "background_storage_unavailable"
            })?;
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
            manager
                .run_job(job_id, command, workspace, options, cancel_rx, runtime_dir)
                .await;
        });
        Ok(snapshot)
    }

    pub fn status(&self, job_id: &str) -> Option<BackgroundJobSnapshot> {
        let mut jobs = self.jobs();
        self.prune(&mut jobs);
        jobs.get(job_id).map(|job| job.snapshot.clone())
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
                if let Err(error) = self.save(&snapshot) {
                    tracing::warn!(%error, "Could not persist cancelled background job");
                }
                self.prune(&mut jobs);
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
        options: CommandOptions,
        mut cancel_rx: watch::Receiver<bool>,
        runtime_dir: Option<PathBuf>,
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
            if let Err(error) = self.save(&job.snapshot) {
                tracing::warn!(%error, "Could not persist running background job");
            }
        }

        let result =
            run_workspace_command(&command, &workspace, options, Some(cancel_rx), runtime_dir)
                .await;

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
            if let Err(error) = self.save(&job.snapshot) {
                tracing::error!(%error, "Could not persist completed background job");
                job.snapshot.error = Some("result_persistence_failed".into());
            }
            job.terminal.notify_one();
            let snapshot = job.snapshot.clone();
            self.prune(&mut jobs);
            snapshot
        };
        self.emit_terminal(&snapshot);
    }

    fn emit_terminal(&self, snapshot: &BackgroundJobSnapshot) {
        if let Some(sink) = &self.event_sink {
            sink(BackgroundJobEvent {
                job_id: snapshot.job_id.clone(),
                backend: snapshot.backend,
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
        "Start a workspace command on host (Android/native shell) or linux (PRoot) and return its job ID immediately; defaults to host. Use background_status/background_cancel to manage it."
    }

    fn parameters_schema(&self) -> Value {
        crate::command::command_schema(crate::command::MAX_BACKGROUND_TIMEOUT_SECS)
    }

    async fn execute(&self, args: Value, ctx: &dyn ToolContext) -> anyhow::Result<ToolResult> {
        let Some(command) = args.get("command").and_then(Value::as_str) else {
            return Ok(failure("command_required"));
        };
        let options = match CommandOptions::from_args(
            &args,
            CommandBackend::Host,
            DEFAULT_TIMEOUT_SECS,
            crate::command::MAX_BACKGROUND_TIMEOUT_SECS,
        ) {
            Ok(options) => options,
            Err(error) => return Ok(failure(error)),
        };
        match self.manager.start_with_options(
            command.to_string(),
            ctx.workspace_dir().to_path_buf(),
            options,
        ) {
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
        "Get status, exit code, and bounded output by job_id, or omit job_id to list recent jobs and active_count. Completed results are retained for up to 24 hours, 64 jobs, and 16 MiB total output; interrupted jobs are not replayed after restart."
    }

    fn parameters_schema(&self) -> Value {
        let mut schema = job_id_schema();
        schema["required"] = serde_json::json!([]);
        schema
    }

    async fn execute(&self, args: Value, _ctx: &dyn ToolContext) -> anyhow::Result<ToolResult> {
        if args.get("job_id").is_none() {
            return Ok(ToolResult {
                success: true,
                output: self.manager.summary().to_string(),
                error: None,
                presentation: None,
            });
        }
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

    fn snapshot(status: BackgroundJobStatus) -> BackgroundJobSnapshot {
        BackgroundJobSnapshot {
            job_id: Uuid::new_v4().to_string(),
            backend: CommandBackend::Linux,
            status,
            created_at: Utc::now(),
            started_at: None,
            finished_at: status.is_terminal().then(Utc::now),
            exit_code: None,
            stdout: "saved output".into(),
            stderr: String::new(),
            error: None,
        }
    }

    #[test]
    fn restores_results_marks_interrupted_jobs_and_excludes_other_owners() {
        let directory = tempfile::tempdir().unwrap();
        let finished = snapshot(BackgroundJobStatus::Succeeded);
        let interrupted = snapshot(BackgroundJobStatus::Running);
        {
            let store = JobStore::open(directory.path().into()).unwrap();
            store.save(&finished).unwrap();
            store.save(&interrupted).unwrap();
            std::fs::write(directory.path().join("damaged.json"), b"broken").unwrap();
        }
        let manager = BackgroundJobManager::persistent(directory.path().into(), None);
        assert_eq!(
            manager.status(&finished.job_id).unwrap().stdout,
            "saved output"
        );
        let recovered = manager.status(&interrupted.job_id).unwrap();
        assert_eq!(recovered.status, BackgroundJobStatus::Failed);
        assert_eq!(recovered.error.as_deref(), Some("interrupted_by_restart"));
        assert_eq!(manager.summary()["active_count"], 0);
        assert!(!manager.summary().to_string().contains("saved output"));
        let second = BackgroundJobManager::persistent(directory.path().into(), None);
        assert!(second.storage_error);
        assert_eq!(
            second
                .start("touch must-not-run".into(), directory.path().into())
                .unwrap_err(),
            "background_storage_unavailable"
        );
        drop(manager);
        let reopened = BackgroundJobManager::persistent(directory.path().into(), None);
        assert!(!reopened.storage_error);
        assert_eq!(
            reopened
                .status(&interrupted.job_id)
                .unwrap()
                .error
                .as_deref(),
            Some("interrupted_by_restart")
        );
    }

    #[test]
    fn prunes_expired_and_excessive_history_without_removing_active_jobs() {
        let directory = tempfile::tempdir().unwrap();
        let manager = BackgroundJobManager::persistent(directory.path().into(), None);
        let active = snapshot(BackgroundJobStatus::Running);
        let mut expired = snapshot(BackgroundJobStatus::Succeeded);
        expired.finished_at = Some(Utc::now() - chrono::Duration::hours(25));
        let expired_id = expired.job_id.clone();
        {
            let mut jobs = manager.jobs();
            for snapshot in std::iter::once(active.clone())
                .chain(std::iter::once(expired))
                .chain(
                    (0..MAX_COMPLETED_JOBS + 5).map(|_| snapshot(BackgroundJobStatus::Succeeded)),
                )
            {
                manager.save(&snapshot).unwrap();
                jobs.insert(
                    snapshot.job_id.clone(),
                    JobRecord {
                        snapshot,
                        cancel_tx: watch::channel(false).0,
                        terminal: Arc::new(Notify::new()),
                    },
                );
            }
        }
        assert_eq!(manager.summary()["active_count"], 1);
        assert_eq!(manager.jobs().len(), MAX_COMPLETED_JOBS + 1);
        assert!(manager.status(&active.job_id).is_some());
        assert!(manager.status(&expired_id).is_none());
        assert!(!directory.path().join(format!("{expired_id}.json")).exists());
        let mut jobs = manager.jobs();
        for job in jobs
            .values_mut()
            .filter(|job| job.snapshot.status.is_terminal())
        {
            job.snapshot.stdout = "x".repeat(1024 * 1024);
        }
        manager.prune(&mut jobs);
        assert_eq!(jobs.len(), 17);
    }

    #[tokio::test]
    async fn persists_actual_command_results_and_optional_status_lists_jobs() {
        let workspace = tempfile::tempdir().unwrap();
        let store = tempfile::tempdir().unwrap();
        let manager = BackgroundJobManager::persistent(store.path().into(), None);
        let job = manager
            .start("printf durable".into(), workspace.path().into())
            .unwrap();
        let completed = wait_terminal(&manager, &job.job_id).await;
        assert_eq!(completed.stdout, "durable");
        let disk: BackgroundJobSnapshot = serde_json::from_slice(
            &std::fs::read(store.path().join(format!("{}.json", job.job_id))).unwrap(),
        )
        .unwrap();
        assert_eq!(disk.status, BackgroundJobStatus::Succeeded);
        assert_eq!(disk.stdout, "durable");
        let tool = BackgroundStatusTool::new(manager);
        struct Context(PathBuf);
        impl ToolContext for Context {
            fn workspace_dir(&self) -> &std::path::Path {
                &self.0
            }
        }
        let result = tool
            .execute(serde_json::json!({}), &Context(workspace.path().into()))
            .await
            .unwrap();
        let summary: Value = serde_json::from_str(&result.output).unwrap();
        assert_eq!(summary["active_count"], 0);
        assert_eq!(summary["jobs"][0]["job_id"], job.job_id);
    }

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
    #[cfg(unix)]
    #[tokio::test]
    async fn linux_jobs_use_shared_runner_and_report_backend_in_events() {
        let (runtime, context) = crate::linux_exec::tests::fake_runtime();
        let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();
        let sink: BackgroundEventSink = Arc::new(move |event| {
            let _ = sender.send(event);
        });
        let manager = BackgroundJobManager::new(Some(sink));
        let started = manager
            .start_with_runtime(
                "printf out; printf err >&2; exit 7".into(),
                context.workspace_dir().into(),
                CommandOptions {
                    backend: CommandBackend::Linux,
                    timeout: Duration::from_secs(2),
                    max_output_bytes: 1024,
                },
                Some(runtime.path().into()),
            )
            .unwrap();
        assert_eq!(started.backend, CommandBackend::Linux);
        let completed = wait_terminal(&manager, &started.job_id).await;
        assert_eq!(completed.status, BackgroundJobStatus::Failed);
        assert_eq!(completed.exit_code, Some(7));
        assert_eq!(completed.stdout, "out");
        assert_eq!(completed.stderr, "err");
        assert!(runtime.path().join("launched").exists());
        let event = tokio::time::timeout(Duration::from_secs(2), receiver.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(event.backend, CommandBackend::Linux);
        assert_eq!(event.job_id, started.job_id);
        assert!(receiver.try_recv().is_err());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn linux_jobs_can_be_cancelled_while_running_or_queued() {
        let (runtime, context) = crate::linux_exec::tests::fake_runtime();
        let manager = test_manager(1, 2, Duration::from_secs(30), 1024);
        let options = CommandOptions {
            backend: CommandBackend::Linux,
            timeout: Duration::from_secs(30),
            max_output_bytes: 1024,
        };
        let running = manager
            .start_with_runtime(
                "sleep 30".into(),
                context.workspace_dir().into(),
                options,
                Some(runtime.path().into()),
            )
            .unwrap();
        tokio::time::timeout(Duration::from_secs(5), async {
            while !runtime.path().join("launched").exists() {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        let queued = manager
            .start_with_runtime(
                "touch queued-should-not-run".into(),
                context.workspace_dir().into(),
                options,
                Some(runtime.path().into()),
            )
            .unwrap();
        assert_eq!(
            manager.cancel(&queued.job_id).await.unwrap().status,
            BackgroundJobStatus::Cancelled
        );
        assert_eq!(
            manager.cancel(&running.job_id).await.unwrap().status,
            BackgroundJobStatus::Cancelled
        );
        assert!(
            !context
                .workspace_dir()
                .join("queued-should-not-run")
                .exists()
        );
        assert!(
            runtime
                .path()
                .join("tasks")
                .read_dir()
                .unwrap()
                .next()
                .is_none()
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn linux_timeout_captures_bounded_output() {
        let (runtime, context) = crate::linux_exec::tests::fake_runtime();
        let manager = test_manager(1, 1, Duration::from_secs(5), 1024);
        let options = CommandOptions {
            backend: CommandBackend::Linux,
            timeout: Duration::from_secs(5),
            max_output_bytes: 4,
        };
        let started = manager
            .start_with_runtime(
                "printf 123456".into(),
                context.workspace_dir().into(),
                options,
                Some(runtime.path().into()),
            )
            .unwrap();
        let completed = wait_terminal(&manager, &started.job_id).await;
        assert_eq!(completed.status, BackgroundJobStatus::Succeeded);
        assert_eq!(completed.stdout, "1234\n[Output truncated]");
        let started = manager
            .start_with_runtime(
                "sleep 30".into(),
                context.workspace_dir().into(),
                CommandOptions {
                    timeout: Duration::from_millis(50),
                    ..options
                },
                Some(runtime.path().into()),
            )
            .unwrap();
        let completed = wait_terminal(&manager, &started.job_id).await;
        assert_eq!(completed.status, BackgroundJobStatus::Failed);
        assert_eq!(completed.error.as_deref(), Some("timed_out"));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn rejects_invalid_backend_before_queuing() {
        let (_, context) = crate::linux_exec::tests::fake_runtime();
        let manager = BackgroundJobManager::new(None);
        let tool = BackgroundRunTool::new(manager.clone());
        let result = tool
            .execute(
                serde_json::json!({"command":"true", "backend":"unknown"}),
                &context,
            )
            .await
            .unwrap();
        assert!(!result.success);
        assert!(manager.jobs().is_empty());
    }
}
