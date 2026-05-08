# clawseed-tools — 内置工具实现

## 概述

`clawseed-tools` 包含 25+ 内置工具的具体实现。所有工具仅依赖 `clawseed-api` 的 trait。需要运行时依赖（Memory 等）的工具通过构造函数注入获取。

## 工具清单

### 文件操作

| 工具 | 名称 | 描述 |
|------|------|------|
| `FileReadTool` | `file_read` | 读取文件内容，支持偏移和行数限制 |
| `FileWriteTool` | `file_write` | 写入文件，自动创建父目录 |
| `FileEditTool` | `file_edit` | 精确字符串替换编辑 |
| `GlobSearchTool` | `glob_search` | 文件名模式搜索 |
| `ContentSearchTool` | `content_search` | 文件内容搜索 |

### Web

| 工具 | 名称 | 描述 |
|------|------|------|
| `HttpRequestTool` | `http_request` | 发送 HTTP 请求（可配置域名白名单） |
| `WebFetchTool` | `web_fetch` | 抓取网页内容 |
| `WebSearchTool` | `web_search` | DuckDuckGo 搜索 |

### 记忆

| 工具 | 名称 | 描述 |
|------|------|------|
| `MemoryStoreTool` | `memory_store` | 存储记忆 |
| `MemoryRecallTool` | `memory_recall` | 检索相关记忆 |
| `MemoryForgetTool` | `memory_forget` | 删除指定记忆 |
| `MemoryPurgeTool` | `memory_purge` | 清除过期记忆 |
| `MemoryExportTool` | `memory_export` | 导出所有记忆 |

### 自动化

| 工具 | 名称 | 描述 |
|------|------|------|
| `CronAddTool` | `cron_add` | 创建定时任务 |
| `CronUpdateTool` | `cron_update` | 更新定时任务 |
| `CronRemoveTool` | `cron_remove` | 删除定时任务 |
| `CronListTool` | `cron_list` | 列出所有任务 |
| `CronRunTool` | `cron_run` | 手动执行任务 |
| `CronRunsTool` | `cron_runs` | 查看运行历史 |

### 开发

| 工具 | 名称 | 描述 |
|------|------|------|
| `ShellTool` | `shell` | 执行 shell 命令 |
| `GitOperationsTool` | `git_operations` | Git 操作 |
| `PdfReadTool` | `pdf_read` | 读取 PDF 文件 |

### 工具

| 工具 | 名称 | 描述 |
|------|------|------|
| `CalculatorTool` | `calculator` | 数学计算（25+ 函数） |
| `LlmTaskTool` | `llm_task` | LLM 子任务 |
| `KnowledgeTool` | `knowledge` | 知识库查询 |
| `ModelRoutingConfigTool` | `model_routing_config` | 模型路由配置 |
| `BackupTool` | `backup` | 备份管理 |

## 注册机制

所有工具通过 `all_tools()` 函数统一注册：

```rust
pub fn all_tools(workspace_dir: PathBuf, config: &Config, memory: Arc<dyn Memory>) -> Vec<Box<dyn Tool>>
```

> **注意：** `crates/clawseed-agent/src/tools.rs` 中有一个过渡性的 `all_tools_with_runtime()` 函数，签名臃肿（13 个参数，大多未使用），实际委托给本 crate 的 `all_tools()`。agent crate 的 `tools.rs` 是 stub/re-export 层——真正的实现在本 crate 中。

1. 实例化所有内置工具
2. 根据配置过滤条件启用/禁用
   - `http_request` — 仅当 `config.http_request.enabled` 为 true
   - `web_fetch` — 仅当 `config.web_fetch.enabled` 为 true
   - `web_search` — 仅当 `config.web_search.enabled` 为 true
3. 有条件工具可配置域名白名单等参数
4. 返回 `Vec<Box<dyn Tool>>`

## 工具实现模式

### 基本结构

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
                "param": { "type": "string", "description": "参数描述" }
            },
            "required": ["param"]
        })
    }
    async fn execute(&self, args: Value, ctx: &dyn ToolContext) -> anyhow::Result<ToolResult> {
        // 实现
    }
}
```

### 关键模式

1. **无状态结构体** — 工具是单例，状态通过 `Arc<Mutex<T>>` 管理
2. **参数提取** — 从 JSON Value 中提取，使用 `.and_then()` 链式处理
3. **工作区沙箱** — 文件工具通过 `ctx.workspace_dir()` 限定路径，使用 canonicalize 防止路径穿越
4. **构造函数注入** — 需要依赖（Memory 等）的工具通过 `new(Arc<dyn Memory>)` 接收
5. **错误返回** — 所有错误封装在 `ToolResult { success: false, error: Some(...) }`，而非 panic

### 路径安全模式

```rust
let full_path = ctx.workspace_dir().join(path);
let canonical = std::fs::canonicalize(&full_path)?;
let workspace_canon = std::fs::canonicalize(ctx.workspace_dir())?;
if !canonical.starts_with(&workspace_canon) {
    return Ok(ToolResult { success: false, output: String::new(), error: Some("Path outside workspace".into()) });
}
```

### 构造函数注入模式

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
        // 直接使用注入的 memory
        self.memory.store("key", &value, MemoryCategory::Core, None).await?;
        Ok(ToolResult { success: true, output: result, error: None })
    }
}
```
