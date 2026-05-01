# ClawSeed ADF 评分与架构改进

基于 ADF (Agent Dev First) 评分标准，对 clawseed 当前设计进行评估，并给出改进建议。

## 一、当前设计评分

### 1. 架构清晰性：8/10

| 指标 | 标准 | clawseed 现状 | 得分 |
|------|------|-------------|------|
| 依赖深度 | ≤4层 | api(0) → config(1) → providers(2) → tools(3) → agent(4) → gateway(5) = **5层** | 1/3 |
| 循环依赖 | 0对 | 0对（严格 DAG） | 4/4 |
| 核心模块公开API | ≤5个 | clawseed-api 导出 ~6 个 trait（Provider/Tool/Hook/Memory/Observer/ContextProvider） | 1/2 |
| 目录结构清晰度 | 路径可推断 | ✅ crate 名即职责 | 1/1 |
| **小计** | | | **7/10** |

**问题：** 依赖深度 5 层，超过 ADF 的 4 层标准。原因是 tools 依赖 providers/memory，agent 又依赖 tools，形成链式堆积。

### 2. 配置统一性：7/10

| 指标 | 标准 | clawseed 现状 | 得分 |
|------|------|-------------|------|
| 配置来源数 | 1个 | 2个（TOML 文件 + 环境变量覆盖） | 2/3 |
| 类型注解覆盖率 | 100% | Rust 强类型，100% | 3/3 |
| 默认值覆盖率 | 100% | Config 有 defaults.rs | 1/2 |
| 层级组合 | 支持嵌套 | ✅ Config 嵌套 ProvidersConfig/AgentConfig 等 | 2/2 |
| **小计** | | | **8/10** |

**问题：** 扩展 crate（security/skills/sop）各自定义配置，配置来源分散到多个 crate。环境变量和 TOML 是两个来源。

### 3. 模块化边界：5/10

| 指标 | 标准 | clawseed 现状 | 得分 |
|------|------|-------------|------|
| >1000行文件数 | 0个 | 估计保留 5-8 个（agent.rs ~1500, loop_.rs ~2000, cron/store.rs ~600→精简后仍可能, provider 各 ~2000, memory/sqlite.rs ~2764→精简后仍 >1000） | 1/3 |
| >500行文件比例 | ≤5% | 估计 20-30% | 0/2 |
| 公开API>10的模块数 | 0个 | clawseed-agent 导出 ~15 个公开类型 | 0/2 |
| 单元测试隔离率 | 100% | 需要验证 | 2/3 |
| **小计** | | | **3/10** |

**问题：** 这是 clawseed 最大的短板。单个文件仍然过大，公开 API 数量超标。

### 4. 测试可观测性：7/10

| 指标 | 标准 | clawseed 现状 | 得分 |
|------|------|-------------|------|
| 单元测试运行时间 | ≤30秒 | Rust 编译慢，但增量测试快，估计达标 | 2/2 |
| 错误定位精度 | 文件:行号 | ✅ Rust 编译器 + test 输出精确 | 2/2 |
| 测试标记系统 | 有 | 无（Rust 没有 pytest.mark 等价物，需用 feature flag 或 #[ignore]） | 0/1 |
| Profile工具 | 有 | 无 | 0/2 |
| 测试覆盖率 | ≥80% | 目标 70-80%，部分 crate 可能不达标 | 1/2 |
| 测试文件数 | ≥50个 | 目标 742+ 单元测试 | 1/1 |
| **小计** | | | **6/10** |

### 5. 模式可复制性：6/10

| 指标 | 标准 | clawseed 现状 | 得分 |
|------|------|-------------|------|
| 参考实现数量 | ≥1个完整pipeline | 开发计划中有 binary 组装示例，但无完整可运行参考 | 1/3 |
| 开发指南步骤数 | ≤5步 | 6 个 Phase，每个含 4-8 步 | 1/2 |
| Skill系统 | 有 | 有 .claude/skills/ | 3/3 |
| 自动化验证工具 | 有 | 有验证命令，无 CI | 1/2 |
| **小计** | | | **6/10** |

### 总分：30/50（C 级）

| 维度 | 得分 | 
|------|------|
| 架构清晰性 | 7/10 |
| 配置统一性 | 8/10 |
| 模块化边界 | 3/10 |
| 测试可观测性 | 6/10 |
| 模式可复制性 | 6/10 |
| **总分** | **30/50 (60%)** |

---

## 二、改进方案

### 改进 1：降低依赖深度（架构清晰性 +2 分）

**问题：** api → config → providers → tools → agent → gateway = 5 层

**根因：** tools 依赖 providers（因为 llm_task.rs 用 Provider）和 memory（因为 memory tools 用 Memory）。这把 tools 推到第 3 层，agent 被推到第 4 层。

**方案：** tools 不直接依赖 providers 和 memory，改为通过 ToolContext 注入。

```
当前：clawseed-tools → clawseed-providers, clawseed-memory
改后：clawseed-tools → clawseed-api（只依赖 trait）

依赖深度变化：
  api(0) → config(1) → providers(2) ──────────────────→ gateway(3)
                    → memory(2)  ─────────────────────→ gateway(3)
                    → tools(2, 只依赖 api) ──────────→ agent(3) → gateway(4)
```

agent 直接依赖 providers/memory 获取实现，通过 ToolContext 传给 tools。tools crate 只知道 trait，不知道具体 crate。

**具体改动：**

```rust
// clawseed-tools/Cargo.toml
[dependencies]
clawseed-api = { workspace = true }      // 只依赖 api
clawseed-config = { workspace = true }    // 配置类型

// 不再依赖：
// clawseed-providers  — LlmTask 通过 ctx.get::<Provider>() 获取
// clawseed-memory     — Memory tools 通过 ctx.get::<Memory>() 获取
```

```rust
// llm_task.rs 改造
async fn execute(&self, args: Value, ctx: &dyn ToolContext) -> Result<ToolResult> {
    // 旧：use clawseed_providers::Provider;  // tools 直接依赖 providers
    // 新：通过 ToolContext 获取
    let provider = ctx.get::<dyn Provider>()
        .ok_or("No provider available in context")?;
    let response = provider.chat(request).await?;
    // ...
}
```

**代价：** LlmTask 和 memory tools 不再能直接 import Provider/Memory 类型，必须通过 `ctx.get::<T>()`。但这是能力袋设计本来的意图。

**效果：** 依赖深度从 5 层降到 4 层，得分 2/3 → 3/3。

### 改进 2：拆分大文件（模块化边界 +3 分）

**问题：** zeroclaw 有 22 个 >1000 行文件，最大 4304 行。clawseed 需要控制但不必过度碎片化。

**方案：** 务实的文件大小约束：

| 约束 | 值 | 说明 |
|------|---|------|
| 指导原则 | ≤800 行 | 超过时考虑拆分 |
| 硬上限 | ≤1500 行 | 不可超过 |
| 例外 | 逻辑紧密的代码不强制拆 | web_fetch（获取+解析紧密耦合）、content_search 等保持单文件 |

**拆分原则：**
1. 按职责拆，不按行数拆——拆出的文件有独立职责才值得
2. 逻辑紧密耦合的代码保持同文件——agent 读一个文件就能理解完整逻辑
3. 拆出的子模块用 `pub(crate)` 内聚，对外只暴露父模块的公开 API
4. 宁可一个 800 行文件，不要 4 个 200 行文件但逻辑割裂

### 改进 3：收敛公开 API（模块化边界 +1 分）

**问题：** zeroclaw-runtime 导出 28 个 mod，clawseed-agent 如果也导出过多类型则难以理解。

**方案：** lib.rs 用 facade 模式导出核心类型，其余 `pub(crate)`。但不硬限 ≤5 个——API 可发现性比数量更重要，10 个有文档的公开 API 比 3 个没文档的更容易用。

```rust
// clawseed-agent/src/lib.rs
pub use agent::{Agent, AgentBuilder};
pub use turn::{TurnEvent, TurnResult};
pub use hook::{Hook, HookResult};
pub use observer::Observer;
pub use dispatcher::ToolDispatcher;
pub use cost::CostTracker;
pub use context::AgentToolContext;
// ~8 个核心导出，每个都有 rustdoc

// 其余全部 pub(crate):
// ToolExecutionResult, ConversationMessage, HistoryManager, SystemPromptBuilder...
// 只在 crate 内部可见
```

### 改进 4：统一配置到单一入口（配置统一性 +1 分）

**问题：** 扩展 crate 各自定义配置，配置来源分散。

**方案：** 扩展配置通过 `Config::extensions` 动态段加载，不各自解析 TOML。

```rust
// clawseed-config
pub struct Config {
    pub workspace_dir: PathBuf,
    pub providers: ProvidersConfig,
    pub agent: AgentConfig,
    pub gateway: GatewayConfig,
    pub storage: StorageConfig,
    pub scheduler: SchedulerConfig,
    pub reliability: ReliabilityConfig,
    pub extensions: HashMap<String, Value>,  // 扩展配置的原生 TOML 值
}

// clawseed-security 中的配置加载
pub struct SecurityConfig { ... }

impl SecurityConfig {
    pub fn from_extensions(extensions: &HashMap<String, Value>) -> Result<Self> {
        let raw = extensions.get("security").ok_or("missing [security]")?;
        SecurityConfig::deserialize(raw.clone())
    }
}
```

**效果：** 所有配置从同一个 TOML 文件加载，配置来源 = 1 个文件 + 环境变量覆盖 = 2 个来源（符合 ADF 2 分标准）。

### 改进 5：测试标记与 Profile（测试可观测性 +3 分）

**问题：** 无测试标记系统、无 Profile 工具。

**方案：**

```rust
// 测试标记 — 用 module 组织替代 pytest.mark
// tests/
//   unit/         — cargo test --test unit（快速，≤30s）
//   integration/  — cargo test --test integration（需要 mock server）
//   slow/         — cargo test --test slow（>1s，CI only）

// Profile 工具 — 在 tools/profile.sh 中提供
// cargo test -- --nocapture + flamegraph 集成
```

**效果：** 测试标记 1/1，Profile 工具 2/2。

### 改进 6：参考实现 pipeline（模式可复制性 +2 分）

**问题：** 无完整可运行参考实现。

**方案：** 在 `examples/` 中提供 1 个完整的扩展 crate 参考实现。

```
examples/clawseed-echo/
├── Cargo.toml
├── src/
│   ├── lib.rs          (~100 行, 注册函数 + 导出)
│   ├── hook.rs         (~80 行, impl Hook: 记录工具调用日志)
│   ├── context.rs      (~60 行, impl ContextProvider: 暴露 EchoConfig)
│   ├── tools.rs        (~100 行, impl Tool: echo 工具)
│   └── config.rs       (~50 行, [echo] section 配置)
```

这个 echo 扩展展示了：
1. 如何实现 Hook / ContextProvider / Tool
2. 如何从 Config::extensions 加载配置
3. 如何在 binary 中注册（一行 `.hook().tools()`）
4. 如何写测试

**效果：** 参考实现完整性 3/3。

---

## 三、改进后评分预估

| 维度 | 改进前 | 改进后 | 提升 |
|------|--------|--------|------|
| 架构清晰性 | 7/10 | 9/10 | +2（依赖深度 5→4） |
| 配置统一性 | 8/10 | 9/10 | +1（扩展配置统一入口） |
| 模块化边界 | 3/10 | 7/10 | +4（文件大小受控、API 收敛） |
| 测试可观测性 | 6/10 | 9/10 | +3（测试分层+Profile） |
| 模式可复制性 | 6/10 | 8/10 | +2（参考实现 pipeline） |
| **总分** | **30/50 (C)** | **42/50 (A)** | **+12** |

---

## 四、需同步更新到开发计划的改动

以下改动需要合并回 `docs/clawseed-development-plan.md`：

1. **tools 不依赖 providers/memory** — 只依赖 api + config，通过 ToolContext 获取 Provider/Memory
2. **文件大小务实控制** — ≤800 行指导原则，≤1500 行硬上限，逻辑紧密的代码不强制拆
3. **lib.rs facade 模式** — 导出核心类型（不硬限 ≤5），其余 pub(crate)
4. **Config::extensions** — 扩展配置的统一加载机制
5. **examples/clawseed-echo** — 扩展 crate 参考实现
6. **测试分层** — unit/integration/slow 三级测试
