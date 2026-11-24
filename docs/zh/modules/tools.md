# clawseed-tools — 内置工具实现

## 概述

`clawseed-tools` 包含 25+ 内置工具的具体实现。所有工具仅依赖 `clawseed-api` 的 trait。需要运行时依赖（Memory 等）的工具通过构造函数注入获取。

ClawSeed 的动态工具也采用 CETP 语义：Linux Runtime 中的 Python/Node.js 插件使用 stdio 传输，Android 外部应用使用 ContentProvider 传输，远程客户端使用 WebSocket 传输。传输不同，但都转换为统一的 `Tool`/`ToolSpec`，由同一套注册、Hook、审批和安全策略处理。

## 动态扩展工具

Linux Runtime 插件目录包含 manifest，声明入口、协议版本、运行时、工作区/网络权限和资源限制；Gateway/Agent 只注册通过校验且满足配置白名单的插件。插件以独立子进程运行，通过 CETP over stdio 返回工具规格和执行结果。

Android 外部应用可以通过 CETP v1/v2 的 ContentProvider 绑定提供工具；这些工具由 SDK 发现后包装为 RemoteTool，和内置工具使用同一个 Agent 调度路径。CETP v1/v2 的 Android 传输规范见[外部工具协议](../external-tool-protocol.md)。

启动失败、超时或进程退出会被记录为扩展故障，不应绕过 Rust 的 Hook、审批和工具策略。Android 默认使用 tawcroot，启动错误回退 PRoot；两者都是 Linux 用户态兼容层，不是强安全沙箱。

## 工具清单

### 文件操作

| 工具 | 名称 | 描述 |
|------|------|------|
| `FileReadTool` | `file_read` | 读取文件内容，支持偏移和行数限制 |
| `FileWriteTool` | `file_write` | 写入文件，自动创建父目录 |
| `FileEditTool` | `file_edit` | 精确字符串替换编辑 |
| `GlobSearchTool` | `glob_search` | 文件名模式搜索 |
| `ContentSearchTool` | `content_search` | 文件内容搜索 |
| `CsvProcessTool` | `csv_process` | 检查、分页读取、过滤排序、写入和追加 CSV |

### Web

| 工具 | 名称 | 描述 |
|------|------|------|
| `HttpRequestTool` | `http_request` | 发送 HTTP 请求（可配置域名白名单） |
| `WebFetchTool` | `web_fetch` | 提取文章正文或清理后的页面可见文本，并支持分页 |
| `WebSearchTool` | `web_search_tool` | 通过 DuckDuckGo、Brave、SearXNG、Tavily、Bing 或 DeepSeek Search 搜索 |

`web_fetch` 接受 `mode`（默认 `article`，也可选 `text`）、`start_index`（默认 `0`）和 `max_chars`（默认 `12000`，最大 `50000`）。`article` 会选择得分最高的语义正文容器；找不到可靠正文时降级为 `text`。JSON 结果包含 `content`、`title`、实际使用的模式、`start_index`、`next_start_index` 和 `has_more`。索引按 Unicode 字符计数，把 `next_start_index` 传给下一次调用不会重复内容，也不会切断多字节字符。

HTML 提取会移除脚本、样式、导航、页脚、侧栏、表单和隐藏元素。页面没有服务端渲染的可读文本时会提示可能依赖 JavaScript，不启动浏览器引擎。响应正文以流式方式读入配置的字节上限，超过上限会返回 `response_too_large`，不会完整读入后再截断。错误使用六种稳定代码之一：`network`、`http_status`、`blocked_url`、`unsupported_content`、`parse_failed` 或 `response_too_large`。原有域名白名单、阻止列表、私网控制、重定向校验和超时继续生效。

### 记忆

| 工具 | 名称 | 描述 |
|------|------|------|
| `MemoryStoreTool` | `memory_store` | 存储记忆 |
| `MemoryUpdateTool` | `memory_update` | 更新已有记忆并保留元数据 |
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
| `ShellTool` | `shell` | 选择 host/Linux 后端同步执行工作区命令 |
| `BackgroundRunTool` | `background_run` | 选择 host/Linux 后端启动命令，立即返回作业 ID |
| `LinuxExecTool` | `linux_exec` | `shell` 的 Linux 兼容入口，新调用使用 `shell` + `backend=linux` |
| `PluginCreateTool` | `plugin_create` | 生成 Python 或 Node.js CETP 插件脚手架 |
| `PluginListTool` | `plugin_list` | 列出插件 manifest、校验状态和加载策略 |
| `PluginValidateTool` | `plugin_validate` | 校验插件 manifest、入口和 CETP 握手 |
| `BackgroundStatusTool` | `background_status` | 查询作业状态、退出码和有界 stdout/stderr |
| `BackgroundCancelTool` | `background_cancel` | 取消排队或运行中的命令并等待其停止 |
| `GitOperationsTool` | `git_operations` | Git 操作 |
| `PdfReadTool` | `pdf_read` | 读取 PDF 文件 |

`shell` 和 `background_run` 使用同一套命令执行器，接受 `backend`、`timeout_secs` 和
`max_output_bytes`。`backend` 默认为 `host`（Android 上为宿主 shell），`android` 是
`host` 的别名；`linux` 进入已安装的 tawcroot/PRoot 环境，优先 tawcroot，启动失败回退 PRoot，未就绪时返回错误，不回退到宿主。
原有仅传 `command` 的调用保持宿主行为，`linux_exec` 保留为固定 Linux 后端的兼容入口。
`eval_javascript` 继续使用内置 QuickJS。

同步执行默认 30 秒、最长 300 秒；兼容入口 `linux_exec` 保留原先的 60 秒默认值。
后台任务默认 300 秒、最长 3600 秒。每个输出流默认保留 1 MiB，可通过
`max_output_bytes` 设置为 1024 字节至 4 MiB。Linux 等待共享执行容量的时间也计入执行预算，
等待期间支持取消；执行中的 PRoot 先接收 SIGQUIT 清理 tracees，tawcroot 和 fallback 都清理残留进程组。
Android 启动 PRoot 前仅在子进程中恢复 SIGQUIT 的处理及屏蔽状态，避免继承 ART
的信号屏蔽导致取消无效；包含分离进程组的 tracees 也纳入真机回归。

后台命令共享进程内队列，最多同时运行 2 个作业并排队 16 个作业。
文件配置对应的后台结果会落盘；重启后原活动任务标记为中断，不自动重跑。
已完成记录最多保留 24 小时、64 条及合计 16 MiB 输出，在查询和状态变化时清理。
`background_status` 不传 `job_id` 时返回活动数量和最近任务元数据。
状态快照和完成事件包含实际 `backend`，`background_status` 和 `background_cancel`
同时适用于两种后端。后台任务与同步命令经过相同的命令白名单和敏感路径检查。
Gateway 重启后恢复历史结果，旧作业不会自动继续执行。所有 Linux 命令合计最多同时执行 2 条。

例如，调用 `shell` 同步运行 Python：

```json
{"backend":"linux","command":"python3 report.py","timeout_secs":60}
```

调用 `background_run` 提交较长任务，然后使用返回的 `job_id` 查询或取消：

```json
{"backend":"linux","command":"node process.js","timeout_secs":1800}
```

### 工具

| 工具 | 名称 | 描述 |
|------|------|------|
| `CalculatorTool` | `calculator` | 数学计算（25+ 函数） |
| `JavaScriptTool` | `eval_javascript` | 沙箱化 QuickJS ES2020 执行，限制运行时间和输出大小 |
| `LlmTaskTool` | `llm_task` | LLM 子任务 |
| `KnowledgeTool` | `knowledge` | 知识库查询 |
| `AskUserTool` | `ask_user` | 请求当前用户确认、选择或输入短文本 |

`ask_user` 支持 `confirm`、`single_select`、`multi_select` 和 `text` 四类问题。Gateway 请求绑定会话、回合和工具调用 ID，只接受第一次有效回答。待回答问题五分钟后过期，并随 Agent 回合取消。工具拒绝索取密码、API Key、访问令牌和支付信息。交互式 CLI 使用终端输入，非交互入口返回 `interaction_unavailable`。
| `ModelRoutingConfigTool` | `model_routing_config` | 模型路由配置 |
| `BackupTool` | `backup` | 备份管理 |

`csv_process` 的 `operation` 支持 `inspect`、`read`、`query`、`write` 和 `append`。读取与查询支持工作区相对路径、UTF-8（可带 BOM）、引号字段、字段内换行、自定义单字符分隔符和分页；`query` 还支持选择列、多个条件过滤以及按列排序。`write` 接受 `headers` 和数组或对象形式的 `rows` 并覆盖写入文件；`append` 读取现有文件表头并将数组或对象形式的 `rows` 追加到文件末尾。两种写入操作都只允许写入工作区，并使用原子替换和进程内串行保护；输入文件最多 20 MiB、20 万条记录，单次返回最多 1000 行且返回 JSON 不超过 1 MiB。

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
   - `skill` / `skill_create` — 仅当 `config.skills.enabled` 为 true
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

## 画像管理工具

启用画像存储后，注册表会加入 `user_profile_search`、`user_profile_change_plan`、`user_profile_apply_plan` 和 `user_profile_undo`。搜索只读；写操作先返回结构化 `ContentBlock::Profile` 预览，包含变更前后值和确认动作。应用时校验身份、版本、TTL、条目上限和单次执行状态，成功后返回可撤销的 operation ID。

这些工具不接受模型可控的 `user_id` 参数，而是从 `ToolContext` 读取认证 `UserContext`。结构化 presentation 只供客户端展示；普通 `ToolResult.output` 仍是提供给模型的结果。
