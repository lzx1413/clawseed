# clawseed-agent — Agent 核心运行时

## 概述

`clawseed-agent` 是 Agent 的核心 crate，负责工具调度、Hook 执行、安全策略、定时任务等。它是连接 Provider、Tool、Memory、Hook 的枢纽。

> **注意：** 除了编排，此 crate 还承担**运行时装配**职责。`Agent::from_config_with_registry()` 直接实例化 provider（通过 `ProviderFactoryRegistry`）、memory（通过 `clawseed_memory::create_memory()`）和 tools（通过 `clawseed_tools::registry::all_tools()`），然后根据 `provider.supports_native_tools()` 选择调度器。tools 依赖 memory 先构造完成；调度器依赖 provider 能力。

## 核心结构

### Agent — Agent 注册中心

```rust
pub struct Agent {
    provider: Box<dyn Provider>,
    tool_registry: Arc<dyn ToolRegistry>,
    memory: Arc<dyn Memory>,
    observer: Arc<dyn Observer>,
    tool_dispatcher: Box<dyn ToolDispatcher>,
    capabilities: HashMap<TypeId, Arc<dyn Any + Send + Sync>>,
    workspace_dir: PathBuf,
    // ...
}
```

Agent 是一个**注册中心**，通过 `ToolRegistry` trait 统一管理所有来源的工具（内置、MCP、远程），通过 `HookRunner` 管理 Hook 管线。核心代码不感知具体工具实现，扩展只需向注册表中添加条目。

> **MCP 注意事项：** `crates/clawseed-agent/src/tools.rs` 中的所有 MCP 类型（`McpRegistry`、`DeferredMcpToolSet`、`McpToolWrapper`、`ToolSearchTool`）都是 **stub**——返回空集合或错误。`ToolSource::Mcp` 枚举变体和 `McpConfig` schema 已存在，但没有实际的 MCP 协议客户端。请勿将 MCP 视为可用能力。

### AgentBuilder — 构建器

```rust
let agent = Agent::builder()
    .provider(provider)
    .tools(tools)                    // 方式一：传入工具列表，自动构建 DefaultToolRegistry
    .tool_registry(registry)         // 方式二：传入预构建的 ToolRegistry（优先级更高）
    .memory(memory)
    .observer(observer)
    .tool_dispatcher(dispatcher)
    .workspace_dir(path)
    .capability(Arc::new(security_policy))
    .allowed_tools(Some(vec!["file_*".into()]))   // glob 模式工具白名单
    .denied_tools(Some(vec!["shell".into()]))      // glob 模式工具黑名单
    .mcp_tool_filters(Some(filters))               // 按 MCP 服务器过滤
    .hook_runner(Some(Arc::new(hook_runner)))       // Hook 管线
    .build()?;

// 从配置文件构建（可选传入自定义 ProviderFactoryRegistry）
let agent = Agent::from_config(&config).await?;
let agent = Agent::from_config_with_registry(&config, Some(provider_factory_registry)).await?;
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

- 工具通过 `tool_registry.get_tool(name)` 查找（返回 `Arc<dyn Tool>`，O(1) 哈希查找）
- 包装工具执行，附带 Observer 事件记录、耗时测量、错误处理、取消支持

### dispatcher.rs — 工具调度器

两种实现：

| 调度器 | 适用场景 | 工作方式 |
|--------|---------|---------|
| `NativeToolDispatcher` | 支持原生工具调用的 Provider | 直接从响应中提取 `tool_calls` |
| `XmlToolDispatcher` | 不支持原生工具调用的 Provider | 先尝试 ◁▷ 格式，失败后 fallback 到多格式解析器 |

### parser.rs — 工具调用解析器

多格式工具调用解析，支持 12+ 种 LLM 输出格式：

- OpenAI 原生 JSON `tool_calls` 数组
- XML 标签：`<tool_call>`、`<toolcall>`、`<tool-call>`、`<invoke>`
- MiniMax `<invoke>` 格式
- Markdown 代码块（` ```tool_call `）
- Anthropic `<FunctionCall>` 标签
- GLM 缩短格式
- Perl/哈希引用风格
- xAI grok ` ```tool <name> ` 格式

`XmlToolDispatcher::parse_response()` 先尝试 ◁▷ 格式（prompt 引导的确定性解析），失败后调用 `parser::parse_tool_calls()` 作为 fallback，用原始响应文本尝试多格式解析。

**安全设计**：不提取无显式包裹的原始 JSON，防止提示注入攻击。

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

**声明式 Hook 链**：通过配置文件中的 `[hooks]` 段声明 Hook 链，`HookFactoryRegistry` 根据 `hook_type` 创建 Hook 实例。`from_config()` 中 `SecurityPolicy` 始终作为管线的第一个 Hook 自动注册。

```rust
pub trait HookFactory: Send + Sync {
    fn hook_type(&self) -> &str;
    fn create(&self, config: &serde_json::Value) -> Option<Box<dyn Hook>>;
}
```

### tool_registry.rs — 工具注册表实现

`DefaultToolRegistry` 是 `ToolRegistry` trait 的默认实现：

- 使用 `DashMap` 实现无锁并发访问，在 async 上下文中安全使用
- `ToolSpec` 缓存 + 写时失效，避免重复计算
- 支持 glob 模式的三层过滤：denied 优先 → allowed 白名单 → MCP 服务器级过滤
- `register_all()` 批量注册、`register_all_arc()` 使用共享 `Arc<dyn Tool>` 实例批量注册（避免网关场景下重复构造）、`unregister_by_source()` 按来源批量移除

> **双重注册表注意：** 运行时存在两个独立的 `ToolRegistry` 实例。网关级 `AppState.tool_registry` 用于 `/api/tools` 端点可见性；每个 Agent 的 `tool_registry` 用于实际工具调度。远程工具必须在两者中都注册。详见[架构概览](../architecture.md)中的"双重工具注册表"一节。

### security/ — 安全策略

- `mod.rs` — `SecurityPolicy` 结构体
  - 自主等级（ReadOnly / Supervised / Full）
  - 命令白名单
  - 中等风险命令列表（touch, rm, cp, mv, mkdir, chmod, chown, kill）
  - 路径限制（`/etc/passwd`, `/etc/shadow`, `/etc/ssh`, `/root/.ssh`）
  - 操作速率限制（`max_actions_per_hour`）
  - **实现 `Hook` trait**：`before_tool_call()` 检查自主等级、速率限制、命令白名单和路径守卫；`after_tool_call()` 记录操作计数。SecurityPolicy 始终作为 Hook 管线的第一个 Hook 注册，不再作为 Capability 注入
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

### prompt.rs — 模块化系统提示构建器

系统提示通过可插拔的 `PromptSection` 实现由 `SystemPromptBuilder` 组装：

```
SystemPromptBuilder::with_defaults()
  ├── DateTimeSection       — 当前日期和时间
  ├── IdentitySection       — AIEOS 身份 + 人格 Markdown 文件
  ├── WorkspaceSection      — 工作目录路径
  ├── ToolsSection          — 可用工具描述
  ├── SafetySection         — 安全规则（感知自主等级）
  └── ToolHonestySection    — 工具诚实性约束
```

可通过 `SystemPromptBuilder::add_section()` 添加自定义分节。

### personality.rs — 人格文件加载器

从工作区目录加载预定义的 Markdown 文件：

| 文件 | 用途 |
|------|------|
| `SOUL.md` | 核心人格和行为准则 |
| `IDENTITY.md` | 名称、角色、背景 |
| `USER.md` | 用户偏好和上下文 |
| `AGENTS.md` | 多 Agent 协调规则 |
| `TOOLS.md` | 工具使用指南 |
| `HEARTBEAT.md` | 定期自检指令 |
| `BOOTSTRAP.md` | 首次运行初始化指令 |
| `MEMORY.md` | 记忆管理指南 |

每个文件在 20K 字符处截断。首次运行时自动生成默认的 `SOUL.md`。

### identity.rs — AIEOS 身份系统

支持 AIEOS v1.1（AI Entity Object Specification）— 一种用于可移植 AI 身份的结构化 JSON 格式。涵盖身份、心理学、语言学、动机、能力、外貌、历史和兴趣。

- `load_aieos_identity()` — 从文件或内联 JSON 加载
- `aieos_to_system_prompt()` — 将 AIEOS 身份渲染为 Markdown
- 通过归一化处理官方生成器输出和简化 JSON 格式

详细文档参见[人格与身份教程](../tutorials/personality-and-identity.md)。

### 其他模块

| 模块 | 职责 |
|------|------|
| `cost.rs` | 令牌计费追踪 |
| `observer.rs` | 事件发射（默认 NoopObserver，定义在 clawseed-agent 本地） |
| `observability.rs` | 重新导出 Observer 类型供外部消费者使用 |
| `approval.rs` | 危险操作的审批工作流 |
| `history.rs` | 对话历史管理 |
| `parser.rs` | 多格式工具调用解析（12+ 种 LLM 输出格式） |
| `health.rs` | 健康检查存根 |
