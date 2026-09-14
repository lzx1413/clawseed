# clawseed-tools — Built-in Tool Implementations

## Overview

`clawseed-tools` contains 25+ built-in tool implementations. All tools depend only on `clawseed-api` traits. Tools that need runtime dependencies (Memory, etc.) receive them via constructor injection.

## Tool List

### File Operations

| Tool | Name | Description |
|------|------|-------------|
| `FileReadTool` | `file_read` | Read file contents with offset and line limit support |
| `FileWriteTool` | `file_write` | Write to files, auto-creates parent directories |
| `FileEditTool` | `file_edit` | Precise string replacement editing |
| `GlobSearchTool` | `glob_search` | Filename pattern search |
| `ContentSearchTool` | `content_search` | File content search |

### Web

| Tool | Name | Description |
|------|------|-------------|
| `HttpRequestTool` | `http_request` | Send HTTP requests (configurable domain allowlists) |
| `WebFetchTool` | `web_fetch` | Extract article content or cleaned visible page text with pagination |
| `WebSearchTool` | `web_search_tool` | Web search via DuckDuckGo, Brave, SearXNG, Tavily, or Bing |

`web_fetch` accepts `mode` (`article` by default or `text`), `start_index` (default `0`), and `max_chars` (default `12000`, maximum `50000`). `article` selects the strongest semantic content container and falls back to `text` when no reliable article is found. The JSON result reports `content`, `title`, the mode actually used, `start_index`, `next_start_index`, and `has_more`. Indexes count Unicode characters, so passing `next_start_index` to the next call neither repeats nor splits multibyte text.

HTML extraction removes scripts, styles, navigation, footers, sidebars, forms, and hidden elements. Pages with no readable server-rendered text return a JavaScript-rendering hint; no browser engine is started. Response bodies are streamed into a configured byte limit and rejected as `response_too_large` instead of being read fully and truncated afterward. Failures use one of six stable codes: `network`, `http_status`, `blocked_url`, `unsupported_content`, `parse_failed`, or `response_too_large`. Existing domain allowlists, blocked domains, private-host controls, redirect validation, and timeouts still apply.

### Memory

| Tool | Name | Description |
|------|------|-------------|
| `MemoryStoreTool` | `memory_store` | Store memories |
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
| `ShellTool` | `shell` | Execute shell commands |
| `BackgroundRunTool` | `background_run` | Start a workspace command and immediately return a job ID |
| `BackgroundStatusTool` | `background_status` | Read job state, exit code, and bounded stdout/stderr |
| `BackgroundCancelTool` | `background_cancel` | Cancel a queued or running command and wait for it to stop |
| `GitOperationsTool` | `git_operations` | Git operations |
| `PdfReadTool` | `pdf_read` | Read PDF files |

Background commands use a process-local queue with at most two running and 16 queued jobs. Each job has a five-minute runtime limit and stores at most 1 MiB from each output stream. `background_run` passes through the same `SecurityPolicy` command allowlist and sensitive-path checks as `shell`. Jobs do not survive a gateway restart.

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
