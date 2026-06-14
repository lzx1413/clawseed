# Prompt 缓存优化 — 实现总结

## 背景

ClawSeed 与 Anthropic 的多轮对话**缓存命中率约为 0%**，原因是系统提示词每轮都在变化（秒级精度的 datetime 位于位置 0），且以单个 `SystemBlock` 发送。Anthropic 的 prompt 缓存基于精确前缀匹配——任意位置的任何变化都会打破整个缓存链。这在多轮对话中每轮浪费约 2–4 KB / 1–3k tokens。

## 根因

三个致命的缓存破坏问题：

1. **`DateTimeSection` 位于位置 0** — 内容每秒变化，在最开头就与前缀不一致
2. **单个 `SystemBlock`** — 任何一个字符变化都要重新处理整个块
3. **稳定内容变化时全量重建** — Core 记忆 / 技能 / 工具变化会替换整个系统消息内容

## 实现

### Phase 0 → 分钟精度（被 Phase 2 取代）

最初将 `DateTimeSection::build()` 从秒级精度改为分钟级精度。这改善了同一分钟内的缓存命中，但系统提示词仍然每分钟变化——对长会话不够。

### Phase 1 — 稳定/动态分区（被 Phase 2 取代）

引入 `CacheClass`（Stable/Dynamic）和 `PartitionedSystemPrompt { stable, dynamic, full }`，将系统提示词拆分为可缓存前缀和逐轮动态后缀。`DateTimeSection` 被标记为 `Dynamic` 并移到末尾，在 stable 块末尾追加前置声明 (`⚠️ THE CURRENT TIME BELOW APPLIES TO ALL ABOVE INSTRUCTIONS.`)。

这实现了 Anthropic 前缀缓存，但需要逐轮动态重建和增加复杂性（前置声明、分割逻辑、`dynamic_system_content` 字段、`refresh_dynamic_system_content()` 方法）。

### Phase 2 — 全量稳定（当前实现）

**关键洞察**：如果系统提示词在所有轮次中 100% 稳定（零逐轮变化），自动前缀缓存无需任何消息级变换即可工作。只有 Anthropic 和 Bedrock 需要显式 `cache_control: ephemeral` 标记；其他所有提供商从稳定前缀自动受益。

#### 1. 从系统提示词中移除 DateTimeSection (`prompt.rs`)

`DateTimeSection` 不再包含在 `SystemPromptBuilder::with_defaults()` 中。当前时间通过**用户消息时间戳前缀**提供：

```
[2024-06-14 15:42:00 CST] 今天天气怎么样？
```

Gateway 和 CLI 都在发送给 agent 之前为每条用户消息添加 `[YYYY-MM-DD HH:MM:SS TZ]` 前缀。这保持了时间上下文可用，而无需将其注入系统提示词。

**收益**：
- 整个系统消息在所有轮次中字节完全一致 → 100% 稳定前缀
- 对**所有**具备自动前缀缓存的提供商（DeepSeek、OpenAI、Groq 等）都有效
- 无逐轮重建开销 — 消除 `refresh_dynamic_system_content()`、`build_dynamic_system_content()` 和 `build_dynamic()`

#### 2. 简化 PartitionedSystemPrompt (`prompt.rs`)

无 Dynamic 节时，`PartitionedSystemPrompt` 简化为：

```rust
pub struct PartitionedSystemPrompt {
    pub stable: String,   // 完整系统提示词内容（所有节）
    pub dynamic: String,  // 始终为空 — 当前无 Dynamic 节
    pub full: String,     // dynamic 为空时等于 stable
}
```

- `build_partitioned()` 不再追加前置声明 — 已删除（`DYNAMIC_PREAMBLE` 常量删除）
- `else` 分支（stable + dynamic 拼接）为未来动态节保留，但当前不执行
- `build_dynamic()` 方法删除 — 无需单独构建动态节

#### 3. 从 Agent 中移除动态内容 (`agent.rs`)

删除的字段和方法：
- `dynamic_system_content` 字段 — 不再需要
- `refresh_dynamic_system_content()` — 无动态内容需逐轮刷新
- `build_dynamic_system_content()` — 无动态节需构建

Agent 现在只有 `stable_system_content` — 完整系统提示词内容，仅在稳定内容变化时重建（Core 记忆更新、技能激活/停用、工具变化）。

#### 4. CacheStrategy 枚举替代 `prompt_caching: bool` (`clawseed-api/src/provider.rs`)

```rust
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum CacheStrategy {
    /// 无显式缓存。系统提示词完全稳定，自动前缀缓存即可工作。
    #[default]
    None,
    /// Anthropic 风格显式 `cache_control: ephemeral` 标记或
    /// Bedrock 风格 `CachePoint` 块。
    ExplicitAnthropic,
}

pub struct ProviderCapabilities {
    pub native_tool_calling: bool,
    pub vision: bool,
    pub cache_strategy: CacheStrategy,  // 替代: prompt_caching: bool
}
```

**提供商分配**：

| 提供商 | CacheStrategy | 原因 |
|--------|--------------|------|
| Anthropic | `ExplicitAnthropic` | 需要系统消息块上的 `cache_control: ephemeral` 标记 |
| Bedrock | `ExplicitAnthropic` | 需要系统消息内的 `CachePoint` 块 |
| OpenAI-compatible (DeepSeek, Groq, Ollama 等) | `None` | 稳定提示词下的服务端自动前缀缓存 |
| Gemini | `None` | 无需显式缓存标记 |

`CacheStrategy::None` 默认意味着新提供商自动获得正确行为 — 从稳定系统提示词受益，无需显式缓存标记。

#### 5. DeepSeek Anthropic 兼容端点 (`factory.rs`)

新增 `DeepSeekAnthropicFactory`，用 DeepSeek 的 Anthropic 兼容基础 URL (`https://api.deepseek.com/anthropic`) 包装 `AnthropicProvider`。此端点支持 `cache_control: ephemeral` 标记，让 DeepSeek 用户获得与 Anthropic 用户相同的显式提示词缓存。

- 提供商名称：`deepseek-anthropic`（别名：`deepseek-claude`）
- 使用 `AnthropicProvider::with_base_url()` — 相同转换逻辑、相同 `stable_prefix` 处理、相同 `cache_control` 注入
- 在 `default_provider_factory_registry()` 中与其他工厂一起注册

**原因**：DeepSeek 的 OpenAI 兼容端点 (`/v1/chat/completions`) 仅支持自动前缀缓存。`/anthropic` 端点支持显式 `cache_control`，为 Anthropic 风格客户端提供更精细的控制和保证的缓存命中。

#### 6. 缓存输入 tokens 解析 (`compatible/parsing.rs`, `provider_impl.rs`)

`TokenUsage.cached_input_tokens` 现从提供商特定响应字段填充：

- **DeepSeek** (`/v1/chat/completions`)：`prompt_cache_hit_tokens` 字段
- **OpenAI**：`prompt_tokens_details.cached_tokens` 子字段
- 通过 `UsageInfo::extract_cached_tokens()` 辅助方法提取（`chat()` 和 `stream_chat()` 路径共享）

```rust
impl UsageInfo {
    pub(super) fn extract_cached_tokens(&self) -> Option<u64> {
        self.prompt_cache_hit_tokens
            .or_else(|| self.prompt_tokens_details.as_ref()?.cached_tokens)
    }
}
```

### Anthropic / Bedrock 成成（Phase 1 后未变）

Anthropic 和 Bedrock 提供商仍使用 `stable_prefix` 将系统消息拆分为可缓存块：

- **Anthropic**：`SystemPrompt::Blocks([stable_block(cache_control: ephemeral), dynamic_block(no_cache)])`
- **Bedrock**：`SystemBlock::Text(stable)` + `CachePoint` + `SystemBlock::Text(dynamic)`

由于 `dynamic` 现在始终为空，"动态块"实际上为空或不存在。stable 块包含整个系统提示词，带单个 `cache_control` 标记，Anthropic 将其整体缓存。

### 缓存断点预算

Anthropic 限制每个请求最多 4 个断点。Phase 2 不增加断点数量：

| 位置 | 修改前 | Phase 2 |
|---|---|---|
| OAuth 前缀块 | 0 或 1 | 0 或 1 |
| 系统提示词 | 1 (单块) | 1 (整个提示词带 `cache_control: ephemeral`) |
| 最后对话消息 | 0 或 1 | 0 或 1 |
| 工具结果 | 0 或 1 | 0 或 1 |
| **最大总数** | **4** | **4** |

## 已知限制

1. **稳定块重建会打破该轮缓存**。触发条件：`memory_store` 新增 Core 记忆、技能激活/停用、远程工具注册。下一轮会重新建立缓存。稳定会话中这种情况很少。

2. **最小可缓存前缀为 1024 tokens** (Sonnet/Opus)。紧凑配置（最小 personality、无技能、无 Core 记忆）可能达不到阈值，不会被缓存。

3. **提供器覆盖范围**: Anthropic + Bedrock 使用 `CacheStrategy::ExplicitAnthropic`（显式标记）。DeepSeek-anthropic 端点也支持显式标记。所有其他提供商使用 `CacheStrategy::None`（通过稳定提示词的自动前缀缓存）。OpenAI/DeepSeek/Groq 的服务端隐式缓存从完全稳定的系统提示词受益。

4. **时间上下文**: 不再在系统提示词中。每条用户消息的 `[YYYY-MM-DD HH:MM:SS TZ]` 前缀提供时间上下文。这意味着：
   - 模型每轮从用户消息知道当前时间
   - 时间不被缓存（每轮变化），但仅给用户消息增加约 30 字节，不影响系统提示词
   - 需要精确时间戳的任务可使用工具调用（如 `shell_exec date`）

5. **Prompt-guided 工具注入**: 当 `native_tool_calling: false` 时，默认 Provider `chat()` 方法将工具指令追加到系统 `content`。如果系统消息有 `stable_prefix: Some(...)`，追加到 `content` 会打破分区不变性。目前没有任何提供器同时具有 `native_tool_calling: false` 和 `CacheStrategy::ExplicitAnthropic`，因此这种情况不会发生。

## 预期行为

| 轮次 | 系统提示词形状 | 缓存结果 |
|------|---------------|---------|
| 第 1 轮 | `[完整提示词(cache_control: ephemeral)]` (Anthropic/Bedrock) 或 `[完整提示词]` (其他) | 全量系统处理；≥1024 tokens 时缓存 |
| 第 2 轮 | 相同系统提示词（字节一致），用户消息带更新时间戳前缀 | 稳定前缀匹配 → 所有提供商 **缓存命中** |
| 第 N 轮 (无稳定变化) | 相同 | 每轮命中缓存（Anthropic 在 5 分钟 TTL 内，其他通过服务端前缀缓存） |
| 稳定变化 (技能/记忆/工具) | 新系统提示词内容 | 该轮缓存未命中；后续轮次建立新缓存 |

**预估节省**: Anthropic 缓存命中时稳定输入 tokens 以正常价格 ~10% 计费。典型 3k-token 稳定前缀，稳态下系统 tokens 成本降低约 90%（整个提示词被缓存，而非仅部分）。其他提供商通过服务端隐式前缀缓存免费受益。

## 验证

1. `cargo test -p clawseed-agent` — 系统提示词无 datetime 节、所有节为 Stable、分区构建 dynamic 为空
2. `cargo test -p clawseed-api` — ChatMessage serde 往返、system_partitioned
3. `cargo test -p clawseed-providers` — Anthropic/Bedrock 分区转换、DeepSeekAnthropicFactory
4. `cargo build` — 整个工作空间编译
5. `./tools/ci_local.sh` — fmt/clippy/test 通过
6. 手动：`clawseed chat` 对 Anthropic，2 轮 → 第 2 轮 `cache_read_input_tokens > 0`
7. 手动：`clawseed chat` 对 DeepSeek-anthropic → `cache_read_input_tokens > 0`
