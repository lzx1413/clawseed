# clawseed-tools — Built-in Tool Implementations

## Overview

`clawseed-tools` contains 25+ built-in tool implementations. All tools depend only on `clawseed-api` traits and access runtime capabilities via `ctx.get::<T>()`.

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
| `WebFetchTool` | `web_fetch` | Fetch web page content |
| `WebSearchTool` | `web_search` | DuckDuckGo search |

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
| `GitOperationsTool` | `git_operations` | Git operations |
| `PdfReadTool` | `pdf_read` | Read PDF files |

### Utilities

| Tool | Name | Description |
|------|------|-------------|
| `CalculatorTool` | `calculator` | Math calculations (25+ functions) |
| `LlmTaskTool` | `llm_task` | LLM sub-tasks |
| `KnowledgeTool` | `knowledge` | Knowledge base queries |
| `ModelRoutingConfigTool` | `model_routing_config` | Model routing configuration |
| `BackupTool` | `backup` | Backup management |

## Registration Mechanism

All tools are registered through the `all_tools()` function:

```rust
pub fn all_tools(workspace_dir: PathBuf, config: &Config) -> Vec<Box<dyn Tool>>
```

1. Instantiate all built-in tools
2. Filter based on configuration
   - `http_request` — only when `config.http_request.enabled` is true
   - `web_fetch` — only when `config.web_fetch.enabled` is true
   - `web_search` — only when `config.web_search.enabled` is true
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
4. **Security checks** — Access security policy via `ctx.get::<SecurityPolicy>()`
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

### Conditional Capability Pattern

```rust
async fn execute(&self, args: Value, ctx: &dyn ToolContext) -> anyhow::Result<ToolResult> {
    // Use memory capability when available
    if let Some(memory) = ctx.get::<dyn Memory>() {
        memory.store(&result, "tool_output").await?;
    }
    // Skip if unavailable — graceful degradation
    Ok(ToolResult { success: true, output: result, error: None })
}
```
