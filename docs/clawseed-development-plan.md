# ClawSeed 开发计划

## 1. 项目概述

### 1.1 定位

ClawSeed 是面向 Android app 的本地 AI agent 框架，替代 zeroseed。基于 zeroclaw 代码库重新设计架构，保留核心能力（LLM 调用、工具执行、记忆、定时任务、HTTP/WS 网关），去除非必要子系统。

### 1.2 与 zeroclaw 的关系

| 维度 | zeroclaw | clawseed |
|------|----------|----------|
| 定位 | 全功能 AI 助手（PC + 硬件 + 浏览器 + 企业集成） | Android 端轻量 agent 框架 |
| 代码量 | ~225K 行 | 目标 ~55K 行 |
| crate 数 | 10 + 二进制 | 9 + 二进制 |
| runtime | 79K 行巨无霸 | 拆分为 agent (~10K) + tools (~10K) |
| config | 26K 行枢纽 | ~5K 行（含 CostConfig） |
| 安全 | 13 种沙箱 + WebAuthn + OTP | ToolContext 安全查询 + approval |
| 浏览器/硬件 | browser/text_browser/hardware_board/hardware_memory | 不需要 |

### 1.3 Android 功能路径（红线）

```
clawseed (binary, --gateway)
  → clawseed-gateway (HTTP/WS, agent turn, remote tool bridge)
    → clawseed-agent (agent loop, tool dispatch, cron, cost)
      → clawseed-tools (file, web, memory, calc, shell, cron 等)
      → clawseed-providers (compatible, anthropic, gemini, bedrock)
      → clawseed-memory (SQLite)
    → clawseed-config (TOML 加载)
    → clawseed-api (trait 定义)
```

### 1.4 代码质量原则：简洁至上

zeroclaw 代码库中存在大量 dead code 和 unused code（105 处 `#[allow(dead_code/unused)]`、599 个 runtime 公开函数中多数无调用方）。clawseed 实现时必须避免重蹈覆辙：

| 原则 | 说明 | 反例（zeroclaw 中的问题） |
|------|------|--------------------------|
| **禁止 allow 指令** | 不允许 `#[allow(dead_code)]`、`#[allow(unused_imports)]`，编译器警告必须修复而非压制 | 105 处 allow 指令掩盖了未使用的代码 |
| **只写被调用的代码** | 每个函数/struct/模块必须有明确的调用方，无调用方的代码不写 | runtime 中 599 个 pub fn，大量无外部调用 |
| **最小公开面** | 默认 `pub(crate)`，只在跨 crate 边界时用 `pub` | 大量不必要的 `pub` 字段和函数 |
| **不过度抽象** | 不为"将来可能需要"写 trait 或泛型，只为当前实际需求 | 为未实现的 JWKS 验证预写了 async trait 并 allow(unused_async) |
| **不迁移 dead code** | 从 zeroclaw 复用代码时，只复制被实际使用的部分，跳过未使用的辅助函数/分支 | loop_/mod.rs 4304 行中大量分支对应已删子系统 |
| **clippy 严格模式** | `cargo clippy -- -D warnings` 作为 CI 门禁，零容忍 | zeroclaw 无法通过此检查 |

**实现时的检查清单（每个模块完成后）：**

```bash
# 1. 无 allow(dead_code/unused) 残留
grep -rn "#\[allow" --include="*.rs" crates/clawseed-*/src/ | grep -i "dead_code\|unused"

# 2. clippy 零警告
cargo clippy -- -D warnings

# 3. 无未使用的依赖
cargo +nightly udeps 2>/dev/null || cargo machete 2>/dev/null

# 4. pub 符号均有调用方（人工审查）
cargo doc --document-private-items 2>&1 | grep "cannot document"
```

---

## 2. 模块架构设计

### 2.1 核心设计原则：核心不 import 扩展，扩展 import 核心

zeroclaw 臃肿的根因是 **核心 import 扩展**：agent.rs 直接 `use crate::security::SecurityPolicy`、`use crate::skills::*`，每加一个功能就要改核心代码。28 个 mod 形成网状耦合。

clawseed 反过来：**扩展 import 核心**。Agent 只知道 trait（Tool、Hook、Observer、ContextProvider），不知道具体实现。加功能 = 加 crate + binary 注册一行，核心代码永远不改。

```
zeroclaw 的方式（核心 import 扩展）：
  agent.rs → use crate::security::SecurityPolicy
  agent.rs → use crate::skills::SkillManager
  tools/mod.rs → SecurityPolicy::wrap(tool)
  加一个功能 → 改 agent.rs + tools/mod.rs + lib.rs

clawseed 的方式（扩展 import 核心）：
  clawseed-security → impl Hook for SecurityHook
  clawseed-skills   → impl Hook for SkillsHook
  加一个功能 → 加一个 crate，binary 加一行 .hook()，核心零改动
```

### 2.2 三大解耦机制

#### 机制 1：Agent 是注册表，不是上帝对象

```rust
pub struct Agent {
    provider: Box<dyn Provider>,
    tools: Vec<Box<dyn Tool>>,                // 注册，不 import 具体类型
    hooks: Vec<Box<dyn Hook>>,                // 注册，不 import 具体类型
    observers: Vec<Box<dyn Observer>>,        // 注册
    context_providers: Vec<Box<dyn ContextProvider>>,  // 注册
    config: AgentConfig,
    history: Vec<ConversationMessage>,
    // 不再有：security, skills, sop, trust...
}
```

#### 机制 2：ToolContext 是能力袋，按类型查询

```rust
// 核心 trait — 永远不改
pub trait ToolContext: Send + Sync {
    fn workspace_dir(&self) -> &Path;
    fn config(&self) -> &Config;
    fn cost_tracker(&self) -> Option<&CostTracker>;

    /// 按类型查询能力。扩展自行定义类型，核心不知道
    fn get<T: 'static>(&self) -> Option<&T>;
}

// shell 工具使用：
fn execute(&self, args: Value, ctx: &dyn ToolContext) -> Result<ToolResult> {
    // 有 SecurityPolicy 就检查，没有就跳过
    if let Some(policy) = ctx.get::<SecurityPolicy>() {
        if !policy.is_command_allowed(&cmd) { return Err(...); }
    }
    // 执行命令...
}
```

`ctx.get::<SecurityPolicy>()` 中的 `SecurityPolicy` 是 clawseed-security crate 定义的类型。没有 security crate？这段代码编译不出，工具自动跳过检查。

#### 机制 3：Hook 替代直接调用

```rust
pub trait Hook: Send + Sync {
    fn before_tool_call(&self, call: &mut ToolCall) -> HookResult;
    fn after_tool_call(&self, result: &ToolExecutionResult) -> HookResult;
}

pub enum HookResult {
    Continue,
    Cancel(String),
    Modify(ToolCall),
}

// agent 执行工具时（核心代码，永远不改）：
for hook in &self.hooks {
    match hook.before_tool_call(&mut call) {
        HookResult::Cancel(msg) => return Err(msg),
        HookResult::Modify(modified) => call = modified,
        HookResult::Continue => {}
    }
}
```

SecurityPolicy 注册为 Hook（检查工具/路径权限），SOP 审批门注册为 Hook，Trust 检查注册为 Hook。**Agent 代码永远不改。**

### 2.3 架构图

```
┌──────────────────────────────────────────────────────────────┐
│                      clawseed (binary)                       │
│  选择引入哪些扩展，注册到 Agent。~2K 行                        │
│  .hook(SecurityHook::new()) .tools(skills_tools()) ...       │
└──────┬───────────────────────────┬──────────────────────────┘
       │                           │
┌──────▼──────────────┐  ┌────────▼────────────────────────────┐
│  clawseed-gateway   │  │         扩展层（按需引入）            │
│  HTTP/WS, ~6K 行    │  │                                     │
└──────┬──────────────┘  │  ┌───────────────┐ ┌─────────────┐  │
       │                 │  │ clawseed-     │ │ clawseed-   │  │
┌──────▼──────────────┐  │  │ security      │ │ skills      │  │
│  clawseed-agent     │  │  │ → Hook+Ctx   │ │ → Hook+Ctx  │  │
│  Agent+Hook注册,    │◄─┤  └───────────────┘ └─────────────┘  │
│  ~10K 行            │  │  ┌───────────────┐ ┌─────────────┐  │
└──┬───┬───┬──────────┘  │  │ clawseed-     │ │ clawseed-   │  │
   │   │   │              │  │ sop           │ │ delegate    │  │
   │   │   │              │  │ → Hook+Ctx   │ │ → Tool+Ctx  │  │
   │   │   │              │  └───────────────┘ └─────────────┘  │
   │   │   │              │  ┌───────────────┐ ┌─────────────┐  │
   │   │   │              │  │ clawseed-mcp  │ │ clawseed-   │  │
   │   │   │              │  │ → Tool+Ctx   │ │ browser     │  │
   │   │   │              │  └───────────────┘ │ → Tool+Ctx  │  │
   │   │   │              └────────────────────┴─────────────┘──┘
   │   │   │                                    都依赖 ↓
   ▼   ▼   ▼
┌────────────┐ ┌──────────┐ ┌──────────────┐ ┌─────────────┐
│ clawseed-  │ │ clawseed-│ │ clawseed-    │ │ clawseed-   │
│ tools      │ │ providers│ │ memory       │ │ config      │
│ ~11K 行    │ │ ~8K 行   │ │ ~6K 行       │ │ ~5K 行      │
└─────┬──────┘ └────┬─────┘ └──────────────┘ └─────────────┘
      │              │
      └──────┬───────┘
             ▼
     ┌──────────────┐      ┌──────────────┐
     │ clawseed-api  │      │ clawseed-    │
     │ 纯 trait      │      │ parser       │
     │ ~2.5K 行      │      │ ~3K 行       │
     └──────────────┘      └──────────────┘

     ┌──────────────┐
     │ clawseed-    │
     │ macros       │
     │ ~1K 行       │
     └──────────────┘
```

**依赖规则：**
- 核心层 → api（单向）
- 基础设施层 → 核心 → api
- 扩展层 → 核心 → api（**反向依赖被禁止**：核心不 import 扩展）
- binary → 所有需要的 crate（唯一知道扩展存在的地方）

### 2.4 依赖关系（严格 DAG，核心不 import 扩展）

```
clawseed-api          → (无内部依赖)
clawseed-macros       → (无内部依赖)
clawseed-parser       → (无内部依赖)
clawseed-config       → clawseed-api
clawseed-providers    → clawseed-api, clawseed-config
clawseed-memory       → clawseed-api, clawseed-config
clawseed-tools        → clawseed-api, clawseed-config
clawseed-agent        → clawseed-api, clawseed-config, clawseed-parser, clawseed-providers, clawseed-memory
clawseed-gateway      → clawseed-api, clawseed-config, clawseed-agent

扩展层（binary 按需引入，核心不知道它们存在）：
clawseed-security     → clawseed-api, clawseed-agent, clawseed-config
clawseed-skills       → clawseed-api, clawseed-agent, clawseed-config
clawseed-sop          → clawseed-api, clawseed-agent, clawseed-config
clawseed-delegate     → clawseed-api, clawseed-agent, clawseed-providers, clawseed-memory
clawseed-mcp          → clawseed-api, clawseed-agent
clawseed-browser      → clawseed-api, clawseed-agent

clawseed (binary)     → clawseed-gateway, clawseed-agent, clawseed-config
                         + 按需引入扩展 crate
```

### 2.5 Binary 组装示例

```rust
// clawseed binary main.rs — 唯一知道扩展存在的地方
fn build_agent(config: &Config) -> Agent {
    let mut builder = Agent::builder()
        .provider(create_provider(config))
        .memory(create_memory(config))
        .tools(builtin_tools())
        .observer(LogObserver::new());

    // 按需引入扩展——加一个功能只加一行
    #[cfg(feature = "security")]
    builder = builder
        .hook(SecurityHook::new(config))
        .context_provider(SecurityContextProvider::new(config));

    #[cfg(feature = "skills")]
    builder = builder
        .hook(SkillsHook::new(config))
        .tools(skills_tools(config));

    #[cfg(feature = "delegate")]
    builder = builder.tool(DelegateTool::new(config));

    builder.build()
}
```

### 2.6 与 zeroclaw 的架构差异

| 决策 | zeroclaw | clawseed | 原因 |
|------|----------|----------|------|
| 核心与扩展的关系 | 核心 import 扩展（agent.rs 直接 use 子系统） | 扩展 import 核心（Agent 只知道 trait） | 加功能不碰核心代码 |
| Agent 结构 | 上帝对象，持有 security/skills/sop/trust... | 注册表，只持有 Vec\<Box\<dyn Hook/Tool\>\> | 字段不随功能增长 |
| ToolContext | 不存在，工具直接依赖 runtime crate | 能力袋 `ctx.get::<T>()`，按类型查询 | 安全/技能等能力按需注入，核心不改 |
| Hook 机制 | 无，工具调用硬编码安全检查 | before/after hook 链，SecurityPolicy/SOP/Trust 都是 Hook | 行为插拔，核心循环不改 |
| 扩展引入方式 | 改 lib.rs mod + agent.rs import + tools/mod.rs | binary 加一行 `.hook()` / `.tools()` | O(1) 改动 vs O(N) 改动 |
| infra crate | 独立 crate (2.2K) | 删除，session 并入 gateway | 唯一使用者不需要抽象层 |
| tools 分裂 | zeroclaw-tools + runtime/src/tools | 统一 clawseed-tools | 消除歧义 |
| runtime 巨无霸 | 79K 行混装 | agent ~10K 行纯核心 | 核心代码量固定，不随功能增长 |
| config 枢纽 | 26K 行含 secrets/policy/autonomy | ~5K 行纯配置 | 扩展的配置放到各自扩展 crate |

### 2.7 加功能时的代码改动对比

| 操作 | zeroclaw | clawseed |
|------|----------|----------|
| 加安全检查 | 改 agent.rs + tools/mod.rs + lib.rs + 3 个 import | 加 `clawseed-security` crate，binary 加一行 `.hook()` |
| 加一个工具 | 改 tools/mod.rs 的 all_tools()，加 import | 扩展 crate 导出工具，binary 加一行 `.tools()` |
| 加一个子系统 | 改 lib.rs mod 声明、改 agent.rs import、改 prompt.rs section、改 Config struct | 实现扩展 crate 的 Hook/ContextProvider，binary 注册 |
| 去掉一个功能 | 删 import + 删调用 + 条件编译到处加 cfg | binary 删一行注册，feature flag 关闭 |

**核心差异：** zeroclaw 是"核心依赖扩展"（加功能改核心），clawseed 是"扩展依赖核心"（加功能只改 binary）。核心的代码量是固定的，不管加多少功能都不会膨胀。

---

## 3. 各模块详细设计

### 3.1 clawseed-api

**职责：** 定义所有核心 trait 和共享类型，零实现。扩展 crate 也依赖此 crate 定义自己的 trait 实现。

**公开 API：**

```rust
// provider.rs
pub trait Provider: Send + Sync {
    async fn chat(&self, request: ChatRequest) -> Result<ChatResponse>;
    async fn stream_chat(&self, request: ChatRequest) -> Result<Pin<Box<dyn Stream<Item = StreamEvent>>>>;
    fn name(&self) -> &str;
    fn model_name(&self) -> &str;
}

pub struct ChatRequest { pub messages: Vec<ChatMessage>, pub tools: Vec<ToolSpec>, pub temperature: Option<f64>, ... }
pub struct ChatResponse { pub content: String, pub tool_calls: Vec<ToolCall>, pub usage: TokenUsage, ... }
pub enum ChatMessage { User{..}, Assistant{..}, System{..}, ToolResult{..} }

// tool.rs
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn parameters_schema(&self) -> serde_json::Value;
    async fn execute(&self, args: serde_json::Value, ctx: &dyn ToolContext) -> Result<ToolResult>;
}

pub struct ToolResult { pub output: String, pub success: bool, pub error: Option<String> }
pub struct ToolSpec { pub name: String, pub description: String, pub parameters: serde_json::Value }
pub struct ToolCall { pub id: String, pub name: String, pub arguments: Value }

// tool_context.rs — 能力袋，按类型查询
pub trait ToolContext: Send + Sync {
    fn workspace_dir(&self) -> &Path;
    fn config(&self) -> &Config;
    fn cost_tracker(&self) -> Option<&CostTracker>;

    /// 按类型查询能力。核心永远不改，扩展自行定义类型。
    /// 示例：ctx.get::<SecurityPolicy>() 检查安全策略
    fn get<T: 'static>(&self) -> Option<&T>;
}

// hook.rs — 扩展行为插拔的核心机制
pub trait Hook: Send + Sync {
    fn before_tool_call(&self, call: &mut ToolCall) -> HookResult;
    fn after_tool_call(&self, result: &ToolExecutionResult) -> HookResult;
}

pub enum HookResult {
    Continue,
    Cancel(String),
    Modify(ToolCall),
}

// context_provider.rs — 扩展 ToolContext 能力的注册机制
pub trait ContextProvider: Send + Sync {
    /// 提供能力对象，AgentToolContext.get::<T>() 通过此方法查询
    fn as_any(&self) -> &dyn Any;
}

// observer.rs
pub trait Observer: Send + Sync {
    fn emit(&self, event: ObserverEvent);
}

// memory_traits.rs
pub trait Memory: Send + Sync {
    async fn store(&self, key: &str, value: &str, category: MemoryCategory) -> Result<()>;
    async fn recall(&self, query: &str, limit: usize) -> Result<Vec<MemoryEntry>>;
    async fn forget(&self, key: &str) -> Result<()>;
    async fn export(&self, filter: ExportFilter) -> Result<Vec<MemoryEntry>>;
}
```

**参考 zeroclaw 源文件：**

| 文件 | 行数 | 说明 |
|------|------|------|
| `crates/zeroclaw-api/src/lib.rs` | 2,910 | 整体结构 |
| `crates/zeroclaw-api/src/provider.rs` | ~730 | Provider trait + ChatMessage/Response 类型 |
| `crates/zeroclaw-api/src/tool.rs` | ~43 | Tool trait + ToolResult/ToolSpec |
| `crates/zeroclaw-api/src/memory_traits.rs` | ~300 | Memory trait + MemoryEntry |
| `crates/zeroclaw-api/src/observability_traits.rs` | ~324 | Observer trait |

**差异：**
- 新增 `Hook` trait + `HookResult` enum（zeroclaw 中 hook 硬编码在 agent 里，现在提升为 api 层抽象）
- 新增 `ContextProvider` trait（扩展 ToolContext 能力的注册机制）
- `ToolContext` 用 `ctx.get::<T>()` 能力袋替代固定方法列表（`is_tool_allowed`/`is_path_allowed`/`build_shell_command` 不再是 ToolContext 的方法，改为扩展 crate 中通过 `ctx.get::<SecurityPolicy>()` 查询）
- 删除 `Channel`、`Peripheral`、`RuntimeAdapter` trait（Android 不需要，其功能由 ToolContext + ContextProvider 承接）
- 保留 task_local 定义（TOOL_LOOP_SESSION_KEY 等）
- **dead code 清理**：删除 zeroclaw-api 中仅为已删子系统（Channel/Peripheral）定义的类型和 re-export

**预估行数：** ~2,500

**测试策略：** 编译期验证（trait 定义无需单元测试），通过下游 crate 的集成测试覆盖。

---

### 3.2 clawseed-macros

**职责：** proc macro crate，提供 Configurable derive macro。

**参考 zeroclaw 源文件：**

| 文件 | 行数 | 说明 |
|------|------|------|
| `crates/zeroclaw-macros/src/lib.rs` | 851 | Configurable derive macro |

**预估行数：** ~800（基本复用）

**测试策略：** proc macro 测试——编译期验证 + 生成代码的正确性测试。

---

### 3.3 clawseed-parser

**职责：** 解析 LLM 返回的 tool call 文本，支持多种格式。纯文本变换，零依赖运行时状态。

**参考 zeroclaw 源文件：**

| 文件 | 行数 | 说明 |
|------|------|------|
| `crates/zeroclaw-tool-call-parser/src/lib.rs` | 2,773 | 全部实现 + 102 个测试 |

**支持的格式：**
- OpenAI native JSON `tool_calls` 数组
- XML 标签：`<tool_call>`、`<toolcall>`、`<invoke>`
- Markdown code block：` ```tool_call `
- Anthropic `<FunctionCall>` 标签
- GLM/MiniMax/Perl 风格

**预估行数：** ~2,800（基本复用，仅改标识符命名）

**测试策略：** 复用 zeroclaw 的 102 个测试用例，逐一对齐。

---

### 3.4 clawseed-config

**职责：** TOML 配置文件加载、解析、环境变量覆盖。不含 secrets/policy/autonomy/pairing。

**公开 API：**

```rust
pub struct Config {
    pub workspace_dir: PathBuf,
    pub providers: ProvidersConfig,
    pub agent: AgentConfig,
    pub gateway: GatewayConfig,
    pub storage: StorageConfig,
    pub scheduler: SchedulerConfig,
    pub reliability: ReliabilityConfig,
}

impl Config {
    pub fn load() -> Result<Self>;
    pub fn from_file(path: &Path) -> Result<Self>;
    pub fn from_env() -> Result<Self>;        // 环境变量覆盖
    pub fn resolve_provider(&self) -> Result<ResolvedProvider>;
}

pub struct ProvidersConfig { ... }
pub struct AgentConfig { pub max_tool_iterations: usize, pub temperature: Option<f64>, ... }
pub struct GatewayConfig { pub host: String, pub port: u16, pub timeout_secs: u64, ... }
pub struct StorageConfig { pub backend: String, pub db_url: Option<String>, ... }
```

**环境变量覆盖（CLAWSEED_* 前缀）：**

```
CLAWSEED_PROVIDER          — 强制指定 provider
CLAWSEED_MODEL             — 强制指定 model
CLAWSEED_API_KEY           — API key
CLAWSEED_PROVIDER_URL      — provider base URL
CLAWSEED_PROVIDER_TIMEOUT_SECS
CLAWSEED_GATEWAY_HOST
CLAWSEED_GATEWAY_PORT
CLAWSEED_WORKSPACE
CLAWSEED_EXTRA_HEADERS
CLAWSEED_WEB_SEARCH_ENABLED
CLAWSEED_WEB_SEARCH_PROVIDER
```

**参考 zeroclaw 源文件：**

| 文件 | 行数 | 说明 |
|------|------|------|
| `crates/zeroclaw-config/src/schema/mod.rs` | 608 | Config 结构定义 |
| `crates/zeroclaw-config/src/schema/core_impl.rs` | 1,529 | 配置加载 + 环境变量覆盖 |
| `crates/zeroclaw-config/src/schema/defaults.rs` | 511 | 默认值 |
| `crates/zeroclaw-config/src/schema/providers.rs` | 190 | provider 配置 |
| `crates/zeroclaw-config/src/schema/agent.rs` | 280 | agent 配置 |
| `crates/zeroclaw-config/src/schema/gateway.rs` | 1,018 | gateway 配置 |
| `crates/zeroclaw-config/src/schema/storage.rs` | 429 | storage 配置 |
| `crates/zeroclaw-config/src/helpers.rs` | 236 | 辅助函数 |
| `crates/zeroclaw-config/src/providers.rs` | 69 | provider 别名 |
| `crates/zeroclaw-config/src/provider_aliases.rs` | 128 | provider 路由 |
| `crates/zeroclaw-config/src/workspace.rs` | 383 | workspace 解析 |
| `crates/zeroclaw-config/src/migration.rs` | 369 | 配置迁移 |

**差异：**
- 删除 `secrets.rs`（905 行，ChaCha20 加密密钥存储）
- 删除 `policy/`（1,842 行，SecurityPolicy 定义，安全查询已移入 ToolContext 能力袋，具体实现在 clawseed-security 扩展 crate）
- 删除 `autonomy.rs`、`pairing.rs`、`domain_matcher.rs`
- 删除 `schema/channels.rs`（1,528 行）、`schema/hardware.rs`、`schema/security.rs`、`schema/sop.rs`、`schema/tunnels.rs`、`schema/multimedia.rs`、`schema/enterprise.rs`
- 删除 `schema/tests.rs`（6,531 行，大量测试属于已删子系统）
- 环境变量前缀 `ZEROCLAW_` → `CLAWSEED_`
- **dead code 清理**：zeroclaw-config 中 20 处 `#[allow(unused_imports)]` 全部删除，每个 schema 文件只保留 clawseed 实际使用的字段/方法；`core_impl.rs` 中仅为已删子系统服务的环境变量读取逻辑不迁移
- **CostTracker 归属**：zeroclaw 中 `CostTracker` 定义在 `zeroclaw-config/src/cost/tracker.rs`（566 行）+ `cost/types.rs`（193 行），共 763 行。clawseed 将 `CostConfig` 保留在 config crate（作为配置 schema），`CostTracker` 实现移入 `clawseed-agent`
- **扩展的配置放到扩展 crate**：SecurityConfig/SkillsConfig/SopConfig 等不放在 clawseed-config，由各自的扩展 crate 定义和加载

**预估行数：** ~4,000

**测试策略：**
- 复用 `core_impl.rs` 中环境变量覆盖的测试逻辑（改前缀为 CLAWSEED_）
- 新增：配置加载/序列化 round-trip 测试
- 新增：环境变量优先级测试（CLAWSEED_PROVIDER > config file > default）
- 目标：30+ 测试用例

---

### 3.5 clawseed-providers

**职责：** LLM provider 实现，支持多种 API 协议。

**保留的 provider：**

| Provider | 说明 | 参考文件 | 行数 |
|----------|------|----------|------|
| `compatible/` | 通用 OpenAI 兼容协议 | `crates/zeroclaw-providers/src/compatible/mod.rs` | 2,339 |
| `anthropic.rs` | Claude 系列（原生协议） | `crates/zeroclaw-providers/src/anthropic.rs` | 2,131 |
| `gemini.rs` | Google Gemini（多 auth） | `crates/zeroclaw-providers/src/gemini.rs` | 2,294 |
| `bedrock.rs` | AWS Bedrock | `crates/zeroclaw-providers/src/bedrock.rs` | 1,979 |

**保留的基础设施：**

| 文件 | 说明 | 行数 |
|------|------|------|
| `reliable.rs` | 重试/fallback 包装 | 2,994 |
| `options.rs` | Provider 配置选项 | 387 |
| `multimodal.rs` | 多模态（图片）支持 | 935 |
| `router.rs` | Provider 路由 | 1,108 |
| `auth/mod.rs` | Auth 服务 | 574 |
| `auth/anthropic_token.rs` | Anthropic token | 86 |
| `auth/gemini_oauth.rs` | Gemini OAuth | 601 |
| `auth/openai_oauth.rs` | OpenAI OAuth | 438 |
| `auth/oauth_common.rs` | OAuth 公共逻辑 | 183 |
| `auth/profiles.rs` | Auth profiles | 716 |
| `aliases.rs` | Provider 别名 | ~200 |
| `registry.rs` | Provider 注册表 | ~300 |

**参考 zeroclaw 源文件：** 上表所列全部文件。

**差异：**
- 删除 `openai.rs`、`azure_openai.rs`、`ollama.rs`、`openrouter.rs`（均为 compatible 变体）
- 删除 `openai_codex.rs`、`telnyx.rs`、`glm.rs`、`kilocli.rs`、`gemini_cli.rs`、`claude_code.rs`、`copilot.rs`、`models_dev.rs`
- `factory.rs` + `registry.rs` + `router.rs` + `aliases.rs` 合并为 `registry.rs`
- 兼容 provider 注册简化：所有 OpenAI-compatible 变体走 `compatible/` 的配置路由
- `clawseed-api` 中需保留 `SchemaCleanr` 模块（844 行），`compatible/` 依赖它做跨 provider schema 标准化
- **dead code 清理**：`factory.rs` 中 1 处 `#[allow(unused_imports)]` 删除；`reliable.rs` 2,994 行中大量 fallback/重试策略分支只保留 Android 实际需要的（简单重试 + 模型降级），删除 circuit breaker 等企业级逻辑；`multimodal.rs` 935 行精简为 Android 需要的图片输入支持（camera/相册）
- **Auth 精简**：`auth/` 模块 3,016 行中，Android 不支持浏览器 OAuth 跳转，保留 API key 认证 + Anthropic token，精简 OAuth 流程

**预估行数：** ~8,000

**测试策略：**
- 复用 zeroclaw 中 `compatible/mod.rs` 的 118 个测试
- 复用 `anthropic.rs` 的 52 个、`gemini.rs` 的 53 个、`bedrock.rs` 的 52 个测试
- 复用 `reliable.rs` 的 34 个测试
- 使用 `wiremock` 做 HTTP mock 测试
- 目标：300+ 测试用例

---

### 3.6 clawseed-memory

**职责：** 记忆存储与检索，SQLite 后端。

**保留的后端：**

| 后端 | 参考文件 | 行数 | 说明 |
|------|----------|------|------|
| SQLite | `crates/zeroclaw-memory/src/sqlite.rs` | 2,764 | 主力后端 |
| None | `crates/zeroclaw-memory/src/none.rs` | 95 | 禁用记忆 |
| Namespaced | `crates/zeroclaw-memory/src/namespaced.rs` | 232 | 命名空间隔离 |

**保留的辅助模块：**

| 模块 | 参考文件 | 行数 | 说明 |
|------|----------|------|------|
| lib.rs | `crates/zeroclaw-memory/src/lib.rs` | 775 | 后端构建 + 选择 |
| retrieval.rs | `crates/zeroclaw-memory/src/retrieval.rs` | 266 | 检索管道 |
| embeddings.rs | `crates/zeroclaw-memory/src/embeddings.rs` | 358 | 向量嵌入 |
| chunker.rs | `crates/zeroclaw-memory/src/chunker.rs` | 377 | 文本分块 |
| vector.rs | `crates/zeroclaw-memory/src/vector.rs` | 403 | 向量操作 |
| decay.rs | `crates/zeroclaw-memory/src/decay.rs` | 151 | 记忆衰减 |
| importance.rs | `crates/zeroclaw-memory/src/importance.rs` | 107 | 重要性评分 |
| backend.rs | `crates/zeroclaw-memory/src/backend.rs` | 185 | 后端 trait |

**删除的模块：**

| 模块 | 行数 | 原因 |
|------|------|------|
| `postgres.rs` | 509 | Android 不需要 |
| `qdrant.rs` | 669 | Android 不需要 |
| `lucid.rs` | 724 | 高性能后端，Android 不需要 |
| `markdown.rs` | 399 | 人可读后端，Android 不需要 |
| `knowledge_graph.rs` | 863 | Android 不需要 |
| `knowledge_graph_pg.rs` | 318 | Android 不需要 |
| `consolidation.rs` | 239 | 非核心 |
| `conflict.rs` | 174 | 非核心 |
| `hygiene.rs` | 586 | 非核心 |
| `audit.rs` | 293 | 非核心 |
| `snapshot.rs` | 470 | 非核心 |
| `response_cache.rs` | 526 | 非核心 |
| `policy.rs` | 198 | 非核心 |

**预估行数：** ~6,000

**测试策略：**
- 复用 `sqlite.rs` 中的测试（CRUD、检索、衰减、命名空间隔离）
- 新增：SQLite 后端 round-trip 测试（store → recall → forget → verify absent）
- 新增：并发读写安全测试（tokio 多任务并行 store/recall）
- 目标：40+ 测试用例

---

### 3.7 clawseed-tools

**职责：** 所有内置工具实现。统一管理，不分两处。

**设计原则：** 工具只依赖 `clawseed-api` 中定义的 trait（Tool、ToolContext），不依赖 agent/providers/memory crate。需要 LLM 调用或记忆访问的工具通过 `ToolContext` 的 `ctx.get::<T>()` 能力袋获取。这降低了依赖深度（tools 从第 3 层降到第 2 层），也让工具能在不同 agent 配置中复用。

**保留的工具：**

| 工具 | 参考文件 | 行数 | 说明 |
|------|----------|------|------|
| file_edit.rs | `crates/zeroclaw-tools/src/file_edit.rs` | 827 | 文件编辑 |
| file_write.rs | `crates/zeroclaw-tools/src/file_write.rs` | 584 | 文件写入 |
| glob_search.rs | `crates/zeroclaw-tools/src/glob_search.rs` | 428 | 文件搜索 |
| content_search.rs | `crates/zeroclaw-tools/src/content_search.rs` | 1,008 | 内容搜索 |
| calculator.rs | `crates/zeroclaw-tools/src/calculator.rs` | 824 | 计算器 |
| canvas.rs | `crates/zeroclaw-tools/src/canvas.rs` | 638 | 画布 |
| web_fetch.rs | `crates/zeroclaw-tools/src/web_fetch.rs` | 1,510 | 网页获取 |
| http_request.rs | `crates/zeroclaw-tools/src/http_request.rs` | 1,041 | HTTP 请求 |
| web_search_tool.rs | `crates/zeroclaw-tools/src/web_search_tool.rs` | 742 | Web 搜索 |
| web_search_provider_routing.rs | `crates/zeroclaw-tools/src/web_search_provider_routing.rs` | ~150 | 搜索路由 |
| memory_store.rs | `crates/zeroclaw-tools/src/memory_store.rs` | 229 | 记忆存储 |
| memory_recall.rs | `crates/zeroclaw-tools/src/memory_recall.rs` | 280 | 记忆检索 |
| memory_forget.rs | `crates/zeroclaw-tools/src/memory_forget.rs` | ~150 | 记忆删除 |
| memory_export.rs | `crates/zeroclaw-tools/src/memory_export.rs` | ~150 | 记忆导出 |
| memory_purge.rs | `crates/zeroclaw-tools/src/memory_purge.rs` | ~150 | 记忆清除 |
| knowledge_tool.rs | `crates/zeroclaw-tools/src/knowledge_tool.rs` | 581 | 知识查询 |
| llm_task.rs | `crates/zeroclaw-tools/src/llm_task.rs` | 491 | LLM 子任务 |
| git_operations.rs | `crates/zeroclaw-tools/src/git_operations.rs` | 994 | Git 操作 |
| backup_tool.rs | `crates/zeroclaw-tools/src/backup_tool.rs` | 466 | 备份 |
| pdf_read.rs | `crates/zeroclaw-tools/src/pdf_read.rs` | 571 | PDF 读取 |
| util_helpers.rs | `crates/zeroclaw-tools/src/util_helpers.rs` | ~200 | 辅助函数 |
| model_routing_config.rs | `crates/zeroclaw-tools/src/model_routing_config.rs` | ~200 | 模型路由配置 |

**从 zeroclaw-runtime/src/tools/ 迁入的工具：**

| 工具 | 参考文件 | 行数 | 说明 |
|------|----------|------|------|
| file_read.rs | `crates/zeroclaw-runtime/src/tools/file_read.rs` | 1,102 | 文件读取（path sandboxed） |
| shell.rs | `crates/zeroclaw-runtime/src/tools/shell.rs` | ~600 | Shell 执行（改用 ToolContext） |
| cron_add.rs | `crates/zeroclaw-runtime/src/tools/cron_add.rs` | ~200 | 定时任务添加 |
| cron_list.rs | `crates/zeroclaw-runtime/src/tools/cron_list.rs` | ~100 | 定时任务列表 |
| cron_remove.rs | `crates/zeroclaw-runtime/src/tools/cron_remove.rs` | ~100 | 定时任务删除 |
| cron_run.rs | `crates/zeroclaw-runtime/src/tools/cron_run.rs` | ~100 | 手动触发 |
| cron_update.rs | `crates/zeroclaw-runtime/src/tools/cron_update.rs` | ~150 | 定时任务更新 |
| cron_runs.rs | `crates/zeroclaw-runtime/src/tools/cron_runs.rs` | ~100 | 执行历史 |

**不保留的 runtime 工具（明确列出）：**

| 工具 | 行数 | 原因 |
|------|------|------|
| delegate.rs | ~3,000 | 多 agent 委派，Android 不需要 |
| schedule.rs | ~813 | 与 cron 功能重叠 |
| security_ops.rs | ~500 | SecurityPolicy 已删除 |
| skill_tool.rs / skill_http.rs | ~600 | skills 系统已删除 |
| sop_*.rs (5 个) | ~1,500 | SOP 系统已删除 |
| verifiable_intent.rs | ~400 | 可验证意图已删除 |
| model_switch.rs | ~300 | 模型切换通过配置实现 |

**新增的注册模块：**

```rust
// registry.rs — 统一工具注册
pub fn all_tools(ctx: &dyn ToolContext) -> Vec<Box<dyn Tool>> {
    let mut tools: Vec<Box<dyn Tool>> = vec![
        Box::new(FileEditTool::new()),
        Box::new(FileWriteTool::new()),
        Box::new(GlobSearchTool::new()),
        Box::new(ContentSearchTool::new()),
        Box::new(CalculatorTool::new()),
        Box::new(WebFetchTool::new()),
        Box::new(HttpRequestTool::new()),
        Box::new(WebSearchTool::new()),
        Box::new(ShellTool::new(ctx)),         // 需要 ToolContext
        Box::new(CronAddTool::new(ctx)),       // 需要 ToolContext
        Box::new(CronListTool::new(ctx)),
        // ... memory tools, etc.
    ];
    tools
}
```

**参考 zeroclaw 源文件：**
- `crates/zeroclaw-tools/src/lib.rs` — 工具导出
- `crates/zeroclaw-runtime/src/tools/mod.rs` — `all_tools_with_runtime()` 注册逻辑
- 上表所列各工具文件

**差异：**
- 统一到一处（不再有 tools crate + runtime/tools/ 分裂）
- 通过 `ToolContext` 的 `ctx.get::<T>()` 能力袋解耦运行时依赖（不需要依赖 agent/providers/memory crate）
- LlmTool 通过 `ctx.get::<dyn Provider>()` 获取 LLM 能力，Memory tools 通过 `ctx.get::<dyn Memory>()` 获取记忆能力
- `file_read.rs` 从 runtime/tools/ 迁入（path-sandboxed 文件读取，12 个工具引用 SecurityPolicy 的调用全部改为 `ctx.get::<SecurityPolicy>()` 能力袋查询）
- 删除 `delegate.rs`（~3,000 行，非 102K，多 agent 委派，Android 不需要）
- 删除所有 SOP/skill/security_ops/verifiable_intent/model_switch/schedule 工具
- 删除 browser/硬件/第三方集成/MCP 工具
- **dead code 清理**：每个工具文件只迁移 `Tool` trait 实现和核心逻辑，跳过 zeroclaw 中为 SecurityPolicy/wrappers/rate-limiting/PathGuard 编写的包装层代码；12+ 工具中对 SecurityPolicy 的直接调用改为 `ctx.get::<SecurityPolicy>()` 能力袋查询（有就检查，没有就跳过）；`util_helpers.rs` 只迁移实际被其他工具调用的函数

**预估行数：** ~11,000

**文件组织（单文件 ≤800 行指导原则，≤1500 行硬上限）：**

```
clawseed-tools/src/
├── lib.rs              (~100)   导出核心类型: Tool, ToolResult, all_tools()
├── registry.rs         (~200)   工具注册 all_tools()
├── file_edit.rs        (~500)   文件编辑
├── file_write.rs       (~400)   文件写入
├── file_read.rs        (~500)   文件读取（从 runtime 迁入）
├── glob_search.rs      (~300)   文件搜索
├── content_search.rs   (~800)   内容搜索（逻辑紧密，不拆分）
├── calculator.rs       (~500)   计算器
├── canvas.rs           (~400)   画布
├── web_fetch.rs        (~800)   网页获取+解析（逻辑紧密，不拆分）
├── http_request.rs     (~700)   HTTP 请求+认证（逻辑紧密，不拆分）
├── web_search.rs       (~400)   Web 搜索
├── web_search_routing.rs (~200) 搜索路由
├── memory_store.rs     (~200)   记忆存储
├── memory_recall.rs    (~200)   记忆检索
├── memory_forget.rs    (~150)   记忆删除
├── memory_export.rs    (~150)   记忆导出
├── memory_purge.rs     (~150)   记忆清除
├── knowledge_tool.rs   (~400)   知识查询
├── llm_task.rs         (~400)   LLM 子任务（通过 ctx.get::<Provider>() 获取）
├── git_operations.rs   (~700)   Git 操作（逻辑紧密，不拆分）
├── backup_tool.rs      (~300)   备份
├── pdf_read.rs         (~400)   PDF 读取
├── shell.rs            (~400)   Shell 执行（通过 ctx.get::<SecurityPolicy>() 检查）
├── cron_add.rs         (~200)   定时任务添加
├── cron_list.rs        (~100)   定时任务列表
├── cron_remove.rs      (~100)   定时任务删除
├── cron_run.rs         (~100)   手动触发
├── cron_update.rs      (~150)   定时任务更新
├── cron_runs.rs        (~100)   执行历史
├── util_helpers.rs     (~200)   辅助函数
└── model_routing.rs    (~200)   模型路由配置
```

**测试策略：**
- 复用 `http_request.rs` 的 58 个测试
- 复用 `web_fetch.rs` 的 49 个测试
- 每个工具至少 2 个基础测试（正常路径 + 错误路径）
- shell 工具：安全策略测试（禁止危险命令）
- cron 工具：表达式解析 + 持久化测试
- 目标：80+ 测试用例

---

### 3.8 clawseed-agent

**职责：** agent 循环、工具调度、成本控制、定时任务引擎。从 zeroclaw-runtime 的 79K 行精简而来。

**公开 API：**

```rust
pub struct Agent {
    provider: Box<dyn Provider>,
    tools: Vec<Box<dyn Tool>>,
    tool_specs: Vec<ToolSpec>,
    memory: Box<dyn Memory>,
    observer: Box<dyn Observer>,
    prompt_builder: SystemPromptBuilder,
    tool_dispatcher: Box<dyn ToolDispatcher>,
    config: AgentConfig,
    history: Vec<ConversationMessage>,
}

impl Agent {
    pub fn builder() -> AgentBuilder;
    pub async fn turn(&mut self, message: &str) -> Result<TurnResult>;
    pub async fn turn_streamed(&mut self, message: &str, tx: mpsc::Sender<TurnEvent>, cancel: CancellationToken) -> Result<TurnResult>;
}

pub enum TurnEvent {
    Chunk { content: String },
    Thinking { content: String },
    ToolCall { id: String, name: String, arguments: Value },
    ToolResult { id: String, name: String, output: String, success: bool },
}

pub struct TurnResult {
    pub content: String,
    pub tool_calls: usize,
    pub total_tokens: u64,
    pub cost_usd: Option<f64>,
}
```

**子模块结构：**

```
clawseed-agent/src/
├── lib.rs              (~200)   公开导出
├── agent.rs            (~1,500) Agent struct + AgentBuilder + turn/turn_streamed
├── loop_.rs            (~2,000) run_tool_call_loop 核心循环
├── dispatcher.rs       (~450)   ToolDispatcher trait + Xml/Native 实现
├── tool_execution.rs   (~250)   工具查找 + 并行/串行执行
├── history.rs          (~300)   对话历史管理 + 上下文裁剪
├── prompt.rs           (~500)   SystemPromptBuilder
├── cost.rs             (~600)   CostTracker 实现 + task-local shim + 预算检查
├── cron/
│   ├── mod.rs          (~300)   导出 + 验证
│   ├── scheduler.rs    (~800)   调度引擎
│   ├── store.rs        (~600)   持久化
│   ├── schedule.rs     (~200)   表达式解析
│   └── types.rs        (~150)   类型定义
├── approval.rs         (~300)   人工审批（简化版）
├── hooks.rs            (~200)   before/after tool call hooks
└── observer.rs         (~300)   noop + log observer

总计: ~8,450 行
```

**参考 zeroclaw 源文件：**

| clawseed-agent 模块 | zeroclaw 源文件 | 行数 | 说明 |
|---------------------|-----------------|------|------|
| agent.rs | `runtime/src/agent/agent.rs` | 2,783 | Agent struct + Builder，精简 |
| loop_.rs | `runtime/src/agent/loop_/mod.rs` | 4,304 | 核心循环，大幅精简 |
| dispatcher.rs | `runtime/src/agent/dispatcher.rs` | 443 | 基本复用 |
| tool_execution.rs | `runtime/src/agent/tool_execution.rs` | 230 | 基本复用 |
| history.rs | `runtime/src/agent/history.rs` | 217 | 基本复用 |
| prompt.rs | `runtime/src/agent/prompt.rs` | 678 | 精简（删除 skills/channel/hardware section） |
| cost.rs | `runtime/src/agent/cost.rs` + `config/src/cost/tracker.rs` + `config/src/cost/types.rs` | 855 | CostTracker 从 config 迁入 + task-local shim |
| cron/ | `runtime/src/cron/` | 4,467 | 精简（删除 channel delivery） |
| approval.rs | `runtime/src/approval/` | ~1,200 | 简化为单一模块 |
| hooks.rs | `runtime/src/hooks/` | ~500 | 简化 |
| observer.rs | `runtime/src/observability/` | ~2,000 | 只保留 noop + log |

**差异：**
- 删除 `agent/eval.rs`、`classifier.rs`、`context_analyzer.rs`、`personality.rs`
- 删除 `tools/delegate.rs`（~3,000 行，非 102K）
- 删除 `security/`（13 种沙箱 + WebAuthn + OTP + threat detection）
- 删除 `skills/`、`skillforge/`、`sop/`
- 删除 `trust/`、`verifiable_intent/`、`tunnel/`、`daemon/`、`doctor/`
- 删除 `i18n.rs`、`migration.rs`、`cli_input.rs`、`identity.rs`
- 删除 `platform/wasm.rs`
- 删除 `observability/prometheus.rs`、`otel.rs`
- prompt.rs 删除 SkillsSection、ChannelMediaSection、RuntimeSection
- cron 删除 channel delivery（Telegram/Discord/Slack 推送）
- **dead code 清理**：`agent/loop_/mod.rs` 4,304 行是 zeroclaw 最臃肿的文件，包含大量为已删子系统（skills/SOP/security/MCP/classifier/personality/eval/context_analyzer）服务的分支和辅助函数，迁移时只复制核心 loop 逻辑（LLM 调用 → tool call 解析 → 执行 → 结果回填 → 循环），跳过所有非核心分支；`agent.rs` 2,783 行中 AgentBuilder 的 24+ 个方法只保留 Android 需要的（provider/tools/memory/config/temperature/cancel_token），删除 skills/classification/autonomy/identity/response_cache/hook_runner 等配置项

**预估行数：** ~10,000（含 CostTracker 实现）

**文件组织（单文件 ≤800 行指导原则，≤1500 行硬上限）：**

```
clawseed-agent/src/
├── lib.rs              (~100)   导出核心类型: Agent, AgentBuilder, TurnEvent, TurnResult, Hook, HookResult
├── agent.rs            (~400)   Agent struct + AgentBuilder
├── turn.rs             (~500)   turn/turn_streamed 实现
├── turn_streaming.rs   (~400)   流式 turn 的 chunk/tool_call 事件发射
├── loop_.rs            (~400)   run_tool_call_loop 主循环入口
├── tool_loop.rs        (~500)   工具执行循环（Hook 检查 + 执行 + 结果回填）
├── context.rs          (~300)   AgentToolContext（impl ToolContext，管理 context_providers）
├── dispatcher.rs       (~450)   ToolDispatcher trait + Xml/Native 实现
├── tool_execution.rs   (~250)   工具查找 + 并行/串行执行
├── history.rs          (~300)   对话历史管理 + 上下文裁剪
├── prompt.rs           (~500)   SystemPromptBuilder
├── cost.rs             (~600)   CostTracker 实现 + task-local shim + 预算检查
├── cron/
│   ├── mod.rs          (~100)   导出
│   ├── scheduler.rs    (~500)   调度引擎
│   ├── executor.rs     (~400)   任务执行
│   ├── store.rs        (~500)   持久化
│   ├── store_queries.rs (~300)  SQL 查询
│   ├── schedule.rs     (~200)   表达式解析
│   └── types.rs        (~150)   类型定义
├── approval.rs         (~300)   人工审批（简化版）
├── hooks.rs            (~200)   Hook trait + 默认 HookChain 执行
└── observer.rs         (~300)   noop + log observer
```

**测试策略：**
- 复用 `loop_/mod.rs` 的 107 个测试中约 40-50 个核心循环测试（62 个测试引用已删子系统需重写或删除）
- 复用 `cron/` 的 ~70 个测试（表达式解析、持久化、调度）
- 新增：agent 端到端测试（mock provider → turn → tool call → result）
- 新增：流式 turn 测试（TurnEvent 序列验证 + CancellationToken 取消）
- 新增：预算耗尽测试（cost tracker → BudgetExceeded → loop breaks）
- 新增：上下文裁剪测试（超长历史 → trim → 不超 token limit）
- 目标：120+ 测试用例

---

### 3.9 clawseed-gateway

**职责：** HTTP/WS 网关，session 管理，remote tool bridge。

**公开 API：**

```rust
pub async fn start_gateway(config: &Config, agent_factory: AgentFactory) -> Result<()>;

pub struct GatewayState {
    pub config: Config,
    pub session_store: SessionStore,
    pub agent_factory: AgentFactory,
}

// REST endpoints
// GET  /api/health          — 健康检查
// GET  /api/config          — 当前配置
// POST /api/chat            — 同步对话
// WS   /ws                  — 流式对话 + remote tool bridge
// POST /api/cron            — 定时任务管理
// GET  /api/cron            — 列表
// DELETE /api/cron/:id      — 删除
```

**子模块结构：**

```
clawseed-gateway/src/
├── lib.rs              (~800)   Axum router + 启动逻辑
├── api.rs              (~800)   REST API endpoints
├── ws.rs               (~900)   WebSocket 处理 + TurnEvent relay
├── session.rs          (~400)   Session 管理（从 infra 迁入）
├── remote_tool.rs      (~350)   Remote tool bridge（Android 端工具调用）
├── auth_rate_limit.rs  (~200)   限流
└── tls.rs              (~300)   TLS 配置（可选）
```

**参考 zeroclaw 源文件：**

| clawseed-gateway 模块 | zeroclaw 源文件 | 行数 | 说明 |
|----------------------|-----------------|------|------|
| lib.rs | `gateway/src/lib.rs` | 2,954 | 精简，删除 node/canvas/voice |
| api.rs | `gateway/src/api.rs` | 2,417 | 精简，删除 pairing/webauthn |
| ws.rs | `gateway/src/ws.rs` | 1,172 | 基本复用 |
| session.rs | `infra/src/session_store.rs` + `gateway/src/session_queue.rs` | ~500 | 合并 |
| remote_tool.rs | `gateway/src/remote_tool.rs` | 364 | 基本复用 |
| auth_rate_limit.rs | `gateway/src/auth_rate_limit.rs` | 204 | 基本复用 |
| tls.rs | `gateway/src/tls.rs` | 456 | 基本复用 |

**差异：**
- 删除 `api_pairing.rs`、`api_webauthn.rs`（设备配对/WebAuthn）
- 删除 `node_tool.rs`、`nodes.rs`（多节点管理）
- 删除 `canvas.rs`、`sse.rs`、`voice_duplex.rs`、`static_files.rs`
- session 管理从 `zeroclaw-infra` 迁入（删除 infra crate）
- 删除 `debounce.rs`、`stall_watchdog.rs`（非核心）
- **dead code 清理**：`lib.rs` 2,954 行和 `api.rs` 2,417 行中大量为 node/canvas/pairing/webauthn/voice 服务的路由注册和 handler 函数不迁移；session 模块只迁移实际被 WS 使用的部分

**预估行数：** ~6,000

**测试策略：**
- 复用 `lib.rs` 的 53 个测试
- 复用 `auth_rate_limit.rs` 的 53 个测试
- 新增：WebSocket 端到端测试（连接 → send message → receive chunks → tool_call → tool_result）
- 新增：remote tool bridge 测试（Rust 请求 → Kotlin 响应模拟）
- 新增：session 持久化测试（SQLite round-trip）
- 目标：60+ 测试用例

---

### 3.10 clawseed (binary)

**职责：** CLI 入口，解析命令行参数，启动 gateway。

**参考 zeroclaw 源文件：**

| 文件 | 行数 | 说明 |
|------|------|------|
| `src/main.rs` | 3,862 | 精简，只保留 Gateway 命令 |
| `src/lib.rs` | ~3,000 | 精简 Commands enum |
| `src/commands/gateway_ops.rs` | ~500 | Gateway 启动逻辑 |

**差异：**
- 删除 Hardware、Peripheral、Sop、Skills、Channel、Integrations、Doctor、Daemon 命令
- 只保留 `clawseed gateway`（启动网关）和 `clawseed --version`
- 二进制名 `clawseed`

**预估行数：** ~2,000

**测试策略：** CLI 参数解析测试 + gateway 启动冒烟测试。

---

## 4. 测试对齐方案

### 4.1 对齐原则

1. **zeroclaw 有测试的模块，clawseed 必须有对应测试覆盖**
2. **zeroclaw 没有测试的模块，clawseed 新增基础测试**
3. **删除的功能不测，保留的功能必测**

### 4.2 测试用例映射表

#### clawseed-parser

| zeroclaw 测试 | clawseed 测试 | 对齐方式 |
|--------------|---------------|----------|
| 102 个解析测试 | 102 个同款测试 | 直接复用，改命名 |
| — | 新增：每格式的序列化 round-trip | 新增 |

#### clawseed-providers

| zeroclaw 测试 | clawseed 测试 | 对齐方式 |
|--------------|---------------|----------|
| compatible 118 个 | compatible 118 个 | 直接复用 |
| anthropic 52 个 | anthropic 52 个 | 直接复用 |
| gemini 53 个 | gemini 53 个 | 直接复用 |
| bedrock 52 个 | bedrock 52 个 | 直接复用 |
| reliable 34 个 | reliable 34 个 | 直接复用 |
| — | registry 测试 10+ | 新增 |

#### clawseed-memory

| zeroclaw 测试 | clawseed 测试 | 对齐方式 |
|--------------|---------------|----------|
| sqlite CRUD 测试 | sqlite CRUD 测试 | 直接复用 |
| sqlite 检索测试 | sqlite 检索测试 | 直接复用 |
| namespaced 测试 | namespaced 测试 | 直接复用 |
| — | 并发安全测试 5+ | 新增 |

#### clawseed-tools

| zeroclaw 测试 | clawseed 测试 | 对齐方式 |
|--------------|---------------|----------|
| http_request 58 个 | http_request 58 个 | 直接复用 |
| web_fetch 49 个 | web_fetch 49 个 | 直接复用 |
| 各工具零散测试 | 统一补齐基础测试 | 新增 |
| — | registry 注册测试 | 新增 |
| — | ToolContext 注入测试 | 新增 |

#### clawseed-agent

| zeroclaw 测试 | clawseed 测试 | 对齐方式 |
|--------------|---------------|----------|
| loop_ 107 个 | loop_ ~45 个核心循环测试 | 62 个引用已删子系统需重写或删除 |
| cron schedule 38 个 | cron schedule 38 个 | 直接复用 |
| cron store 32 个 | cron store 32 个 | 直接复用 |
| approval 35 个 | approval ~15 个 | 精简复用 |
| — | agent 端到端 10+ | 新增 |
| — | 预算耗尽 5+ | 新增 |
| — | 流式 turn 5+ | 新增 |
| — | 上下文裁剪 5+ | 新增 |

#### clawseed-gateway

| zeroclaw 测试 | clawseed 测试 | 对齐方式 |
|--------------|---------------|----------|
| lib.rs 53 个 | lib.rs 30+ | 精简复用（删除 node/canvas 相关） |
| auth_rate_limit 53 个 | auth_rate_limit 53 个 | 直接复用 |
| — | WS 端到端 10+ | 新增 |
| — | remote tool bridge 5+ | 新增 |
| — | session 持久化 5+ | 新增 |

#### clawseed-config

| zeroclaw 测试 | clawseed 测试 | 对齐方式 |
|--------------|---------------|----------|
| schema/tests.rs 296 个 | ~30 个 | 只保留配置加载相关，删除已删子系统测试 |
| policy/tests.rs 153 个 | 0 | 不保留（policy 已删） |
| — | 环境变量覆盖 10+ | 新增 |
| — | 配置 round-trip 10+ | 新增 |

### 4.3 测试基础设施

```
tests/
├── support/
│   ├── mock_provider.rs    — Mock LLM provider（从 zeroclaw 复用）
│   ├── mock_tools.rs       — Mock tool 实现
│   ├── mock_memory.rs      — Mock memory 后端
│   └── helpers.rs          — 测试工具函数
├── fixtures/
│   ├── config.toml         — 测试用配置文件
│   └── ...                 — 其他测试数据
└── integration/
    ├── agent_turn.rs       — Agent 端到端测试
    ├── gateway_ws.rs       — WebSocket 集成测试
    └── remote_tool.rs      — Remote tool bridge 测试
```

### 4.4 测试覆盖率目标

| crate | 单元测试 | 集成测试 | 目标行覆盖率 |
|-------|---------|---------|-------------|
| clawseed-api | 0（trait only） | 0 | N/A |
| clawseed-macros | 5+ | 0 | 80% |
| clawseed-parser | 102+ | 5+ | 90% |
| clawseed-config | 30+ | 10+ | 70% |
| clawseed-providers | 300+ | 20+ | 80% |
| clawseed-memory | 40+ | 10+ | 70% |
| clawseed-tools | 80+ | 15+ | 70% |
| clawseed-agent | 120+ | 20+ | 75% |
| clawseed-gateway | 60+ | 20+ | 70% |
| clawseed (binary) | 5+ | 5+ | 50% |
| **总计** | **742+** | **105+** | — |
### 4.5 验证命令

```bash
# 1. 编译检查
cargo check

# 2. 全部测试
cargo test

# 3. Clippy 零警告（严格模式）
cargo clippy -- -D warnings

# 4. 无 allow(dead_code/unused) 残留
grep -rn "#\[allow" --include="*.rs" crates/clawseed-*/src/ | grep -i "dead_code\|unused"  # 应返回空

# 5. Android 交叉编译
cargo build --no-default-features --features android --target aarch64-linux-android

# 6. 默认编译（android feature）
cargo build

# 7. 无 zeroclaw 残留
grep -rn "zeroclaw" --include="*.rs" --include="*.toml"  # 应返回空
```

---

## 5. 开发里程碑

### Phase 1：骨架搭建（1-2 天）

**目标：** 10 个 crate 空壳 + 依赖关系编译通过

- [ ] 创建 `crates/clawseed-{api,macros,parser,config,providers,memory,tools,agent,gateway}` 目录
- [ ] 每个目录创建 `Cargo.toml` + `src/lib.rs`（空或最小骨架）
- [ ] 根 `Cargo.toml` 更新 workspace members
- [ ] `cargo check` 通过

**验证：** 空项目编译通过，依赖图正确

### Phase 2：API 层 + Parser（1-2 天）

**目标：** trait 定义 + tool call 解析器可用

- [ ] `clawseed-api`：Provider、Tool、ToolContext、Memory、Observer trait
- [ ] `clawseed-macros`：Configurable derive macro
- [ ] `clawseed-parser`：从 zeroclaw 复用 + 改标识符 + 102 个测试通过
- [ ] `cargo test -p clawseed-parser` 全绿

**验证：** parser 测试全绿，api trait 可被下游引用

### Phase 3：Config + Providers（3-4 天）

**目标：** 配置加载 + 4 个 provider 可用

- [ ] `clawseed-config`：Config 结构 + TOML 加载 + 环境变量覆盖
- [ ] `clawseed-providers`：compatible/ + anthropic + gemini + bedrock + reliable + auth
- [ ] 配置测试 30+ 通过
- [ ] provider 测试 300+ 通过
- [ ] `cargo test -p clawseed-config -p clawseed-providers` 全绿

**验证：** 可通过配置文件指定 provider 并成功调用 LLM

### Phase 4：Memory + Tools（3-4 天）

**目标：** 记忆存储 + 内置工具可用

- [ ] `clawseed-memory`：SQLite 后端 + 检索 + 衰减
- [ ] `clawseed-tools`：所有保留工具 + registry + ToolContext 注入
- [ ] memory 测试 40+ 通过
- [ ] tools 测试 80+ 通过
- [ ] `cargo test -p clawseed-memory -p clawseed-tools` 全绿

**验证：** 可 store/recall 记忆，可执行 file/web/cron 工具

### Phase 5：Agent + Gateway（4-5 天）

**目标：** agent loop + HTTP/WS 网关可用

- [ ] `clawseed-agent`：Agent + Builder + turn/turn_streamed + cron + cost
- [ ] `clawseed-gateway`：Axum router + WS + session + remote tool bridge
- [ ] `clawseed (binary)`：CLI 入口 + gateway 命令
- [ ] agent 测试 120+ 通过
- [ ] gateway 测试 60+ 通过
- [ ] `cargo test` 全绿

**验证：** `clawseed gateway` 启动后，可通过 WebSocket 发送消息并收到流式响应

### Phase 6：Android 对齐 + 清理（2-3 天）

**目标：** Android 编译通过 + 无 zeroclaw 残留

- [ ] Android 交叉编译通过（aarch64-linux-android）
- [ ] `tools/build-clawseed-android.sh` 脚本
- [ ] SO 文件名 `libclawseed.so`
- [ ] `cargo clippy -- -D warnings` 无警告
- [ ] 无 `#[allow(dead_code)]` / `#[allow(unused_imports)]` 残留
- [ ] `grep -rn "zeroclaw"` 返回空
- [ ] 更新 `clients/android/` Kotlin 代码中的引用

**验证：** Android app 可加载 libclawseed.so 并通过 gateway 通信

---

## 附录 A：代码量对比

| crate | zeroclaw 行数 | clawseed 行数 | 削减比例 |
|-------|-------------|-------------|---------|
| api | 2,910 | 2,500 | 14% |
| macros | 851 | 800 | 6% |
| parser | 2,773 | 2,800 | +1% |
| config | 26,406 | 5,000 | 81% |
| providers | 31,955 | 8,000 | 75% |
| memory | 11,682 | 6,000 | 49% |
| tools | 45,136 + runtime/tools | 11,000 | 76% |
| runtime | 79,452 | — (拆为 agent) | — |
| agent (新) | — | 10,000 | — || infra | 2,229 | — (并入 gateway/agent) | — |
| gateway | 10,271 | 6,000 | 42% |
| binary | 10,700 | 2,000 | 81% |
| **总计** | **~225K** | **~55K** | **76%** |

## 附录 B：环境变量迁移表

| zeroclaw 环境变量 | clawseed 环境变量 |
|-------------------|-------------------|
| ZEROCLAW_PROVIDER | CLAWSEED_PROVIDER |
| ZEROCLAW_MODEL | CLAWSEED_MODEL |
| ZEROCLAW_API_KEY | CLAWSEED_API_KEY |
| ZEROCLAW_PROVIDER_URL | CLAWSEED_PROVIDER_URL |
| ZEROCLAW_PROVIDER_TIMEOUT_SECS | CLAWSEED_PROVIDER_TIMEOUT_SECS |
| ZEROCLAW_GATEWAY_HOST | CLAWSEED_GATEWAY_HOST |
| ZEROCLAW_GATEWAY_PORT | CLAWSEED_GATEWAY_PORT |
| ZEROCLAW_GATEWAY_TIMEOUT_SECS | CLAWSEED_GATEWAY_TIMEOUT_SECS |
| ZEROCLAW_WORKSPACE | CLAWSEED_WORKSPACE |
| ZEROCLAW_EXTRA_HEADERS | CLAWSEED_EXTRA_HEADERS |
| ZEROCLAW_CONFIG_DIR | CLAWSEED_CONFIG_DIR |
| ZEROCLAW_WEB_SEARCH_ENABLED | CLAWSEED_WEB_SEARCH_ENABLED |
| ZEROCLAW_WEB_SEARCH_PROVIDER | CLAWSEED_WEB_SEARCH_PROVIDER |
| ZEROCLAW_WEB_SEARCH_MAX_RESULTS | CLAWSEED_WEB_SEARCH_MAX_RESULTS |
| ZEROCLAW_WEB_SEARCH_TIMEOUT_SECS | CLAWSEED_WEB_SEARCH_TIMEOUT_SECS |
| ZEROCLAW_INTERACTIVE | CLAWSEED_INTERACTIVE |
| ZEROCLAW_LOCALE | CLAWSEED_LOCALE |
| ZEROCLAW_HTTP_PROXY | CLAWSEED_HTTP_PROXY |
| ZEROCLAW_HTTPS_PROXY | CLAWSEED_HTTPS_PROXY |
| ZEROCLAW_ALL_PROXY | CLAWSEED_ALL_PROXY |
| ZEROCLAW_NO_PROXY | CLAWSEED_NO_PROXY |
| ZEROCLAW_STORAGE_DB_URL | CLAWSEED_STORAGE_DB_URL |
| ZEROCLAW_STORAGE_PROVIDER | CLAWSEED_STORAGE_PROVIDER |
| ZEROCLAW_REASONING_ENABLED | CLAWSEED_REASONING_ENABLED |
| ZEROCLAW_REASONING_EFFORT | CLAWSEED_REASONING_EFFORT |
| ZEROCLAW_TEMPERATURE | CLAWSEED_TEMPERATURE |
| ZEROCLAW_BRAVE_API_KEY | CLAWSEED_BRAVE_API_KEY |
| ZEROCLAW_SEARXNG_INSTANCE_URL | CLAWSEED_SEARXNG_INSTANCE_URL |

以下 zeroclaw 环境变量在 clawseed 中**不保留**（对应子系统已删除）：

| 删除的环境变量 | 原因 |
|---------------|------|
| ZEROCLAW_ALLOW_PUBLIC_BIND | 安全策略已删除 |
| ZEROCLAW_AUDIT_SIGNING_KEY | 审计已删除 |
| ZEROCLAW_CODEX_BASE_URL | codex provider 已删除 |
| ZEROCLAW_CODEX_REASONING_EFFORT | codex provider 已删除 |
| ZEROCLAW_CODEX_RESPONSES_URL | codex provider 已删除 |
| ZEROCLAW_LUCID_* | lucid memory 已删除 |
| ZEROCLAW_NEXTCLOUD_TALK_WEBHOOK_SECRET | channel 已删除 |
| ZEROCLAW_OPEN_SKILLS_ENABLED | skills 已删除 |
| ZEROCLAW_OPEN_SKILLS_DIR | skills 已删除 |
| ZEROCLAW_PROXY_ENABLED | proxy 策略已删除 |
| ZEROCLAW_PROXY_SCOPE | proxy 策略已删除 |
| ZEROCLAW_PROXY_SERVICES | proxy 策略已删除 |
| ZEROCLAW_REQUIRE_PAIRING | pairing 已删除 |
| ZEROCLAW_SKILLS_ALLOW_SCRIPTS | skills 已删除 |
| ZEROCLAW_SKILLS_PROMPT_MODE | skills 已删除 |
| ZEROCLAW_STORAGE_CONNECT_TIMEOUT_SECS | 非核心 |
| ZEROCLAW_TEST_PASSTHROUGH | 测试专用 |
| ZEROCLAW_WEB_DIST_DIR | web dashboard 已删除 |
| ZEROCLAW_WHATSAPP_APP_SECRET | channel 已删除 |

## 附录 C：Android SO 命名变更

| 项目 | zeroclaw | clawseed |
|------|----------|----------|
| 二进制名 | `zeroclaw` | `clawseed` |
| SO 文件名 | `libzeroclaw.so` | `libclawseed.so` |
| JNI 库路径 | `clients/android/app/src/main/jniLibs/<abi>/libzeroclaw.so` | `clients/android/app/src/main/jniLibs/<abi>/libclawseed.so` |
| Kotlin 包名 | `dev.zeroclaw.client` | `dev.clawseed.client` |
| Kotlin 类名 | `ZeroclawClient` / `ZeroclawMessages` / `ZeroclawService` | `ClawseedClient` / `ClawseedMessages` / `ClawseedService` |
