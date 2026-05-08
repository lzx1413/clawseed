# Gateway 共享 Provider/Memory 重构

## Context

Gateway 每个 WebSocket 连接调用 `Agent::from_config()` 创建独立 Agent，导致 Provider（含 HTTP 连接池）和 Memory（含 SQLite 连接）被重复创建。而 `AppState` 已经持有 `Arc<dyn Provider>`、`Arc<dyn Memory>`、`Arc<dyn Observer>` 等共享组件，但 ws.rs 完全忽略了它们。

目标：Gateway 使用共享 Provider/Memory/Observer 构建 Agent，不再每个连接独立创建。CLI chat 模式继续使用 `from_config()` 不受影响。

## 变更清单

### 1. Agent: `provider` 从 `Box<dyn Provider>` 改为 `Arc<dyn Provider>`

**文件**: `crates/clawseed-agent/src/agent.rs`

- Line 62: `provider: Box<dyn Provider>` → `provider: Arc<dyn Provider>`
- Line 86: `provider: Option<Box<dyn Provider>>` → `provider: Option<Arc<dyn Provider>>`
- Line 144-146: `provider()` 方法改为包装 Box 为 Arc：
  ```rust
  pub fn provider(mut self, provider: Box<dyn Provider>) -> Self {
      self.provider = Some(Arc::from(provider));
      self
  }
  ```
- 新增 `shared_provider()` 方法：
  ```rust
  pub fn shared_provider(mut self, provider: Arc<dyn Provider>) -> Self {
      self.provider = Some(provider);
      self
  }
  ```

`Provider` trait 有 `Arc<T>: Provider` blanket impl（`clawseed-api/src/provider.rs`），所有 `self.provider.chat()` / `self.provider.stream_chat()` 调用无需改动。

### 2. 新增 `from_config_with_shared_components()` 公共构造器

**文件**: `crates/clawseed-agent/src/agent.rs`

**核心改动**：不在 ws.rs / handlers.rs 中手写 builder 逻辑，而是在 Agent 上新增一个公共构造器，把 `from_config()` 的装配逻辑完整保留，只替换 provider/memory/observer 的来源：

```rust
/// Build an agent from config, reusing externally-provided shared components.
///
/// Unlike `from_config()` which creates its own provider/memory/observer,
/// this method accepts pre-built instances — typically shared across
/// gateway WebSocket connections.
///
/// model_name and temperature are also taken from the shared bundle
/// (state.model / state.temperature), not re-read from config, to avoid
/// provider-config skew (e.g., old provider + new model after a config update).
pub async fn from_config_with_shared_components(
    config: &clawseed_config::schema::Config,
    provider: Arc<dyn Provider>,
    memory: Arc<dyn Memory>,
    observer: Arc<dyn Observer>,
    model_name: String,
    temperature: f64,
) -> anyhow::Result<Self>
```

实现方式：从 `from_config_with_registry()` 提取公共逻辑。两个方法共享同一段装配代码（hook chain、tool filters、skills、identity、autonomy、model/temperature 等），区别仅在 provider/memory/observer 的来源：

| 装配项 | `from_config_with_registry()` | `from_config_with_shared_components()` |
|--------|-------------------------------|----------------------------------------|
| provider | 从 config 创建 `Box<dyn Provider>` | 接收外部 `Arc<dyn Provider>` |
| memory | 从 config 创建 `Arc<dyn Memory>` | 接收外部 `Arc<dyn Memory>` |
| observer | 创建 `NoopObserver` | 接收外部 `Arc<dyn Observer>` |
| tools | `all_tools(config, mem)` | 同，使用传入的 mem |
| dispatcher | 根据 `provider.supports_native_tools()` | 同 |
| HookRunner + SecurityPolicy | 从 config 创建 | 同 |
| declarative hook chain | 从 config.hooks.chain 加载 | 同 |
| allowed/denied/mcp filters | 从 config 读取 | 同 |
| model_name / temperature | 从 config fallback provider 读取 | 接收外部参数（`state.model` / `state.temperature`） |
| workspace_dir | 从 config 读取 | 同 |
| autonomy_level | 从 config 读取 | 同 |
| identity_config | 从 config 读取 | 同 |
| auto_save | 从 config 读取 | 同 |
| skills config | 从 config 读取 | 同 |
| memory_session_id | 不设置（由调用方后续设置） | 同 |

**为什么不用 builder 手写**：`from_config_with_registry()` 负责 15+ 项装配逻辑。如果 ws.rs 和 handlers.rs 各自手写 builder 调用，遗漏任何一项都会导致回归（hook 丢失、工具过滤失效、系统提示漂移、auto_save 关闭、skills 失效等）。公共构造器确保装配逻辑只在一个地方维护。

**实现策略**：提取一个私有辅助方法 `build_agent_from_config()` 承载公共装配逻辑，`from_config_with_registry()` 和 `from_config_with_shared_components()` 都调用它，各自提供 provider/memory/observer：

```rust
impl Agent {
    /// Private: shared assembly logic for both public constructors.
    fn build_from_config(
        config: &Config,
        provider: Arc<dyn Provider>,
        memory: Arc<dyn Memory>,
        observer: Arc<dyn Observer>,
        model_name: String,
        temperature: f64,
    ) -> anyhow::Result<Self> {
        // tools, dispatcher, hook_runner, filters, skills, identity...
        // All the logic currently in from_config_with_registry()
        // lines 374-464, minus the provider/memory/observer/model/temperature creation.
    }

    pub async fn from_config_with_registry(...) -> Result<Self> {
        let provider = /* create from config */;
        let memory = /* create from config */;
        let observer = Arc::new(NoopObserver);
        let (model_name, temperature) = /* from config fallback */;
        Self::build_from_config(config, provider, memory, observer, model_name, temperature)
    }

    pub async fn from_config_with_shared_components(...) -> Result<Self> {
        Self::build_from_config(config, provider, memory, observer, model_name, temperature)
    }
}
```

### 3. ws.rs: 用 `from_config_with_shared_components()` 替换 `from_config()`

**文件**: `crates/clawseed-gateway/src/ws.rs`

替换 lines 211-233：

```rust
// Before:
let config = state.config.lock().clone();
let mut agent = Agent::from_config(&config).await?;

// After:
let config = state.config.lock().clone();
let mut agent = Agent::from_config_with_shared_components(
    &config,
    state.provider.clone(),
    state.mem.clone(),
    state.observer.clone(),
    state.model.clone(),
    state.temperature,
).await?;
```

一行替换，所有装配逻辑由公共构造器处理，不遗漏任何字段。

### 4. handlers.rs: webhook 同样替换

**文件**: `crates/clawseed-gateway/src/handlers.rs`

替换 lines 219-224（`#[cfg(not(test))]` 分支）：

```rust
// Before:
let config = _state.config.lock().clone();
let mut agent = Agent::from_config(&config).await?;

// After:
let config = _state.config.lock().clone();
let mut agent = Agent::from_config_with_shared_components(
    &config,
    _state.provider.clone(),
    _state.mem.clone(),
    _state.observer.clone(),
    None,
).await?;
```

`#[cfg(test)]` 分支无需改动。

### 5. 不需要改动的部分

- **`from_config()` / `from_config_with_registry()`** — CLI chat 仍在用，保留不变
- **`AppState` 结构体** — 已有 `Arc<dyn Provider>` / `Arc<dyn Memory>` / `Arc<dyn Observer>`
- **测试代码** — 用 `Agent::builder().provider(Box::new(MockProvider))` 的测试，`provider()` 方法内部包装为 Arc，无需改动
- **双注册表** — `AppState.tool_registry` 继续用于 `/api/tools` 可见性，Agent 的 tool_registry 每连接独立用于实际调度

## 安全性分析

| 组件 | 共享安全？ | 原因 |
|------|-----------|------|
| Provider | ✅ 线程安全 | `ReliableProvider.key_index` 用 `AtomicUsize`，线程安全 |
| Memory | ⚠️ 线程安全但有吞吐 trade-off | 见下方"Memory 并发模型" |
| Observer | ✅ | `NoopObserver` 无状态 |
| HookRunner | ❌ 必须每连接 | `SecurityPolicy.action_count` 语义上每会话应有独立速率限制预算 |
| ToolRegistry | ❌ 必须每连接 | 远程工具按连接隔离，共享会导致 agent A 调用 agent B 的远程工具 |

### Memory 并发模型

当前 `SqliteMemory` 内部用 `Arc<Mutex<Connection>>`（单连接 + 互斥锁）。改为共享 `state.mem` 后，所有 WS/webhook 会话的 memory 读写串行在同一把锁上。

**评估**：
- SQLite 本身的写并发就是串行的（即使多连接也通过 WAL 或文件锁序列化），所以这不是新引入的瓶颈，只是从"多把锁各自串行"变成"一把锁全局串行"
- **auto_save 开启时 Memory 在关键路径上**：当 `config.memory.auto_save = true`（默认），每次 `turn()` 都会调用 `memory.store()` 写入用户消息（`agent.rs:947-957`）。这意味着共享 `state.mem` 的串行化不只是工具侧问题，而是会直接影响常规聊天请求的并发吞吐
- 当前主要场景（Android 单连接）完全无影响
- **长期方案**：`SqliteMemory` 改用连接池（如 `r2d2-sqlite`），消除单锁瓶颈
- **验证要求**：需在 auto_save 开启下做并发 chat 压测，确认多连接场景下共享 memory 不会成为瓶颈

## 配置热更新语义变化

**当前行为**（隐式不一致）：
- `AppState.provider` / `AppState.mem` — 启动时创建，配置更新不重建
- per-connection Agent — 每次从最新 config 创建，新连接可吃到新 provider/memory 配置

**变更后行为**（显式一致）：
- 所有路径统一使用 `AppState` 共享组件（provider、memory、observer、model、temperature），配置更新不重建

**这是一个 breaking change**：通过 `/api/config` 更新 API key / provider / base_url / model / temperature 后，新连接不再使用新配置。当前虽然只有 Agent 级别隐式地部分生效，但确实有用户可观测的差异。

**两个选项**：
1. **接受 breaking change** — 在 CHANGELOG 和文档中明确标注，将 `/api/config` 的 provider/memory 热更新能力列为已知限制。当前 `AppState` 级别的共享组件本来就不支持热更新，这只是统一了行为
2. **纳入共享组件重建机制** — 在 config 更新时重建 `AppState` 的 provider 和 memory（需要 `RwLock` 或 `ArcSwap` 替代当前 `Arc`，并在更新时原子替换）。这是一个独立的增强项，复杂度较高，建议作为后续跟进

**本重构选择选项 1**，理由：当前 `/api/config` 更新对 provider/memory 的"热更新"是隐式且不可靠的（只有新连接生效，已有连接不受影响），显式化这个限制比维持隐式行为更安全。model 和 temperature 也一并纳入共享，避免"旧 provider + 新 model"的 provider-config skew。

## 验证

```bash
cargo build                          # 编译通过
cargo test                           # 全部测试通过
cargo test -p clawseed-gateway       # gateway 相关测试
cargo clippy                         # 无新增 warning
```

手动验证：启动 gateway，通过 WebSocket 连接，确认 chat、remote tool 注册/调用/断连清理正常工作。
