//! Local CETP plugin discovery and stdio transport.
//!
//! The first implementation runs Python and Node.js entrypoints as long-lived
//! child processes. Runtime-specific launchers can be added without changing
//! the CETP request/response handling in this module.

use anyhow::{Context, Result, anyhow, bail};
use async_trait::async_trait;
use clawseed_api::tool::{Tool, ToolPresentation, ToolResult, ToolSpec};
use clawseed_api::tool_context::ToolContext;
use clawseed_api::tool_registry::ToolSource;
use clawseed_config::schema::PluginsConfig;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{
    Arc,
    atomic::{AtomicU8, Ordering},
};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout};
use tokio::sync::Mutex;
use tokio::time::timeout;
use uuid::Uuid;

const PROTOCOL_VERSION: u32 = 2;
const DEFAULT_STARTUP_TIMEOUT: Duration = Duration::from_secs(5);
const DEFAULT_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const DEFAULT_MAX_OUTPUT_BYTES: usize = 1_048_576;
const MAX_MAX_OUTPUT_BYTES: usize = 4 * 1_024 * 1_024;

/// A tool and the source metadata needed by the registry.
pub struct PluginToolRegistration {
    pub tool: Box<dyn Tool>,
    pub source: ToolSource,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PluginManifest {
    pub schema_version: u32,
    pub plugin_id: String,
    pub name: String,
    pub version: String,
    pub description: Option<String>,
    pub entry: String,
    pub protocol: ProtocolConfig,
    pub runtime: RuntimeConfig,
    #[serde(default)]
    pub permissions: PermissionsConfig,
    #[serde(default)]
    pub limits: LimitsConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ProtocolConfig {
    pub versions: Vec<u32>,
    pub transport: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RuntimeConfig {
    pub language: String,
    #[serde(default)]
    pub components: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PermissionsConfig {
    #[serde(default = "default_workspace_permission")]
    pub workspace: String,
    #[serde(default = "default_network_permission")]
    pub network: String,
    #[serde(default)]
    pub environment: Vec<String>,
}

fn default_workspace_permission() -> String {
    "none".into()
}

fn default_network_permission() -> String {
    "deny".into()
}

impl Default for PermissionsConfig {
    fn default() -> Self {
        Self {
            workspace: default_workspace_permission(),
            network: default_network_permission(),
            environment: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct LimitsConfig {
    #[serde(default = "default_startup_timeout_ms")]
    pub startup_timeout_ms: u64,
    #[serde(default = "default_request_timeout_ms")]
    pub request_timeout_ms: u64,
    #[serde(default = "default_max_concurrent_requests")]
    pub max_concurrent_requests: usize,
    #[serde(default = "default_max_output_bytes")]
    pub max_output_bytes: usize,
}

fn default_startup_timeout_ms() -> u64 {
    DEFAULT_STARTUP_TIMEOUT.as_millis() as u64
}

fn default_request_timeout_ms() -> u64 {
    DEFAULT_REQUEST_TIMEOUT.as_millis() as u64
}

fn default_max_output_bytes() -> usize {
    DEFAULT_MAX_OUTPUT_BYTES
}

fn default_max_concurrent_requests() -> usize {
    1
}

impl Default for LimitsConfig {
    fn default() -> Self {
        Self {
            startup_timeout_ms: default_startup_timeout_ms(),
            request_timeout_ms: default_request_timeout_ms(),
            max_concurrent_requests: default_max_concurrent_requests(),
            max_output_bytes: default_max_output_bytes(),
        }
    }
}

impl PluginManifest {
    pub fn validate(&self, root: &Path) -> Result<PathBuf> {
        if self.schema_version != 1 {
            bail!(
                "unsupported plugin manifest schema: {}",
                self.schema_version
            );
        }
        if !valid_plugin_id(&self.plugin_id) {
            bail!("invalid plugin_id: {}", self.plugin_id);
        }
        if self.name.trim().is_empty() || self.version.trim().is_empty() {
            bail!("plugin name and version are required");
        }
        if self.protocol.transport != "stdio" {
            bail!("unsupported plugin transport: {}", self.protocol.transport);
        }
        if !self.protocol.versions.contains(&PROTOCOL_VERSION) {
            bail!("plugin does not support CETP v{PROTOCOL_VERSION}");
        }
        if !matches!(self.runtime.language.as_str(), "python" | "node") {
            bail!("unsupported plugin language: {}", self.runtime.language);
        }
        if !matches!(
            self.permissions.workspace.as_str(),
            "none" | "read" | "write"
        ) {
            bail!(
                "unsupported workspace permission: {}",
                self.permissions.workspace
            );
        }
        if !matches!(self.permissions.network.as_str(), "deny" | "allow") {
            bail!(
                "unsupported network permission: {}",
                self.permissions.network
            );
        }
        if self
            .permissions
            .environment
            .iter()
            .any(|name| name.is_empty() || name.contains('=') || name.as_bytes().contains(&0))
        {
            bail!("environment contains an invalid variable name");
        }
        if self.limits.max_concurrent_requests == 0 {
            bail!("max_concurrent_requests must be greater than zero");
        }
        if !(1024..=MAX_MAX_OUTPUT_BYTES).contains(&self.limits.max_output_bytes) {
            bail!("max_output_bytes must be between 1024 and {MAX_MAX_OUTPUT_BYTES}");
        }
        let entry = Path::new(&self.entry);
        if entry.is_absolute()
            || entry
                .components()
                .any(|component| matches!(component, std::path::Component::ParentDir))
        {
            bail!("plugin entry must stay inside the plugin directory");
        }
        let entry = root.join(entry);
        if !entry.is_file() {
            bail!("plugin entry is missing: {}", entry.display());
        }
        let canonical_root = root
            .canonicalize()
            .with_context(|| format!("could not canonicalize plugin root {}", root.display()))?;
        let canonical_entry = entry
            .canonicalize()
            .with_context(|| format!("could not canonicalize plugin entry {}", entry.display()))?;
        if !canonical_entry.starts_with(&canonical_root) {
            bail!("plugin entry resolves outside the plugin directory");
        }
        Ok(canonical_entry)
    }
}

pub fn valid_plugin_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"_.-".contains(&byte)
        })
        && value.as_bytes()[0].is_ascii_lowercase()
}

/// Validate a plugin directory and perform its CETP startup handshake.
pub async fn validate_plugin_root(
    root: &Path,
    workspace: &Path,
) -> Result<(PluginManifest, Vec<ToolSpec>)> {
    let manifest_path = root.join("manifest.json");
    let manifest: PluginManifest = serde_json::from_slice(
        &tokio::fs::read(&manifest_path)
            .await
            .with_context(|| format!("could not read {}", manifest_path.display()))?,
    )
    .with_context(|| format!("invalid plugin manifest: {}", manifest_path.display()))?;
    let entry = manifest.validate(root)?;
    let client = PluginClient::start(
        manifest.clone(),
        root.to_path_buf(),
        entry,
        workspace.to_path_buf(),
    )
    .await?;
    let tools = client.list_tools().await?;
    Ok((manifest, tools))
}

struct PluginSession {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    _runtime_guard: Option<crate::linux_exec::LinuxPluginGuard>,
}

struct PluginClient {
    manifest: PluginManifest,
    root: PathBuf,
    entry: PathBuf,
    workspace: PathBuf,
    session: Mutex<Option<PluginSession>>,
    restart_attempts: AtomicU8,
    session_token: Mutex<String>,
}

impl PluginClient {
    async fn start(
        manifest: PluginManifest,
        root: PathBuf,
        entry: PathBuf,
        workspace: PathBuf,
    ) -> Result<Arc<Self>> {
        let client = Arc::new(Self {
            manifest,
            root,
            entry,
            workspace,
            session: Mutex::new(None),
            restart_attempts: AtomicU8::new(0),
            session_token: Mutex::new("local-session".into()),
        });
        let session = client.start_session().await?;
        *client.session.lock().await = Some(session);
        Ok(client)
    }

    async fn start_session(&self) -> Result<PluginSession> {
        let mut session = self.spawn().await?;
        let timeout_duration = self.startup_timeout();
        let negotiation = self
            .request_locked(
                &mut session,
                "negotiate",
                json!({
                    "consumer_versions": [PROTOCOL_VERSION],
                    "consumer_capabilities": []
                }),
                timeout_duration,
            )
            .await
            .with_context(|| {
                format!(
                    "plugin {} negotiate failed or timed out",
                    self.manifest.plugin_id
                )
            })?;
        let token = negotiation
            .get("session_token")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .unwrap_or("local-session")
            .to_owned();
        *self.session_token.lock().await = token;
        let info = self
            .request_locked(
                &mut session,
                "get_provider_info",
                json!({
                    "protocol_version": PROTOCOL_VERSION,
                    "session_token": self.session_token().await
                }),
                timeout_duration,
            )
            .await
            .with_context(|| {
                format!(
                    "plugin {} get_provider_info failed or timed out",
                    self.manifest.plugin_id
                )
            })?;
        let provider_id = info
            .get("provider_id")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow!("plugin provider info is missing provider_id"))?;
        if provider_id != self.manifest.plugin_id {
            bail!(
                "plugin provider_id {provider_id} does not match {}",
                self.manifest.plugin_id
            );
        }
        Ok(session)
    }

    async fn list_tools(&self) -> Result<Vec<ToolSpec>> {
        let mut guard = self.session.lock().await;
        if guard.is_none() {
            *guard = Some(self.start_session().await?);
        }
        let session = guard
            .as_mut()
            .ok_or_else(|| anyhow!("plugin session is not running"))?;
        let data = self
            .request_locked(
                session,
                "list_tools",
                json!({
                    "protocol_version": PROTOCOL_VERSION,
                    "session_token": self.session_token().await
                }),
                self.request_timeout(),
            )
            .await
            .with_context(|| {
                format!(
                    "plugin {} list_tools failed or timed out",
                    self.manifest.plugin_id
                )
            })?;
        self.restart_attempts.store(0, Ordering::Relaxed);
        let tools = data
            .get("tools")
            .and_then(Value::as_array)
            .ok_or_else(|| anyhow!("plugin list_tools response is missing tools"))?;
        tools
            .iter()
            .cloned()
            .map(|value| serde_json::from_value(value).context("invalid plugin tool spec"))
            .collect()
    }

    async fn call(&self, tool_name: &str, args: Value) -> Result<Value> {
        let mut guard = self.session.lock().await;
        if guard.is_none() {
            if self.restart_attempts.fetch_add(1, Ordering::Relaxed) >= 3 {
                bail!("plugin exceeded the restart limit");
            }
            *guard = Some(self.start_session().await?);
        }
        let session = guard
            .as_mut()
            .ok_or_else(|| anyhow!("plugin session is not running"))?;
        let result = self
            .request_locked(
                session,
                "execute_tool",
                json!({
                    "protocol_version": PROTOCOL_VERSION,
                "session_token": self.session_token().await,
                    "request_id": Uuid::new_v4().to_string(),
                    "tool_name": tool_name,
                    "args": args
                }),
                self.request_timeout(),
            )
            .await;
        if result.is_err() {
            self.stop_session(&mut guard).await;
        } else {
            self.restart_attempts.store(0, Ordering::Relaxed);
        }
        result
    }

    async fn stop_session(&self, session: &mut Option<PluginSession>) {
        if let Some(mut session) = session.take() {
            let _ = session.child.kill().await;
            let _ = session.child.wait().await;
        }
    }

    async fn session_token(&self) -> String {
        self.session_token.lock().await.clone()
    }

    async fn spawn(&self) -> Result<PluginSession> {
        #[cfg(unix)]
        {
            let permissions = crate::linux_exec::LinuxPluginPermissions {
                workspace: &self.manifest.permissions.workspace,
                network: &self.manifest.permissions.network,
                environment: &self.manifest.permissions.environment,
            };
            let (child, runtime_guard) = crate::linux_exec::spawn_linux_plugin(
                &self.manifest.runtime.language,
                &self.entry,
                &self.root,
                &self.workspace,
                None,
                &self.manifest.plugin_id,
                &permissions,
            )
            .await
            .with_context(|| {
                format!(
                    "Linux runtime is required for plugin {}",
                    self.manifest.plugin_id
                )
            })?;
            self.take_session(child, Some(runtime_guard))
        }

        #[cfg(not(unix))]
        bail!("Linux runtime plugins require a Unix host");
    }

    fn take_session(
        &self,
        mut child: Child,
        runtime_guard: Option<crate::linux_exec::LinuxPluginGuard>,
    ) -> Result<PluginSession> {
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| anyhow!("plugin stdin unavailable"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| anyhow!("plugin stdout unavailable"))?;
        if let Some(mut stderr) = child.stderr.take() {
            let plugin_id = self.manifest.plugin_id.clone();
            tokio::spawn(async move {
                let mut buffer = [0_u8; 4096];
                loop {
                    match tokio::io::AsyncReadExt::read(&mut stderr, &mut buffer).await {
                        Ok(0) | Err(_) => break,
                        Ok(size) => {
                            let text = String::from_utf8_lossy(&buffer[..size]);
                            tracing::debug!(plugin = %plugin_id, "plugin stderr: {text}");
                        }
                    }
                }
            });
        }
        Ok(PluginSession {
            child,
            stdin,
            stdout: BufReader::new(stdout),
            _runtime_guard: runtime_guard,
        })
    }

    async fn request_locked(
        &self,
        session: &mut PluginSession,
        method: &str,
        params: Value,
        request_timeout: Duration,
    ) -> Result<Value> {
        let request_id = Uuid::new_v4().to_string();
        let request = StdioRequest {
            id: request_id.clone(),
            method: method.to_owned(),
            params,
        };
        let mut encoded = serde_json::to_vec(&request)?;
        encoded.push(b'\n');
        if encoded.len() > self.manifest.limits.max_output_bytes {
            bail!("plugin request exceeds configured output limit");
        }
        session.stdin.write_all(&encoded).await?;
        session.stdin.flush().await?;
        let line = timeout(
            request_timeout,
            read_bounded_line(&mut session.stdout, self.manifest.limits.max_output_bytes),
        )
        .await??;
        let Some(line) = line else {
            let status = session.child.try_wait()?.map(|status| status.to_string());
            bail!(
                "plugin exited before responding{}",
                status.map(|s| format!(": {s}")).unwrap_or_default()
            );
        };
        let response: StdioResponse = serde_json::from_slice(&line)
            .with_context(|| format!("invalid response from plugin method {method}"))?;
        if response.id != request_id {
            bail!("plugin response id does not match request");
        }
        if response.status != "success" {
            let error = response
                .error
                .map(|value| value.to_string())
                .unwrap_or_else(|| "plugin request failed".into());
            bail!("plugin {method} failed: {error}");
        }
        response
            .data
            .ok_or_else(|| anyhow!("plugin {method} response is missing data"))
    }

    fn startup_timeout(&self) -> Duration {
        Duration::from_millis(self.manifest.limits.startup_timeout_ms.max(1))
    }

    fn request_timeout(&self) -> Duration {
        Duration::from_millis(self.manifest.limits.request_timeout_ms.max(1))
    }
}

#[derive(Debug, Serialize)]
struct StdioRequest {
    id: String,
    method: String,
    params: Value,
}

#[derive(Debug, Deserialize)]
struct StdioResponse {
    id: String,
    status: String,
    data: Option<Value>,
    error: Option<Value>,
}

async fn read_bounded_line<R>(
    reader: &mut BufReader<R>,
    max_bytes: usize,
) -> std::io::Result<Option<Vec<u8>>>
where
    R: tokio::io::AsyncRead + Unpin,
{
    let mut line = Vec::new();
    loop {
        let chunk = reader.fill_buf().await?;
        if chunk.is_empty() {
            return Ok((!line.is_empty()).then_some(line));
        }
        let newline = chunk.iter().position(|byte| *byte == b'\n');
        let take = newline.map_or(chunk.len(), |position| position + 1);
        if line.len().saturating_add(take) > max_bytes {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "plugin response exceeds configured output limit",
            ));
        }
        line.extend_from_slice(&chunk[..take]);
        reader.consume(take);
        if newline.is_some() {
            return Ok(Some(line));
        }
    }
}

struct PluginTool {
    client: Arc<PluginClient>,
    inner_name: String,
    spec: ToolSpec,
    qualified_name: String,
}

#[async_trait]
impl Tool for PluginTool {
    fn name(&self) -> &str {
        &self.qualified_name
    }

    fn description(&self) -> &str {
        &self.spec.description
    }

    fn parameters_schema(&self) -> Value {
        self.spec.parameters.clone()
    }

    async fn execute(&self, args: Value, _ctx: &dyn ToolContext) -> Result<ToolResult> {
        let value = self.client.call(&self.inner_name, args).await?;
        plugin_tool_result(value)
    }
}

/// Convert the transport-neutral CETP result into the Agent tool result.
///
/// Existing plugins may return any JSON value directly. New plugins can return
/// `{ "output": ..., "presentation": ... }` to provide rich client content
/// without putting media URLs into the LLM-facing text.
fn plugin_tool_result(value: Value) -> Result<ToolResult> {
    let (output_value, presentation) = match value {
        Value::Object(mut object) => {
            let presentation = object
                .remove("presentation")
                .map(serde_json::from_value::<ToolPresentation>)
                .transpose()
                .context("invalid plugin tool presentation")?;
            let output_value = object.remove("output").unwrap_or(Value::Object(object));
            (output_value, presentation)
        }
        value => (value, None),
    };
    if let Some(presentation) = &presentation {
        presentation
            .validate()
            .context("invalid plugin tool presentation")?;
    }
    let output = match output_value {
        Value::String(value) => value,
        value => serde_json::to_string(&value)?,
    };
    Ok(ToolResult {
        success: true,
        output,
        error: None,
        presentation,
    })
}

/// Discover and initialize enabled local plugins.
pub async fn load_plugin_tools(
    workspace_dir: &Path,
    config: &PluginsConfig,
) -> Vec<PluginToolRegistration> {
    if !config.enabled || config.max_loaded == 0 {
        return Vec::new();
    }
    let mut roots = vec![workspace_dir.join(".clawseed/plugins")];
    roots.extend(config.extra_roots.iter().map(PathBuf::from));
    let mut seen = HashSet::new();
    let mut registrations = Vec::new();
    let mut loaded_plugins = 0usize;
    for root in roots {
        let Ok(entries) = std::fs::read_dir(&root) else {
            continue;
        };
        let mut plugin_dirs: Vec<_> = entries
            .filter_map(std::result::Result::ok)
            .map(|entry| entry.path())
            .filter(|path| path.is_dir())
            .collect();
        plugin_dirs.sort();
        for plugin_dir in plugin_dirs {
            if loaded_plugins >= config.max_loaded {
                return registrations;
            }
            let manifest_path = plugin_dir.join("manifest.json");
            let manifest = match std::fs::read(&manifest_path)
                .ok()
                .and_then(|bytes| serde_json::from_slice::<PluginManifest>(&bytes).ok())
            {
                Some(manifest) => manifest,
                None => {
                    tracing::warn!(path = %manifest_path.display(), "skipping invalid plugin manifest");
                    continue;
                }
            };
            if !config.allowed.is_empty()
                && !config.allowed.iter().any(|id| id == &manifest.plugin_id)
            {
                continue;
            }
            if config.denied.iter().any(|id| id == &manifest.plugin_id)
                || !seen.insert(manifest.plugin_id.clone())
            {
                continue;
            }
            let plugin_id = manifest.plugin_id.clone();
            let entry = match manifest.validate(&plugin_dir) {
                Ok(entry) => entry,
                Err(error) => {
                    tracing::warn!(plugin = %plugin_id, "skipping invalid plugin: {error:#}");
                    continue;
                }
            };
            let client =
                match PluginClient::start(manifest, plugin_dir, entry, workspace_dir.to_path_buf())
                    .await
                {
                    Ok(client) => client,
                    Err(error) => {
                        tracing::warn!(plugin = %plugin_id, "failed to start plugin: {error:#}");
                        continue;
                    }
                };
            let specs = match client.list_tools().await {
                Ok(specs) => specs,
                Err(error) => {
                    tracing::warn!(plugin = %plugin_id, "failed to list plugin tools: {error:#}");
                    continue;
                }
            };
            loaded_plugins += 1;
            for spec in specs {
                if !valid_tool_name(&spec.name) {
                    tracing::warn!(plugin = %plugin_id, tool = %spec.name, "skipping invalid plugin tool name");
                    continue;
                }
                let inner_name = spec.name.clone();
                let qualified_name = format!("{plugin_id}__{inner_name}");
                registrations.push(PluginToolRegistration {
                    tool: Box::new(PluginTool {
                        client: Arc::clone(&client),
                        inner_name,
                        spec,
                        qualified_name,
                    }),
                    source: ToolSource::Plugin {
                        plugin_id: plugin_id.clone(),
                    },
                });
            }
        }
    }
    registrations
}

fn valid_tool_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value.as_bytes()[0].is_ascii_lowercase()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
}

#[cfg(test)]
mod tests {
    use super::*;
    use clawseed_config::schema::PluginsConfig;
    use std::fs;
    use tempfile::tempdir;

    struct TestContext {
        workspace: PathBuf,
    }

    impl ToolContext for TestContext {
        fn workspace_dir(&self) -> &Path {
            &self.workspace
        }
    }

    fn runtime_configured() -> bool {
        std::env::var_os("CLAWSEED_PROOT_RUNTIME_DIR").is_some()
    }

    #[test]
    fn validates_python_and_node_manifests() {
        let root = tempdir().unwrap();
        let entry = root.path().join("entry.py");
        fs::write(&entry, "print('ok')").unwrap();
        let manifest = PluginManifest {
            schema_version: 1,
            plugin_id: "example.weather".into(),
            name: "Weather".into(),
            version: "0.1.0".into(),
            description: None,
            entry: "entry.py".into(),
            protocol: ProtocolConfig {
                versions: vec![2],
                transport: "stdio".into(),
            },
            runtime: RuntimeConfig {
                language: "python".into(),
                components: vec!["python".into()],
            },
            permissions: PermissionsConfig::default(),
            limits: LimitsConfig::default(),
        };
        assert!(manifest.validate(root.path()).is_ok());
        let mut node_manifest = manifest.clone();
        node_manifest.runtime.language = "node".into();
        assert!(node_manifest.validate(root.path()).is_ok());
    }

    #[tokio::test]
    async fn discovers_and_calls_python_plugin() {
        if !runtime_configured() {
            return;
        }
        let root = tempdir().unwrap();
        let plugin = root.path().join("example.echo");
        fs::create_dir_all(&plugin).unwrap();
        fs::write(
            plugin.join("manifest.json"),
            serde_json::json!({
                "schema_version": 1,
                "plugin_id": "example.echo",
                "name": "Echo",
                "version": "0.1.0",
                "entry": "plugin.py",
                "protocol": {"versions": [2], "transport": "stdio"},
                "runtime": {"language": "python", "components": ["python"]}
            })
            .to_string(),
        )
        .unwrap();
        fs::write(
            plugin.join("plugin.py"),
            r#"import json, sys
for line in sys.stdin:
    request = json.loads(line)
    method = request["method"]
    if method == "negotiate":
        data = {"selected_version": 2}
    elif method == "get_provider_info":
        data = {"provider_id": "example.echo"}
    elif method == "list_tools":
        data = {"tools": [{"name": "echo", "description": "Echo input", "parameters": {"type": "object"}}]}
    elif method == "execute_tool":
        data = request["params"]["args"]
    else:
        data = {}
    print(json.dumps({"id": request["id"], "status": "success", "data": data}), flush=True)
"#,
        )
        .unwrap();
        let config = PluginsConfig {
            enabled: true,
            extra_roots: vec![root.path().to_string_lossy().into_owned()],
            ..Default::default()
        };
        let mut registrations = load_plugin_tools(root.path(), &config).await;
        assert_eq!(registrations.len(), 1);
        assert_eq!(registrations[0].tool.name(), "example.echo__echo");
        let registration = registrations.pop().unwrap();
        let result = registration
            .tool
            .execute(
                json!({"text": "hello"}),
                &TestContext {
                    workspace: root.path().to_path_buf(),
                },
            )
            .await
            .unwrap();
        assert!(result.success);
        assert_eq!(result.output, r#"{"text":"hello"}"#);
    }

    #[tokio::test]
    async fn discovers_node_plugin_when_runtime_is_configured() {
        if !runtime_configured() {
            return;
        }
        let root = tempdir().unwrap();
        let plugin = root.path().join("example.node");
        fs::create_dir_all(&plugin).unwrap();
        fs::write(
            plugin.join("manifest.json"),
            serde_json::json!({
                "schema_version": 1,
                "plugin_id": "example.node",
                "name": "Node echo",
                "version": "0.1.0",
                "entry": "plugin.js",
                "protocol": {"versions": [2], "transport": "stdio"},
                "runtime": {"language": "node", "components": ["node"]}
            })
            .to_string(),
        )
        .unwrap();
        fs::write(
            plugin.join("plugin.js"),
            r#"const readline = require('readline');
const input = readline.createInterface({ input: process.stdin });
input.on('line', (line) => {
  const request = JSON.parse(line);
  let data = {};
  if (request.method === 'get_provider_info') data = { provider_id: 'example.node' };
  if (request.method === 'list_tools') data = { tools: [{ name: 'echo', description: 'Echo input', parameters: { type: 'object' } }] };
  if (request.method === 'execute_tool') data = request.params.args;
  process.stdout.write(JSON.stringify({ id: request.id, status: 'success', data }) + '\n');
});
"#,
        )
        .unwrap();
        let config = PluginsConfig {
            enabled: true,
            extra_roots: vec![root.path().to_string_lossy().into_owned()],
            ..Default::default()
        };
        let registrations = load_plugin_tools(root.path(), &config).await;
        assert_eq!(registrations.len(), 1);
        assert_eq!(registrations[0].tool.name(), "example.node__echo");
    }

    #[test]
    fn plugin_result_preserves_legacy_json_values() {
        let result = plugin_tool_result(json!({"temperature": 21})).unwrap();
        assert_eq!(result.output, r#"{"temperature":21}"#);
        assert!(result.presentation.is_none());
    }

    #[test]
    fn plugin_result_maps_and_validates_media_presentation() {
        let result = plugin_tool_result(json!({
            "output": "forecast",
            "presentation": {
                "version": 1,
                "blocks": [{
                    "type": "image",
                    "media": {
                        "url": "https://cdn.example.com/weather.png",
                        "mime_type": "image/png"
                    },
                    "alt": "Weather chart"
                }, {
                    "type": "audio",
                    "media": {
                        "url": "https://cdn.example.com/forecast.mp3",
                        "mime_type": "audio/mpeg"
                    },
                    "title": "Forecast"
                }, {
                    "type": "video",
                    "media": {
                        "url": "https://cdn.example.com/radar.mp4",
                        "mime_type": "video/mp4"
                    },
                    "title": "Radar"
                }]
            }
        }))
        .unwrap();
        assert_eq!(result.output, "forecast");
        assert_eq!(result.presentation.as_ref().unwrap().blocks.len(), 3);
    }

    #[test]
    fn plugin_result_rejects_insecure_media() {
        let error = plugin_tool_result(json!({
            "output": "unsafe",
            "presentation": {
                "version": 1,
                "blocks": [{
                    "type": "image",
                    "media": {"url": "http://cdn.example.com/image.png"}
                }]
            }
        }))
        .unwrap_err();
        assert!(format!("{error:#}").contains("HTTPS"));
    }
}
