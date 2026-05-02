# clawseed-agent — Agent 核心运行时

## 概述

`clawseed-agent` 是 Agent 的核心 crate，负责工具调度、Hook 执行、安全策略、定时任务等。它是连接 Provider、Tool、Memory、Hook 的枢纽。

## 核心结构

### Agent — Agent 注册中心

```rust
pub struct Agent {
    tools: Vec<Box<dyn Tool>>,
    hooks: HookRunner,
    provider: Box<dyn Provider>,
    memory: Arc<dyn Memory>,
    observer: Box<dyn Observer>,
    tool_dispatcher: Box<dyn ToolDispatcher>,
    capabilities: HashMap<TypeId, Arc<dyn Any + Send + Sync>>,
    workspace_dir: PathBuf,
    // ...
}
```

Agent 是一个**注册中心**，持有所有工具、Hook、Provider 等的 `Vec<Box<dyn Trait>>`。核心代码永不修改，扩展只需向向量中添加条目。

### AgentBuilder — 构建器

```rust
let agent = Agent::builder()
    .provider(provider)
    .tools(tools)
    .memory(memory)
    .observer(observer)
    .tool_dispatcher(dispatcher)
    .workspace_dir(path)
    .capability(Arc::new(security_policy))
    .build()?;
```

## 模块架构

### agent_loop.rs — Agent 循环入口

提供 Gateway 兼容的 `turn()` 调用接口：

1. 接收用户消息
2. 构建系统提示（调用 `prompt.rs`）
3. 发送至 LLM
4. 解析响应
5. 若包含工具调用 → 进入工具循环
6. 返回最终文本响应

### tool_loop.rs — 工具循环

管理工具循环的执行流程：

1. 解析 LLM 响应中的工具调用
2. 对每个工具调用执行 before_hook
3. 执行工具
4. 执行 after_hook
5. 格式化结果，发送回 LLM
6. 重复直到 LLM 返回纯文本

### tool_execution.rs — 单次工具执行

```rust
pub fn find_tool(agent: &Agent, name: &str) -> Option<&Box<dyn Tool>>
pub async fn execute_one_tool(agent: &Agent, name: &str, args: Value, ctx: &dyn ToolContext) -> Result<ToolResult>
```

- `find_tool()` — O(n) 名称查找（工具集小，可接受）
- `execute_one_tool()` — 包装工具执行，附带 Observer 事件记录、耗时测量、错误处理、取消支持

### dispatcher.rs — 工具调度器

两种实现：

| 调度器 | 适用场景 | 工作方式 |
|--------|---------|---------|
| `NativeToolDispatcher` | 支持原生工具调用的 Provider | 直接从响应中提取 `tool_calls` |
| `XmlToolDispatcher` | 不支持原生工具调用的 Provider | 从文本中解析 ◁▷ 标记包裹的 JSON |

### context.rs — 工具上下文

```rust
pub struct AgentToolContext {
    workspace_dir: PathBuf,
    capabilities: HashMap<TypeId, Arc<dyn Any + Send + Sync>>,
}

impl ToolContext for AgentToolContext {
    fn workspace_dir(&self) -> &Path { &self.workspace_dir }
    fn get<T: 'static>(&self) -> Option<&T> {
        self.capabilities.get(&TypeId::of::<T>())
            .and_then(|arc| arc.downcast_ref::<T>())
    }
}
```

### hooks.rs — Hook 运行器

```rust
pub struct HookRunner {
    hooks: Vec<Box<dyn Hook>>,
}
```

执行流程：
1. `run_before_tool_call()` — 按顺序遍历 Hook
   - 第一个返回 `Cancel` 的 Hook 停止管线
   - `Modify` 修改后的调用传递给下一个 Hook
2. `fire_after_tool_call()` — 通知所有 Hook，仅观察，不修改

### security/ — 安全策略

- `mod.rs` — `SecurityPolicy` 结构体
  - 自主等级（ReadOnly / Supervised / Full）
  - 命令白名单
  - 中等风险命令列表（touch, rm, cp, mv, mkdir, chmod, chown, kill）
  - 路径限制（`/etc/passwd`, `/etc/shadow`, `/etc/ssh`, `/root/.ssh`）
  - 操作速率限制（`max_actions_per_hour`）
- `pairing.rs` — `PairingGuard`，设备配对验证，使用常量时间比较
- `secrets.rs` — `SecretStore` 凭证管理，`WebAuthnManager` 支持

### cron/ — 定时任务

- `scheduler.rs` — 任务执行引擎，跟踪任务和运行历史
- `store.rs` — 持久化存储
  - `add_agent_job()` / `add_shell_job()` — 创建定时任务
  - `update_job()` / `remove_job()` — 修改/删除
  - `list_jobs()` / `due_jobs()` — 查询
  - `record_run()` / `list_runs()` — 运行历史
  - `sync_declarative_jobs()` — 从配置文件同步
- `types.rs` — `CronJob`、`Schedule`、`SessionTarget`、`DeliveryConfig`
- `mod.rs` — 安全集成
  - `validate_shell_command()` — 安全策略检查
  - `add_shell_job_with_approval()` — 审批后持久化

### 其他模块

| 模块 | 职责 |
|------|------|
| `cost.rs` | 令牌计费追踪 |
| `observer.rs` | 事件发射（默认 no-op） |
| `approval.rs` | 危险操作的审批工作流 |
| `history.rs` | 对话历史管理 |
| `prompt.rs` | 系统提示构建 |
| `health.rs` | 健康检查存根 |
