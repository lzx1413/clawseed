//! Linux user-space execution through an installed PRoot runtime.

use anyhow::Context;
use async_trait::async_trait;
use clawseed_api::tool::{Tool, ToolResult};
use clawseed_api::tool_context::ToolContext;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use tokio::sync::Semaphore;
use tokio::time::timeout;

const RUNTIME_DIR_ENV: &str = "CLAWSEED_PROOT_RUNTIME_DIR";
const DEFAULT_TIMEOUT_SECS: u64 = 60;
const MAX_CONCURRENT_LINUX_EXEC: usize = 2;
const MAX_CONCURRENT_LINUX_PLUGINS: usize = 16;
const PLUGIN_RUNTIME_ACQUIRE_TIMEOUT: Duration = Duration::from_secs(2);

fn linux_exec_limit() -> Arc<Semaphore> {
    static LIMIT: OnceLock<Arc<Semaphore>> = OnceLock::new();
    LIMIT
        .get_or_init(|| Arc::new(Semaphore::new(MAX_CONCURRENT_LINUX_EXEC)))
        .clone()
}

fn linux_plugin_limit() -> Arc<Semaphore> {
    static LIMIT: OnceLock<Arc<Semaphore>> = OnceLock::new();
    LIMIT
        .get_or_init(|| Arc::new(Semaphore::new(MAX_CONCURRENT_LINUX_PLUGINS)))
        .clone()
}

/// Resources that must stay alive while a persistent plugin runs in PRoot.
#[cfg(unix)]
pub(crate) struct LinuxPluginGuard {
    _permit: tokio::sync::OwnedSemaphorePermit,
    _lease: Option<usage::Lease>,
    _task_tmp: tempfile::TempDir,
}

pub(crate) struct LinuxPluginPermissions<'a> {
    pub workspace: &'a str,
    pub network: &'a str,
    pub environment: &'a [String],
}

/// Spawn a long-lived Python or Node.js process inside the active PRoot runtime.
#[cfg(unix)]
pub(crate) async fn spawn_linux_plugin(
    language: &str,
    entry: &Path,
    plugin_root: &Path,
    workspace: &Path,
    runtime_dir: Option<PathBuf>,
    plugin_id: &str,
    permissions: &LinuxPluginPermissions<'_>,
) -> anyhow::Result<(tokio::process::Child, LinuxPluginGuard)> {
    anyhow::ensure!(
        workspace.is_dir(),
        "workspace is not a directory: {}",
        workspace.display()
    );
    let permit = timeout(
        PLUGIN_RUNTIME_ACQUIRE_TIMEOUT,
        linux_plugin_limit().acquire_owned(),
    )
    .await
    .map_err(|_| {
        anyhow::anyhow!(
            "timed out waiting for Linux runtime capacity; stop another Linux task or plugin and retry"
        )
    })??;
    let store = runtime_dir
        .clone()
        .or_else(|| std::env::var_os(RUNTIME_DIR_ENV).map(PathBuf::from));
    let lease = store
        .as_ref()
        .filter(|path| path.is_dir())
        .map(|path| usage::Lease::acquire(path))
        .transpose()
        .map_err(|e| anyhow::anyhow!("runtime is being removed or cannot be locked: {e}"))?;
    let layout = RuntimeLayout::load(runtime_dir).map_err(anyhow::Error::msg)?;
    let task_dir = layout.root.join("tasks");
    std::fs::create_dir_all(&task_dir)?;
    let task_tmp = tempfile::Builder::new()
        .prefix("clawseed-plugin-")
        .tempdir_in(&task_dir)?;
    let proot_tmp = task_tmp.path().join("proot-tmp");
    std::fs::create_dir(&proot_tmp)?;
    let workspace_mount = if permissions.workspace == "read" {
        let snapshot = task_tmp.path().join("workspace");
        copy_workspace_snapshot(workspace, &snapshot)?;
        snapshot
    } else {
        workspace.to_path_buf()
    };
    let proot_l2s = layout.rootfs.join("root/.l2s");
    std::fs::create_dir_all(&proot_l2s)?;

    let plugin_root = plugin_root.canonicalize()?;
    let entry = entry.canonicalize()?;
    let relative_entry = entry
        .strip_prefix(&plugin_root)
        .map_err(|_| anyhow::anyhow!("plugin entry is outside its plugin directory"))?;
    let guest_entry = Path::new("/plugin").join(relative_entry);
    let interpreter = match language {
        // The signed runtime installs Python under /opt/python. This path is
        // inside the guest rootfs and is resolved after PRoot starts.
        "python" => "/opt/python/bin/python3",
        "node" => "/opt/node/bin/node",
        other => anyhow::bail!("unsupported plugin language: {other}"),
    };

    let mut cmd = tokio::process::Command::new(&layout.proot);
    cmd.args(["--kill-on-exit", "--link2symlink", "--sysvipc", "-0", "-r"])
        .arg(&layout.rootfs)
        .args(["-b"])
        .arg(format!("{}:/plugin", plugin_root.display()))
        .args(["-b"])
        .arg(format!("{}:/tmp", task_tmp.path().display()))
        .args(["-b", "/dev", "-b", "/proc"]);
    if permissions.workspace != "none" {
        cmd.args(["-b"])
            .arg(format!("{}:/workspace", workspace_mount.display()));
    }
    #[cfg(target_os = "android")]
    cmd.args(["-b", "/system", "-b", "/apex"]);
    cmd.args([
        "-w",
        "/plugin",
        "/usr/bin/env",
        "-i",
        "HOME=/root",
        "USER=root",
        "LOGNAME=root",
        "SHELL=/bin/sh",
        "PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin",
        "LANG=C.UTF-8",
        "SSL_CERT_FILE=/etc/ssl/certs/ca-certificates.crt",
        "TMPDIR=/tmp",
        "TERM=dumb",
        interpreter,
    ])
    .arg(guest_entry)
    .current_dir(plugin_root)
    .env_clear()
    .env("HOME", "/root")
    .env(
        "PATH",
        "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin",
    )
    .env("LANG", "C.UTF-8")
    .env("TMPDIR", &proot_tmp)
    .env("TERM", "dumb")
    .env("CLAWSEED_PLUGIN_ID", plugin_id)
    .env("CLAWSEED_CETP_VERSION", "2")
    .env("CLAWSEED_PLUGIN_NETWORK", permissions.network)
    .env(
        "CLAWSEED_PLUGIN_WORKSPACE",
        if permissions.workspace == "none" {
            ""
        } else {
            "/workspace"
        },
    )
    .stdin(std::process::Stdio::piped())
    .stdout(std::process::Stdio::piped())
    .stderr(std::process::Stdio::piped())
    .kill_on_drop(true);
    if let Some(loader) = &layout.loader {
        cmd.env("PROOT_LOADER", loader);
    }
    if let Some(library_dir) = &layout.library_dir {
        cmd.env("LD_LIBRARY_PATH", library_dir);
    }
    for name in permissions.environment {
        if let Some(value) = std::env::var_os(name) {
            cmd.env(name, value);
        }
    }
    cmd.env("PROOT_TMP_DIR", &proot_tmp)
        .env("PROOT_L2S_DIR", &proot_l2s);
    let child = cmd
        .spawn()
        .with_context(|| format!("failed to start Linux plugin {plugin_id}"))?;
    Ok((
        child,
        LinuxPluginGuard {
            _permit: permit,
            _lease: lease,
            _task_tmp: task_tmp,
        },
    ))
}

fn copy_workspace_snapshot(source: &Path, destination: &Path) -> anyhow::Result<()> {
    std::fs::create_dir_all(destination)?;
    for entry in std::fs::read_dir(source)? {
        let entry = entry?;
        let source_path = entry.path();
        let destination_path = destination.join(entry.file_name());
        let metadata = std::fs::symlink_metadata(&source_path)?;
        let file_type = metadata.file_type();
        if file_type.is_symlink() {
            continue;
        }
        if file_type.is_dir() {
            copy_workspace_snapshot(&source_path, &destination_path)?;
        } else if file_type.is_file() {
            std::fs::copy(&source_path, &destination_path)?;
        }
    }
    Ok(())
}

// FileChannel.lock() on Android uses POSIX record locks. Keep one descriptor
// per store open in this process: closing any descriptor for that inode would
// release all process-owned record locks, including other concurrent tasks.
#[cfg(unix)]
mod usage {
    use parking_lot::Mutex;
    use std::collections::HashMap;
    use std::fs::{File, OpenOptions};
    use std::os::fd::AsRawFd;
    use std::path::{Path, PathBuf};
    use std::sync::{Arc, OnceLock};

    struct State {
        file: File,
        users: usize,
    }
    pub(super) struct Lease(Arc<Mutex<State>>);

    fn record_lock(file: &File, kind: libc::c_short) -> std::io::Result<()> {
        // SAFETY: a zeroed flock is valid; its whence/type are initialized below.
        let mut lock: libc::flock = unsafe { std::mem::zeroed() };
        lock.l_type = kind;
        lock.l_whence = libc::SEEK_SET as libc::c_short;
        // SAFETY: the descriptor is live and F_SETLK takes a pointer to flock.
        if unsafe { libc::fcntl(file.as_raw_fd(), libc::F_SETLK, &lock) } == -1 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(())
    }

    impl Lease {
        pub(super) fn acquire(store: &Path) -> std::io::Result<Self> {
            type StoreLocks = Mutex<HashMap<PathBuf, Arc<Mutex<State>>>>;
            static STORES: OnceLock<StoreLocks> = OnceLock::new();
            let path = store.canonicalize()?;
            let mut stores = STORES.get_or_init(Mutex::default).lock();
            let state = if let Some(state) = stores.get(&path) {
                state.clone()
            } else {
                let file = OpenOptions::new()
                    .read(true)
                    .write(true)
                    .create(true)
                    .truncate(false)
                    .open(path.join("usage.lock"))?;
                let state = Arc::new(Mutex::new(State { file, users: 0 }));
                stores.insert(path, state.clone());
                state
            };
            let mut current = state.lock();
            if current.users == 0 {
                record_lock(&current.file, libc::F_RDLCK as libc::c_short)?;
            }
            current.users += 1;
            drop(current);
            Ok(Self(state))
        }
    }

    impl Drop for Lease {
        fn drop(&mut self) {
            let mut state = self.0.lock();
            state.users -= 1;
            if state.users == 0 {
                let _ = record_lock(&state.file, libc::F_UNLCK as libc::c_short);
            }
        }
    }
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
        let mut root = runtime_dir
            .or_else(|| std::env::var_os(RUNTIME_DIR_ENV).map(PathBuf::from))
            .ok_or_else(|| {
                format!(
                    "PRoot runtime is not configured; set {RUNTIME_DIR_ENV} after installing it"
                )
            })?;
        // Android supplies the runtime store, so activation takes effect without
        // restarting the gateway. Explicit generation paths remain supported.
        let active = root.join("active.json");
        if active.is_file() {
            let pointer: Value = serde_json::from_slice(
                &std::fs::read(&active)
                    .map_err(|e| format!("could not read active runtime: {e}"))?,
            )
            .map_err(|e| format!("invalid active runtime: {e}"))?;
            let generation = pointer
                .get("generation")
                .and_then(Value::as_str)
                .filter(|name| {
                    !name.is_empty()
                        && *name != "."
                        && *name != ".."
                        && name
                            .bytes()
                            .all(|b| b.is_ascii_alphanumeric() || b"._+-".contains(&b))
                })
                .ok_or("invalid runtime generation")?;
            root = root.join("versions").join(generation);
        }
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
        let health = root.join("health.json");
        if health.is_file() {
            let health: Value =
                serde_json::from_slice(&std::fs::read(&health).map_err(|e| e.to_string())?)
                    .map_err(|e| format!("invalid runtime health record: {e}"))?;
            if health.get("ok").and_then(Value::as_bool) != Some(true) {
                return Err(
                    "Linux runtime health check failed; check or repair it in settings".into(),
                );
            }
        }
        let manifest = root.join("manifest.json");
        if manifest.is_file() {
            let metadata: Value =
                serde_json::from_slice(&std::fs::read(&manifest).map_err(|e| e.to_string())?)
                    .map_err(|e| format!("invalid runtime manifest: {e}"))?;
            if metadata.get("components").is_some() {
                for component in [
                    "usr/bin/env",
                    "bin/sh",
                    "opt/node/bin/node",
                    "opt/python/bin/python3",
                ] {
                    if !is_executable(&rootfs.join(component)) {
                        return Err(format!(
                            "Linux runtime component is missing: {component}; repair it in settings"
                        ));
                    }
                }
            }
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
                let components = value.get("components").cloned().unwrap_or(Value::Null);
                format!(", version={version}, runtime_id={runtime_id}, components={components}")
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
        "Compatibility alias for shell with backend=linux; prefer shell for new calls"
    }

    fn parameters_schema(&self) -> Value {
        let mut schema = crate::command::command_schema(crate::command::MAX_SYNC_TIMEOUT_SECS);
        schema["properties"]
            .as_object_mut()
            .unwrap()
            .remove("backend");
        schema
    }

    async fn execute(&self, mut args: Value, ctx: &dyn ToolContext) -> anyhow::Result<ToolResult> {
        if !args.is_object() {
            return Ok(crate::command::failure("command_required"));
        }
        if args.get("backend").is_some_and(|value| value != "linux") {
            return Ok(crate::command::failure(
                "linux_exec always uses the linux backend",
            ));
        }
        args["backend"] = Value::String("linux".into());
        crate::command::execute_sync(
            args,
            ctx,
            crate::command::CommandBackend::Linux,
            DEFAULT_TIMEOUT_SECS,
            self.runtime_dir.clone(),
        )
        .await
    }
}

/// Shared Linux backend for shell, background jobs, and the legacy alias.
#[cfg(unix)]
pub(crate) async fn run_linux_command(
    command: &str,
    workspace: &Path,
    timeout: Duration,
    max_output_bytes: usize,
    mut cancel_rx: Option<tokio::sync::watch::Receiver<bool>>,
    runtime_dir: Option<PathBuf>,
) -> anyhow::Result<crate::shell::CommandOutput> {
    let deadline = tokio::time::Instant::now() + timeout;
    let stopped = |termination| crate::shell::CommandOutput {
        termination,
        exit_code: None,
        stdout: String::new(),
        stderr: String::new(),
    };
    let acquire = linux_exec_limit().acquire_owned();
    let _permit = if let Some(cancel) = cancel_rx.as_mut() {
        tokio::select! {
            biased;
            _ = crate::shell::wait_for_cancel(cancel) => return Ok(stopped(crate::shell::CommandTermination::Cancelled)),
            _ = tokio::time::sleep_until(deadline) => return Ok(stopped(crate::shell::CommandTermination::TimedOut)),
            permit = acquire => permit?,
        }
    } else {
        tokio::select! {
            biased;
            _ = tokio::time::sleep_until(deadline) => return Ok(stopped(crate::shell::CommandTermination::TimedOut)),
            permit = acquire => permit?,
        }
    };
    let store = runtime_dir
        .clone()
        .or_else(|| std::env::var_os(RUNTIME_DIR_ENV).map(PathBuf::from));
    let _lease = store
        .as_ref()
        .filter(|path| path.is_dir())
        .map(|path| usage::Lease::acquire(path))
        .transpose()
        .map_err(|e| anyhow::anyhow!("runtime is being removed or cannot be locked: {e}"))?;
    let layout = RuntimeLayout::load(runtime_dir).map_err(anyhow::Error::msg)?;

    // The Android global temp directory is not writable by apps.
    let task_dir = layout.root.join("tasks");
    std::fs::create_dir_all(&task_dir)?;
    let task_tmp = tempfile::Builder::new()
        .prefix("clawseed-linux-")
        .tempdir_in(&task_dir)
        .map_err(|error| anyhow::anyhow!("could not create task temp directory: {error}"))?;
    let proot_tmp = task_tmp.path().join("proot-tmp");
    // link2symlink backing files must survive individual commands.
    let proot_l2s = layout.rootfs.join("root/.l2s");
    std::fs::create_dir(&proot_tmp)?;
    std::fs::create_dir_all(&proot_l2s)?;

    let mut cmd = tokio::process::Command::new(&layout.proot);
    cmd.args(["--kill-on-exit", "--link2symlink", "--sysvipc", "-0", "-r"])
        .arg(&layout.rootfs)
        .args(["-b"])
        .arg(format!("{}:/workspace", workspace.display()))
        .args(["-b"])
        .arg(format!("{}:/tmp", task_tmp.path().display()))
        .args(["-b", "/dev", "-b", "/proc"]);
    #[cfg(target_os = "android")]
    cmd.args(["-b", "/system", "-b", "/apex"]);
    cmd.args([
        "-w",
        "/workspace",
        "/usr/bin/env",
        "-i",
        "HOME=/root",
        "USER=root",
        "LOGNAME=root",
        "SHELL=/bin/sh",
        "PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin",
        "LANG=C.UTF-8",
        "SSL_CERT_FILE=/etc/ssl/certs/ca-certificates.crt",
        "TMPDIR=/tmp",
        "TERM=dumb",
        "/bin/sh",
        "-c",
    ])
    .arg(command)
    .current_dir(workspace)
    .env_clear()
    .env("HOME", "/root")
    .env(
        "PATH",
        "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin",
    )
    .env("LANG", "C.UTF-8")
    .env("TMPDIR", &proot_tmp)
    .env("TERM", "dumb");
    if let Some(loader) = &layout.loader {
        cmd.env("PROOT_LOADER", loader);
    }
    if let Some(library_dir) = &layout.library_dir {
        cmd.env("LD_LIBRARY_PATH", library_dir);
    }
    cmd.env("PROOT_TMP_DIR", &proot_tmp)
        .env("PROOT_L2S_DIR", &proot_l2s);

    let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
    if remaining.is_zero() {
        return Ok(stopped(crate::shell::CommandTermination::TimedOut));
    }
    let result =
        crate::shell::run_proot_command(cmd, remaining, max_output_bytes, cancel_rx).await?;
    Ok(result)
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

#[cfg(all(test, unix))]
pub(crate) mod tests {
    use super::*;
    use clawseed_api::tool_context::ToolContext;
    use std::os::unix::fs::PermissionsExt;
    use tempfile::TempDir;

    pub(crate) struct Context {
        workspace: PathBuf,
    }

    impl ToolContext for Context {
        fn workspace_dir(&self) -> &Path {
            &self.workspace
        }
    }

    pub(crate) fn fake_runtime() -> (TempDir, Context) {
        let runtime = tempfile::tempdir().unwrap();
        let rootfs = runtime.path().join("rootfs");
        std::fs::create_dir(&rootfs).unwrap();
        let proot = runtime.path().join("proot");
        std::fs::write(&proot, b"#!/bin/sh\nprintf linux > \"${0%/*}/launched\"\nwhile [ \"$1\" != \"/bin/sh\" ]; do shift; done\nshift\nshift\nexec /bin/sh -c \"$1\"\n").unwrap();
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

    #[test]
    fn failed_health_and_missing_interpreters_are_not_reported_ready() {
        let (runtime, _) = fake_runtime();
        std::fs::write(runtime.path().join("health.json"), br#"{"ok":false}"#).unwrap();
        assert!(
            RuntimeLayout::load(Some(runtime.path().into()))
                .unwrap_err()
                .contains("health check failed")
        );
        std::fs::write(runtime.path().join("health.json"), br#"{"ok":true}"#).unwrap();
        assert!(RuntimeLayout::load(Some(runtime.path().into())).is_ok());
        std::fs::write(
            runtime.path().join("manifest.json"),
            br#"{"components":{"node":"test"}}"#,
        )
        .unwrap();
        assert!(
            RuntimeLayout::load(Some(runtime.path().into()))
                .unwrap_err()
                .contains("component is missing")
        );
    }

    #[test]
    fn external_removal_lock_probe() {
        use std::os::fd::AsRawFd;
        let Some(path) = std::env::var_os("CLAWSEED_TEST_USAGE_PATH") else {
            return;
        };
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(path)
            .unwrap();
        // SAFETY: initialized flock passed to F_SETLK on a live descriptor.
        let mut lock: libc::flock = unsafe { std::mem::zeroed() };
        lock.l_type = libc::F_WRLCK as libc::c_short;
        lock.l_whence = libc::SEEK_SET as libc::c_short;
        let result = unsafe { libc::fcntl(file.as_raw_fd(), libc::F_SETLK, &lock) };
        assert_eq!(
            result == 0,
            std::env::var("CLAWSEED_TEST_USAGE_FREE").unwrap() == "true"
        );
    }

    #[test]
    fn removal_lock_waits_for_every_running_command() {
        let store = tempfile::tempdir().unwrap();
        let first = usage::Lease::acquire(store.path()).unwrap();
        let second = usage::Lease::acquire(store.path()).unwrap();
        let probe = |free: bool| {
            let result = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "linux_exec::tests::external_removal_lock_probe"])
                .env("CLAWSEED_TEST_USAGE_PATH", store.path().join("usage.lock"))
                .env("CLAWSEED_TEST_USAGE_FREE", free.to_string())
                .output()
                .unwrap();
            assert!(
                result.status.success(),
                "{}",
                String::from_utf8_lossy(&result.stdout)
            );
        };
        probe(false);
        drop(first);
        probe(false);
        drop(second);
        probe(true);
    }

    #[test]
    fn resolves_new_active_generation_without_recreating_tool() {
        let store = tempfile::tempdir().unwrap();
        for name in ["one", "two"] {
            let generation = store.path().join("versions").join(name);
            std::fs::create_dir_all(generation.join("rootfs")).unwrap();
            std::fs::write(generation.join("proot"), b"#!/bin/sh\n").unwrap();
            std::fs::set_permissions(
                generation.join("proot"),
                std::fs::Permissions::from_mode(0o755),
            )
            .unwrap();
            std::fs::write(
                store.path().join("active.json"),
                serde_json::json!({"generation": name}).to_string(),
            )
            .unwrap();
            let layout = RuntimeLayout::load(Some(store.path().to_path_buf())).unwrap();
            assert_eq!(layout.root, generation);
        }
        std::fs::write(
            store.path().join("active.json"),
            r#"{"generation":"../../outside"}"#,
        )
        .unwrap();
        assert!(RuntimeLayout::load(Some(store.path().to_path_buf())).is_err());
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
    #[tokio::test]
    async fn cancellation_interrupts_waiting_for_linux_capacity() {
        let limit = linux_exec_limit();
        let permits = limit
            .acquire_many_owned(MAX_CONCURRENT_LINUX_EXEC as u32)
            .await
            .unwrap();
        let (_runtime, context) = fake_runtime();
        let (sender, receiver) = tokio::sync::watch::channel(false);
        let command = run_linux_command(
            "touch should-not-run",
            context.workspace_dir(),
            Duration::from_secs(1),
            1024,
            Some(receiver),
            Some(PathBuf::from("/missing")),
        );
        let cancel = async {
            tokio::task::yield_now().await;
            sender.send(true).unwrap();
        };
        let (result, ()) = tokio::join!(command, cancel);
        assert_eq!(
            result.unwrap().termination,
            crate::shell::CommandTermination::Cancelled
        );
        assert!(!context.workspace_dir().join("should-not-run").exists());
        let timed_out = run_linux_command(
            "touch should-not-run",
            context.workspace_dir(),
            Duration::from_millis(20),
            1024,
            None,
            Some(PathBuf::from("/missing")),
        )
        .await
        .unwrap();
        assert_eq!(
            timed_out.termination,
            crate::shell::CommandTermination::TimedOut
        );
        drop(permits);
    }
}
