# clawseed-tools — Built-in Tool Implementations

## Overview

`clawseed-tools` contains 25+ built-in tool implementations. All tools depend only on `clawseed-api` traits. Tools that need runtime dependencies (Memory, etc.) receive them via constructor injection.

ClawSeed's dynamic tools use CETP semantics as well: Python/Node.js plugins in the Linux runtime use stdio, Android external apps use ContentProvider, and remote clients use WebSocket. The transport differs, but each capability is normalized to the same `Tool`/`ToolSpec` model and passes through the same registry, hooks, approvals, and security policy.

## Dynamic Extension Tools

Linux-runtime plugin directories contain a manifest declaring the entrypoint, protocol versions, runtime, workspace/network permissions, and resource limits. Gateway/Agent registers only plugins that pass validation and configuration allowlists. Plugins run as separate child processes and exchange tool specs and results through CETP over stdio.

Android external apps can expose tools through the CETP v1/v2 ContentProvider binding. The SDK discovers these providers and wraps their capabilities as `RemoteTool`, using the same Agent dispatch path as built-in tools. See the [External Tool Protocol](../external-tool-protocol.md).

Startup failures, timeouts, and exits are recorded as extension failures and cannot bypass Rust hooks, approvals, or tool policy. PRoot is Linux userspace compatibility, not a strong security sandbox.

## Tool List

### File Operations

| Tool | Name | Description |
|------|------|-------------|
| `FileReadTool` | `file_read` | Read file contents with offset and line limit support |
| `FileWriteTool` | `file_write` | Write to files, auto-creates parent directories |
| `FileEditTool` | `file_edit` | Precise string replacement editing |
| `GlobSearchTool` | `glob_search` | Filename pattern search |
| `ContentSearchTool` | `content_search` | File content search |
| `CsvProcessTool` | `csv_process` | Inspect, page, filter, sort, write, and append CSV files |

### Web

| Tool | Name | Description |
|------|------|-------------|
| `HttpRequestTool` | `http_request` | Send HTTP requests (configurable domain allowlists) |
| `WebFetchTool` | `web_fetch` | Extract article content or cleaned visible page text with pagination |
| `WebSearchTool` | `web_search_tool` | Web search via DuckDuckGo, Brave, SearXNG, Tavily, Bing, or DeepSeek Search |

`web_fetch` accepts `mode` (`article` by default or `text`), `start_index` (default `0`), and `max_chars` (default `12000`, maximum `50000`). `article` selects the strongest semantic content container and falls back to `text` when no reliable article is found. The JSON result reports `content`, `title`, the mode actually used, `start_index`, `next_start_index`, and `has_more`. Indexes count Unicode characters, so passing `next_start_index` to the next call neither repeats nor splits multibyte text.

HTML extraction removes scripts, styles, navigation, footers, sidebars, forms, and hidden elements. Pages with no readable server-rendered text return a JavaScript-rendering hint; no browser engine is started. Response bodies are streamed into a configured byte limit and rejected as `response_too_large` instead of being read fully and truncated afterward. Failures use one of six stable codes: `network`, `http_status`, `blocked_url`, `unsupported_content`, `parse_failed`, or `response_too_large`. Existing domain allowlists, blocked domains, private-host controls, redirect validation, and timeouts still apply.

### Memory

| Tool | Name | Description |
|------|------|-------------|
| `MemoryStoreTool` | `memory_store` | Store memories |
| `MemoryUpdateTool` | `memory_update` | Update an existing memory while preserving metadata |
| `MemoryRecallTool` | `memory_recall` | Retrieve relevant memories |
| `MemoryForgetTool` | `memory_forget` | Delete specific memories |
| `MemoryPurgeTool` | `memory_purge` | Clear expired memories |
| `MemoryExportTool` | `memory_export` | Export all memories |

### Automation

| Tool | Name | Description |
|------|------|-------------|
| `CronAddTool` | `cron_add` | Create scheduled jobs |
| `CronUpdateTool` | `cron_update` | Update scheduled jobs |
| `CronRemoveTool` | `cron_remove` | Delete scheduled jobs |
| `CronListTool` | `cron_list` | List all jobs |
| `CronRunTool` | `cron_run` | Manually execute a job |
| `CronRunsTool` | `cron_runs` | View run history |

### Development

| Tool | Name | Description |
|------|------|-------------|
| `ShellTool` | `shell` | Run workspace commands synchronously on host or Linux |
| `BackgroundRunTool` | `background_run` | Start a host/Linux command and immediately return a job ID |
| `LinuxExecTool` | `linux_exec` | Compatibility alias for `shell` with `backend=linux` |
| `BackgroundStatusTool` | `background_status` | Read job state, exit code, and bounded stdout/stderr |
| `BackgroundCancelTool` | `background_cancel` | Cancel a queued or running command and wait for it to stop |
| `GitOperationsTool` | `git_operations` | Git operations |
| `PdfReadTool` | `pdf_read` | Read PDF files |

`shell` and `background_run` share one command runner and accept `backend`,
`timeout_secs`, and `max_output_bytes`. The default backend is `host` (the native
Android shell on Android); `android` is an alias for `host`. `linux` uses the
installed PRoot runtime and fails without falling back when it is unavailable.
Existing command-only calls retain host behavior. `linux_exec` remains a fixed
Linux compatibility alias; new callers should use `shell` with `backend=linux`.
`eval_javascript` continues to use embedded QuickJS.

Synchronous calls default to 30 seconds, with a maximum of 300 seconds. The legacy
`linux_exec` alias keeps its 60-second default. Background jobs default to 300
seconds and allow up to 3600 seconds. Each output stream retains 1 MiB by default,
configurable from 1024 bytes to 4 MiB. Waiting for shared Linux execution capacity
counts against the timeout and can be cancelled. Running PRoot tasks receive
SIGQUIT to stop tracees before the remaining process group is cleaned up.
Before executing PRoot, the child restores SIGQUIT handling and unblocks that
signal so ART's inherited mask cannot suppress cancellation. Device regression
also covers tracees in detached process groups.

Background commands share a process-local queue with at most two running and 16
queued jobs. File-backed configurations persist results; after restart, former
active jobs are marked interrupted and never replayed. Completed history is
pruned on access and transitions to 24 hours, 64 entries and 16 MiB total output.
Omit `job_id` from `background_status` to list metadata and the active count.
Status snapshots and completion events include
the actual `backend`; `background_status` and `background_cancel` work for both.
The same command allowlist and sensitive-path policy applies to synchronous and
background commands. All Linux callers
share a maximum of two concurrent executions.

For example, call `shell` to run Python synchronously:

```json
{"backend":"linux","command":"python3 report.py","timeout_secs":60}
```

Call `background_run` for a longer task, then query or cancel its `job_id`:

```json
{"backend":"linux","command":"node process.js","timeout_secs":1800}
```

### Utilities

| Tool | Name | Description |
|------|------|-------------|
| `CalculatorTool` | `calculator` | Math calculations (25+ functions) |
| `JavaScriptTool` | `eval_javascript` | Sandboxed QuickJS ES2020 evaluation with bounded execution and output |
| `LlmTaskTool` | `llm_task` | LLM sub-tasks |
| `KnowledgeTool` | `knowledge` | Knowledge base queries |
| `AskUserTool` | `ask_user` | Request confirmation, a selection, or short text from the current user |

`ask_user` supports `confirm`, `single_select`, `multi_select`, and `text` questions. Gateway requests are bound to the session, turn, and tool-call IDs; only the first valid response is accepted. Pending questions expire after five minutes and are cancelled with their agent turn. The tool rejects prompts for passwords, API keys, access tokens, and payment information. Interactive CLI chat uses terminal input, while non-interactive callers receive `interaction_unavailable`.
| `ModelRoutingConfigTool` | `model_routing_config` | Model routing configuration |
| `BackupTool` | `backup` | Backup management |

`csv_process` supports `inspect`, `read`, `query`, `write`, and `append` operations. Reads and queries use workspace-relative paths, UTF-8 (optionally with a BOM), quoted fields, embedded newlines, configurable single-character delimiters, and pagination. `query` also supports column selection, multiple filters, and sorting. `write` accepts `headers` and array or object `rows` and replaces the file; `append` reads the existing header and adds array or object `rows` to the end of the file. Both write operations remain inside the workspace and use atomic replacement with process-local serialization. Input files are limited to 20 MiB and 200,000 records; each call returns at most 1,000 rows and 1 MiB of JSON.

## Registration Mechanism

All tools are registered through the `all_tools()` function:

```rust
pub fn all_tools(workspace_dir: PathBuf, config: &Config, memory: Arc<dyn Memory>) -> Vec<Box<dyn Tool>>
```

> **Note:** `crates/clawseed-agent/src/tools.rs` contains a transitional `all_tools_with_runtime()` function with a bloated signature (13 parameters, most unused). It delegates to the real `all_tools()` here. The agent crate's `tools.rs` is a stub/re-export layer — the actual implementation lives in this crate.

1. Instantiate all built-in tools
2. Filter based on configuration
   - `http_request` — only when `config.http_request.enabled` is true
   - `web_fetch` — only when `config.web_fetch.enabled` is true
   - `web_search` — only when `config.web_search.enabled` is true
   - `skill` / `skill_create` — only when `config.skills.enabled` is true
3. Conditionally-enabled tools can be configured with domain allowlists, etc.
4. Returns `Vec<Box<dyn Tool>>`

## Tool Implementation Patterns

### Basic Structure

```rust
pub struct MyTool;

impl MyTool {
    pub fn new() -> Self { Self }
}

#[async_trait]
impl Tool for MyTool {
    fn name(&self) -> &str { "my_tool" }
    fn description(&self) -> &str { "Does something useful" }
    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "param": { "type": "string", "description": "Parameter description" }
            },
            "required": ["param"]
        })
    }
    async fn execute(&self, args: Value, ctx: &dyn ToolContext) -> anyhow::Result<ToolResult> {
        // implementation
    }
}
```

### Key Patterns

1. **Stateless struct** — Tools are singletons; state managed via `Arc<Mutex<T>>`
2. **Parameter extraction** — Extract from JSON Value using `.and_then()` chains
3. **Workspace sandboxing** — File tools use `ctx.workspace_dir()` to scope paths, with canonicalize to prevent path traversal
4. **Constructor injection** — Tools that need dependencies (Memory, etc.) receive them via `new(Arc<dyn Memory>)`
5. **Error returns** — All errors wrapped in `ToolResult { success: false, error: Some(...) }`, never panic

### Path Safety Pattern

```rust
let full_path = ctx.workspace_dir().join(path);
let canonical = std::fs::canonicalize(&full_path)?;
let workspace_canon = std::fs::canonicalize(ctx.workspace_dir())?;
if !canonical.starts_with(&workspace_canon) {
    return Ok(ToolResult { success: false, output: String::new(), error: Some("Path outside workspace".into()) });
}
```

### Constructor Injection Pattern

```rust
pub struct MemoryStoreTool {
    memory: Arc<dyn Memory>,
}

impl MemoryStoreTool {
    pub fn new(memory: Arc<dyn Memory>) -> Self {
        Self { memory }
    }
}

#[async_trait]
impl Tool for MemoryStoreTool {
    async fn execute(&self, args: Value, _ctx: &dyn ToolContext) -> anyhow::Result<ToolResult> {
        // Use injected memory directly
        self.memory.store("key", &value, MemoryCategory::Core, None).await?;
        Ok(ToolResult { success: true, output: result, error: None })
    }
}
```

## Profile Management Tools

When the profile store is enabled, the registry adds `user_profile_search`, `user_profile_change_plan`, `user_profile_apply_plan`, and `user_profile_undo`. Search is read-only. Mutations first return a structured `ContentBlock::Profile` preview with before/after values and confirmation actions; apply checks identity, version, TTL, item limit, and single-use state. Successful apply returns an undo operation ID.

These tools never accept `user_id` as a model-controlled argument. They read the authenticated `UserContext` from `ToolContext`. The structured presentation is client-facing only; plain `ToolResult.output` remains the model-facing result.
