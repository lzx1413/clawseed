# Prompt 缓存优化 — 实现总结

## 背景

ClawSeed 与 Anthropic 的多轮对话**缓存命中率约为 0%**，原因是系统提示词每轮都在变化（秒级精度的 datetime 位于位置 0），且以单个 `SystemBlock` 发送。Anthropic 的 prompt 缓存基于精确前缀匹配——任意位置的任何变化都会打破整个缓存链。这在多轮对话中每轮浪费约 2–4 KB / 1–3k tokens。

## 根因

三个致命的缓存破坏问题：

1. **`DateTimeSection` 位于位置 0** — 内容每秒变化，在最开头就与前缀不一致
2. **单个 `SystemBlock`** — 任何一个字符变化都要重新处理整个块
3. **稳定内容变化时全量重建** — Core 记忆 / 技能 / 工具变化会替换整个系统消息内容

## 实现

### Phase 0 — 降低 DateTime 精度

**文件**: `crates/clawseed-agent/src/prompt.rs`

将 `DateTimeSection::build()` 从秒级精度改为分钟级精度：

```rust
// 修改前：
"Date: {year:04}-{month:02}-{day:02}\nTime: {hour:02}:{minute:02}:{second:02} ({tz})"
// 修改后：
"Date: {year:04}-{month:02}-{day:02}\nTime: {hour:02}:{minute:02} ({tz})"
```

**权衡**: 分钟精度意味着同一分钟内可命中缓存。多轮对话通常在 Anthropic 的 5 分钟缓存 TTL 内，因此缓存收益得以保留，同时保持了实用的时间粒度。模型不再知道秒级时间，但仍可精确到分钟——足以满足所有实际任务需求。

**收益**: 广泛——不仅对 Anthropic，也改善了 OpenAI/DeepSeek 等提供商的服务端隐式前缀缓存命中率。

### Phase 1 — 稳定/动态分区

#### 1. CacheClass + PartitionedSystemPrompt (`prompt.rs`)

- `CacheClass` 枚举：`Stable` / `Dynamic`
- `PromptSection::cache_class()` 默认方法 → `Stable`
- `DateTimeSection::cache_class()` 覆写 → `Dynamic`
- `PartitionedSystemPrompt { stable, dynamic, full }` 结构体
- `SystemPromptBuilder::build_partitioned()` — 将 `Stable` 路由到 `stable_buf`，`Dynamic` 路由到 `dynamic_buf`
- `SystemPromptBuilder::build_dynamic()` — 仅重建 `Dynamic` 部分（用于逐轮刷新）

**前置声明 (Preamble)**：当两个部分都不为空时，在 stable 缓冲区的末尾追加前置声明 `⚠️ THE CURRENT TIME BELOW APPLIES TO ALL ABOVE INSTRUCTIONS.`。这填补了将 datetime 从位置 0 移到末尾带来的语义缺口。前置声明位于 stable 块内部，属于可缓存前缀的一部分，且永远不变。

**`full` 中的节顺序**：
```
// 分区前 (legacy build()):
[DateTime] → [Identity] → [Platform] → [Workspace] → ... → [Skills]

// 分区后 (build_partitioned().full):
[Identity] → [Platform] → [Workspace] → ... → [Skills] → [preamble] → [DateTime]
```

前置声明让模型将时间视为适用于所有前面指令的信息，减轻了 datetime 移到末尾带来的语义影响。

#### 2. ChatMessage 上的 stable_prefix (`clawseed-api/src/provider.rs`)

```rust
pub struct ChatMessage {
    pub role: String,
    pub content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stable_prefix: Option<String>,
}
```

- `ChatMessage::system_partitioned(stable, dynamic, full)` — 设置 `content = full`，`stable_prefix = Some(stable)`
- 其他构造器 (`system`, `user`, `assistant`, `tool`) 设置 `stable_prefix: None`
- `stable_prefix` 不持久化到会话存储——由 `seed_history` 在恢复时重建
- Serde: `#[serde(default)]` 保证与缺少该字段的 JSON 的向后兼容；`#[serde(skip_serializing_if)]` 在值为 `None` 时省略

#### 3. Agent 使用分区提示词 (`agent.rs`)

Agent 新增字段：
- `stable_system_content: String` — 缓存的稳定部分（不会逐轮重建）
- `dynamic_system_content: String` — 缓存的动态部分（逐轮重建）

新增方法：
- `build_system_prompt_partitioned()` — 完整分区构建，存储两部分到 Agent
- `build_dynamic_system_content()` — 仅重建 Dynamic 部分 (datetime)
- `refresh_dynamic_system_content()` — 调用 `build_dynamic_system_content()`，从缓存的 `stable_system_content` + preamble + 新动态内容重建 `full`，更新历史中的系统 ChatMessage

修改的方法：
- `prepare_turn()` — 第一轮：`build_system_prompt_partitioned()`，推入 `ChatMessage::system_partitioned(stable, dynamic, full)`
- `rebuild_system_prompt()` — 稳定内容变化时（技能/记忆/工具）全量重建，使用分区构建
- `seed_history()` — 丢弃旧系统消息，从当前上下文通过分区构建重建
- `turn()` / `turn_streamed()` — 在后续轮次的 `prepare_turn()` 之后调用 `refresh_dynamic_system_content()`

**逐轮开销**: 仅重建 1 个 Dynamic 部分 (DateTimeSection)，而非完整的 11 节流水线。`stable_system_content` 从缓存值读取——不重建。

#### 4. Anthropic 提供器 (`anthropic.rs`)

在 `convert_messages()` 中：
- 捕获系统消息的 `msg.stable_prefix` 和 `msg.content`
- 当 `stable_prefix` 为 `Some(stable)` 且 content 以 `stable` 开头时：
  - 输出 `SystemPrompt::Blocks([stable_block(cache_control), dynamic_block(no_cache)])`
  - 从动态部分剥离前置声明（前置声明仅对直接读取 `content` 的非 Anthropic 提供器有用）
- 否则：单块带 `cache_control`（传统路径）
- 防御：`text.starts_with(&stable)` 和 `!stable.is_empty()` 守卫防止 content/stable_prefix 不同步时的错误切片

#### 5. Bedrock 提供器 (`bedrock.rs`)

在 `convert_messages()` 中：
- 当 `stable_prefix` 为 `Some(stable)` 且 content 以 `stable` 开头时：
  - 输出 `SystemBlock::Text(stable)` + `SystemBlock::CachePoint` + `SystemBlock::Text(dynamic)`
  - 从动态部分剥离前置声明
- 否则：单个 `SystemBlock::Text(content)`（传统路径）
- 防御：相同的 `starts_with` 和 `is_empty` 守卫
- 在 `chat()` 中：当分区已提供 CachePoint 时，跳过冗余的后续 CachePoint 插入

### 缓存断点预算

Anthropic 限制每个请求最多 4 个断点。Phase 1 不增加断点数量：

| 位置 | 修改前 | Phase 1 后 |
|---|---|---|
| OAuth 前缀块 | 0 或 1 | 0 或 1 |
| 系统提示词 | 1 (单块) | 1 (仅稳定块) |
| 最后对话消息 | 0 或 1 | 0 或 1 |
| 工具结果 | 0 或 1 | 0 或 1 |
| **最大总数** | **4** | **4** |

## 已知限制

1. **稳定块重建会打破该轮缓存**。触发条件：`memory_store` 新增 Core 记忆、技能激活/停用、远程工具注册。下一轮会重新建立缓存。稳定会话中这种情况很少。

2. **最小可缓存前缀为 1024 tokens** (Sonnet/Opus)。紧凑配置（最小 personality、无技能、无 Core 记忆）可能达不到阈值，不会被缓存。

3. **提供器覆盖范围**: Anthropic + Bedrock 已接入。Gemini、OpenAI、DeepSeek、Ollama 的 `prompt_caching: false` 且忽略 `stable_prefix`。OpenAI/DeepSeek 的服务端隐式缓存仍受益于 Phase 0 (分钟精度)。

4. **Datetime 位置**: 在分区的 `full` 字符串中，datetime 出现在末尾而非位置 0。前置声明填补了这一语义缺口。非 Anthropic 提供器看到 `content` 中 datetime 在末尾——前置声明使这一点明确。

5. **分钟精度权衡**: 模型不再知道秒级时间。需要精确时间戳的任务应使用工具调用（如 `shell_exec date`）。这个权衡对于缓存收益是可接受的。

6. **Prompt-guided 工具注入**: 当 `native_tool_calling: false` 时，默认 Provider `chat()` 方法将工具指令追加到系统 `content`。如果系统消息有 `stable_prefix: Some(...)`，追加到 `content` 会打破分区不变性。目前没有任何提供器同时具有 `native_tool_calling: false` 和 `prompt_caching: true`，因此这种情况不会发生。

## 预期行为

| 轮次 | 系统提示词形状 | 缓存结果 |
|------|---------------|---------|
| 第 1 轮 | `[stable_block(cache_control)] + [preamble(在 stable 内)] + [dynamic_block]` | 全量系统处理；稳定前缀在 ≥1024 tokens 时缓存 |
| 第 2 轮 | 相同的稳定块，新动态内容 (datetime 刷新到当前分钟) | 稳定前缀匹配 → **缓存命中**；仅动态块重新处理 |
| 第 N 轮 (无稳定变化) | 相同 | 稳定部分在 5 分钟 TTL 内每轮命中缓存 |
| 稳定变化 (技能/记忆/工具) | 新稳定块内容 | 该轮缓存未命中；后续轮次建立新缓存 |

**预估节省**: 缓存命中时稳定输入 tokens 以正常价格 ~10% 计费。典型 3k-token 稳定前缀加 ~50-token 动态块，稳态下系统 tokens 成本降低约 80–85%。对话级缓存也受益，因为系统前缀现在稳定了。

## 验证

1. `cargo test -p clawseed-agent` — datetime 分钟精度、缓存类别默认值、分区构建、build_dynamic
2. `cargo test -p clawseed-api` — ChatMessage serde 往返、system_partitioned
3. `cargo test -p clawseed-providers` — Anthropic/Bedrock 分区转换
4. `cargo build` — 整个工作空间编译
5. `./tools/ci_local.sh` — fmt/clippy/test 通过
6. 手动：`clawseed chat` 对 Anthropic，同一分钟内发 2 轮 → 第 2 轮 `cache_read_input_tokens > 0`
