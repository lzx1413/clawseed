# ClawSeed Memory System Upgrade Plan

## Context

ClawSeed 当前 memory 系统采用 3 层平面分类 (Core/Daily/Conversation) + 启发式合并，搜索用 BM25+向量加权融合(0.7/0.3)，冲突检测用纯 Jaccard 词集相似度。在分析腾讯开源的 TencentDB-Agent-Memory (TDAI) 后，识别出多个可借鉴的工程改进点——特别是 RRF 融合、最小保留守卫、Prompt Cache 分区注入、延迟嵌入等。这些改进不依赖重 LLM 调用，适合移动端 on-device 场景，且可增量交付。

---

## Phase A: RRF 混合搜索融合

**目标**: 用 Reciprocal Rank Fusion 替代当前加权平均融合，消除 BM25-向量尺度不匹配问题。

**问题**: 当前 `hybrid_merge()` 对 BM25 做逐查询最大值归一化，若 top-1 BM25 分数异常高，其余结果被压缩至近零。RRF 用排名位置而非原始分数，天然免归一化。

**实施步骤**:

1. **`crates/clawseed-memory/src/vector.rs`** — 新增 `rrf_merge(vector_results, keyword_results, k: u32, limit: usize)`:
   - 对每个列表按分数排序，分配排名 (0-based)
   - 每个 id 的分数 = `1/(k + rank + 1)`，出现在两个列表的 id 累加
   - 按累加分数降序排列，截取 limit
   - 保留原 `hybrid_merge()` 作为 `Weighted` 回退

2. **`crates/clawseed-api/src/memory_traits.rs`** — 新增 `MergeStrategy` enum：
   ```rust
   pub enum MergeStrategy {
       Rrf { k: u32 },           // k=60 是论文推荐值
       Weighted { vector_weight: f32, keyword_weight: f32 },
   }
   ```

3. **`crates/clawseed-config/src/schema/mod.rs`** — `MemoryConfig` 新增 `merge_strategy: Option<MergeStrategy>`，新增 helper `effective_merge_strategy()`:
   - `None` → 若 `vector_weight` 或 `keyword_weight` 显式设置（非 None）→ `Weighted`（保留用户意图），否则 → `Rrf { k: 60 }`
   - 保留 `vector_weight`/`keyword_weight` 字段不变（向后读取旧 config）

4. **`crates/clawseed-memory/src/sqlite.rs`** — `SqliteMemory` 新增 `merge_strategy` 字段，`recall()` 合并阶段 dispatch 到 `rrf_merge()` 或 `hybrid_merge()`。

5. **`crates/clawseed-memory/src/lib.rs`** — `create_memory_with_storage_and_routes()` 传递 merge_strategy。

**向后兼容**: 这是**默认行为迁移**。现有用户未显式设置 weight 时会从 Weighted 切换到 RRF。显式设置 `vector_weight = 0.7` → 保持旧行为。在 CHANGELOG 中标注。

**验证**: 对比 RRF vs Weighted 排序差异；BM25-only 模式正常；`cargo test -p clawseed-memory`。

**工作量**: ~2-3 天

---

## Phase B: 最小保留量守卫

**目标**: 防止 hygiene 过度清理，确保每个分类始终保留至少 N 条记忆。

**问题**: `prune_category_rows()` 无保留下限。在低频使用的工作空间（特别是移动端），一次 hygiene 可能清空所有 Daily/Conversation。

**实施步骤**:

1. **`crates/clawseed-config/src/schema/mod.rs`** — `MemoryConfig` 新增：
   - `min_retention_floor: Option<usize>` (default: None=无守卫)
   - `daily_retention_floor: Option<usize>` (default: None)
   - `conversation_retention_floor: Option<usize>` (default: None)
   - 全局 floor 兜底，per-category floor 可覆盖

2. **`crates/clawseed-memory/src/hygiene.rs`** — 修改（hygiene 入口是 `run_if_due(config, workspace_dir)`，在 `lib.rs:42/98` 直接调用，**不走 SqliteMemory**）:
   - `run_if_due()` 从 config 读取 floor 字段，传给 `prune_category_rows()`
   - `prune_category_rows(workspace_dir, category, retention_days, retention_floor)`:
     - 删除前 `SELECT COUNT(*) FROM memories WHERE category = ? AND superseded_by IS NULL`
     - 计算 `eligible = SELECT COUNT(*) WHERE category = ? AND updated_at < cutoff`
     - `allowed = max(0, total - floor)`
     - 若 `eligible ≤ allowed`：直接 `DELETE FROM memories WHERE category = ? AND updated_at < ?`
     - 若 `eligible > allowed`：先 `SELECT rowid FROM ... ORDER BY updated_at ASC LIMIT ?`，再 `DELETE FROM memories WHERE rowid IN (...)`（不用 `DELETE ... ORDER BY ... LIMIT`，兼容 Android SQLite）
   - Core 分类不受影响（现有逻辑已排除 Core）

**向后兼容**: `None` floor = 无守卫，与当前一致。

**验证**: floor=50 + 100 条 Conversation，hygiene 后剩余 ≥ 50。

**工作量**: ~1 天

---

## Phase D: 延迟嵌入 (Deferred Embedding)

**目标**: store 时先写 metadata+FTS（毫秒级），嵌入异步补填，消除 store 路径嵌入延迟。

**问题**: 当前 `store()` 同步计算嵌入后才写 DB。Android ONNX 推理 50-200ms，远程 API 100-500ms。

**实施步骤**:

1. **`crates/clawseed-config/src/schema/mod.rs`** — `MemoryConfig` 新增 `defer_embedding: Option<bool>`:
   - None 时：有 embedding_provider → true，无 → false

2. **`crates/clawseed-memory/src/sqlite.rs`** — Schema migration:
   - `ALTER TABLE memories ADD COLUMN embedding_content_hash TEXT DEFAULT NULL` — 直接可执行的 SQL
   - **历史数据回填**: SQLite 无内置 SHA-256 函数，`hash(content)` 是伪 SQL，不能直接执行。回填必须在 Rust 里逐行完成：
     1. `init_schema()` 后追加 Rust 回填逻辑：`SELECT id, content FROM memories WHERE embedding IS NOT NULL AND embedding_content_hash IS NULL`
     2. 对每行调用现有 `content_hash()` 函数（与 `embedding_cache` 使用的 SHA-256(content)[0:8] hex 一致）
     3. `UPDATE memories SET embedding_content_hash = ? WHERE id = ?` 批量写入
     4. 在 `spawn_blocking` 中执行，与现有 store/recall 的阻塞模式一致
   - `embedding_content_hash` 仅用于 deferred-update guard，不参与 recall/consistency 判断

3. **`crates/clawseed-memory/src/sqlite.rs`** — 修改 `store()` 和 `store_with_metadata()`:
   - 检查 embedding_cache：命中 → 直接写嵌入（无延迟）
   - 未命中 + `defer_embedding=true` → 写行 `embedding=NULL, embedding_content_hash=hash(content)`，FTS trigger 正常触发（立即可 BM25 搜索）
   - 后台任务分两段执行（与现有 store/recall 的阻塞模式一致）：
     1. **async 阶段**: `tokio::spawn` → 计算 embedding（调用 embedder.embed_one()，纯 async，不涉及 DB）
     2. **blocking 阶段**: `tokio::task::spawn_blocking` → 持有 `Arc<Mutex<Connection>>` 执行 `UPDATE memories SET embedding = ? WHERE key = ? AND embedding_content_hash = ?`
     3. 不能在 async runtime 线程上直接跑 rusqlite 操作（会阻塞 executor）
   - hash 不匹配（content 已变）→ UPDATE 影响 0 行，静默放弃，新 content 的嵌入在下次 store 或 backfill 计算
   - 嵌入失败 → 保持 NULL，backfill 补救

4. **`crates/clawseed-memory/src/sqlite.rs`** — `recall()` 已正确处理 `embedding IS NULL`：vector_search 只查非 NULL，BM25/LIKE 不依赖嵌入。

5. **生命周期管理** — 不修改 Memory trait：
   - `SqliteMemory` 内部用 `Arc<AtomicUsize>` 跟踪 pending 任务
   - `Drop impl` 等待计数归零或超时（5s）
   - Gateway 场景（`AppState.mem` 永不释放）= fire-and-forget；chat 场景（agent 独占）= 正确 drain

**向后兼容**: `defer_embedding=false` 或无 provider → 同步嵌入。

**验证**: store → BM25 立即可搜；30s 后嵌入填充；并发 store+defer；后台失败；同 key A→B upsert 竞态。

**工作量**: ~3 天

---

## Phase C: Prompt Cache 优化（稳定区 + 动态区分区注入）

**目标**: Core 记忆注入系统提示词（可缓存区），per-turn 新记忆注入用户消息（动态区），利用 prompt caching 节省 token。

**问题**: 当前所有记忆注入在 `[Memory context]` 前缀到用户消息（动态区），每轮重新计费。

**实施步骤**:

1. **`crates/clawseed-config/src/schema/mod.rs`** — `MemoryConfig` 新增 `stable_memory_in_system_prompt: Option<bool>`:
   - None 时 auto_recall=true 则 true

2. **`crates/clawseed-api/src/memory_traits.rs`** — `MemoryEntry` 新增 `embedding: Option<Vec<f32>>` (`#[serde(default)]`); `Memory` trait 新增 default impl `top_core_memories(limit)`:
   ```rust
   async fn top_core_memories(&self, limit: usize) -> anyhow::Result<Vec<MemoryEntry>> {
       let entries = self.list(Some(&MemoryCategory::Core), None).await?;
       let mut sorted = entries;
       sorted.sort_by(|a, b| {
           b.importance.unwrap_or(0.5).partial_cmp(&a.importance.unwrap_or(0.5)).unwrap_or(std::cmp::Ordering::Equal)
       });
       sorted.truncate(limit);
       Ok(sorted)
   }
   ```

3. **`crates/clawseed-memory/src/sqlite.rs`** — `SqliteMemory` **必做**覆写 `top_core_memories()`:
   - `SELECT ... WHERE category='core' AND superseded_by IS NULL ORDER BY importance DESC LIMIT ?`
   - 这是必做项（default impl 受 `DEFAULT_LIST_LIMIT=1000` 限制，Core 不会被 hygiene 清理，无上限约束）
   - default impl 仅作为 NoneMemory 等其他实现者的 fallback

4. **`crates/clawseed-agent/src/agent.rs`** — `turn()` / `turn_streamed()`:
   - 每轮入口调用 `memory.top_core_memories(auto_recall_limit)`，对比 `injected_core_state: HashMap<String, String>`（key → content hash）
   - key 集合或任一 content hash 有变化 → `rebuild_system_prompt()` 更新稳定区 + 更新 HashMap
   - dynamic auto-recall 过滤掉 HashMap 中已有的 key（按 key+content hash 去重）
   - 新增 Core → 插入 HashMap；content 更新 → 更新 HashMap + rebuild；key superseded → 移除 + rebuild

5. **`crates/clawseed-agent/src/cron/scheduler.rs`** — **不改**。cron 保留现有 query-based `memory.recall(&prompt, 5, ...)`，stable/dynamic 分区仅用于 agent 多轮会话（cron 一次性执行，无 prompt caching 收益，改 importance 排序会丢 query relevance）。

6. **`crates/clawseed-agent/src/prompt.rs`** — `PromptContext` 新增 `stable_core_memories: Vec<MemoryEntry>`; `SystemPromptBuilder` 新增 `StableMemorySection` 渲染器。

**向后兼容**: `stable_memory_in_system_prompt=false` → 全部注入用户消息，与当前一致。

**验证**: Anthropic prompt caching cache hit；Core 变化触发 rebuild；key+content hash 去重不遗漏；Core 规模 > 1000 时 `top_core_memories()` 正确排序。

**工作量**: ~3-4 天

---

## Phase E: 改进冲突检测（多信号融合 + 启发式矛盾检测）

**目标**: 用 embedding cosine + BM25 overlap + Jaccard 加权融合替代纯 Jaccard，加启发式矛盾信号检测。

**问题**: 纯 Jaccard 噪声大——高词重叠非矛盾误触发，低词重叠语义矛盾漏掉。

**实施步骤**:

1. **`crates/clawseed-api/src/memory_traits.rs`** — `MemoryEntry` 新增 `embedding: Option<Vec<f32>>` (`#[serde(default)]`，Phase C 已加); 新增 `ConflictMode` enum:
   ```rust
   pub enum ConflictMode {
       Jaccard,                          // 现有行为
       Combined { jaccard_w: f32, cosine_w: f32, bm25_w: f32 },
   }
   ```

2. **`crates/clawseed-memory/src/conflict.rs`** — 新增 `combined_similarity()`:
   - 参数: `content_a, content_b, emb_a: Option<Vec<f32>>, emb_b: Option<Vec<f32>>, weights`
   - **退化规则**: 若 `emb_a.is_none() || emb_b.is_none()` → 直接返回 `jaccard_similarity(content_a, content_b)`，不做 weight renormalization。明确：embedding 任一侧缺失 = 切回 `ConflictMode::Jaccard`，不是"权重转移"
   - 两侧 embedding 都存在 → `0.4*jaccard + 0.4*cosine + 0.2*bm25_overlap`
   - 新增 `detect_contradiction_signals()` — 启发式矛盾检测（否定词翻转、偏好变更、时间矛盾）
   - 修改 `find_text_conflicts()` 使用 `combined_similarity`

3. **`crates/clawseed-memory/src/sqlite.rs`** — `SqliteMemory` 的 `recall()`/`list()` 构建 MemoryEntry 时从 DB 读取 embedding BLOB 反序列化为 `Vec<f32>`。

4. **`crates/clawseed-config/src/schema/mod.rs`** — `MemoryConfig` 新增 `conflict_mode: Option<ConflictMode>` (None → Combined 默认权重 0.4/0.4/0.2)。不引入 LLM 路径。

**向后兼容**: `ConflictMode::Jaccard` = 现有行为；embedding 缺失时自动切回 Jaccard。移动端完全可用。

**验证**: Jaccard vs Combined 检测率对比；embedding NULL 退化行为。

**工作量**: ~3 天

---

## Phase F (未来): Topic 聚合 & Persona 生成

在 Core 之上新增 Topic (L2) 和 Persona (L3) 层，实现 TDAI 4 层金字塔。需要 LLM 调用，仅桌面/服务器 opt-in。依赖 Phase A-E 稳定。不在本期范围。

---

## 实施顺序

```
Phase A (RRF)        ── 独立，无依赖     ── 2-3 天
Phase B (保留守卫)    ── 独立，无依赖     ── 1 天
Phase D (延迟嵌入)    ── 独立，但需与 A 协调 ── 3 天
Phase C (Prompt Cache) ── 依赖 A+D 稳定   ── 3-4 天
Phase E (冲突检测)    ── 依赖 A+D 稳定   ── 3 天
──────────────────────────────────────────────
总计: ~12-14 天
```

推荐 A → B → D → C → E。

---

## 向后兼容策略

所有新 config 字段用 `Option<T>` + `None` 默认值：
- **Phase A**: `merge_strategy: None` → 默认 `Rrf { k: 60 }`（默认行为迁移）。显式 weight → `Weighted`
- **Phase B**: `min_retention_floor: None` → 无守卫
- **Phase D**: `defer_embedding: None` → 有 provider 时 true；`embedding_content_hash` 仅用于 deferred-update guard；启动时回填历史数据
- **Phase C**: `stable_memory_in_system_prompt: None` → auto_recall=true 时 true；`top_core_memories()` SqliteMemory 覆写为必做项；去重用 `HashMap<String, String>`（key → content hash）；cron 不改
- **Phase E**: `conflict_mode: None` → `Combined`；embedding 任一侧缺失 → 切回 Jaccard（不做 renormalization）

不修改现有 API trait 方法签名（只加 default impl 和 struct 字段）。DB schema 只加列。config.toml 必填字段不变。

---

## 验证策略

每个 Phase 完成后：
1. `cargo test -p clawseed-memory` — 单元测试覆盖新逻辑
2. `cargo test -p clawseed-agent` — 集成测试覆盖 auto-recall/consolidation
3. `./tools/ci_local.sh` — fmt + clippy + 全量测试
4. 手动端到端：`clawseed chat` 验证 recall 排序、hygiene 保留量、store 速度、prompt 缓存效果

---

## Review 修正总结

三轮 review 共发现 13 个问题，全部验证并修正：

| # | 问题 | 修正 |
|---|---|---|
| 1 | Phase D upsert UUID 不匹配 | → 被 #8 的 content_hash 方案取代 |
| 2 | Phase C recall() 无 category filter | 新增 `top_core_memories()` trait default impl |
| 3 | Phase E MemoryEntry 缺 embedding | `MemoryEntry` 新增 `embedding: Option<Vec<f32>>` |
| 4 | Phase D shutdown/drain 无 trait hook | `SqliteMemory::Drop` 等待 pending 计数归零 |
| 5 | Phase A 默认行为迁移而非完全向后兼容 | 明确标注，显式 weight → 保持 Weighted |
| 6 | Phase B DELETE LIMIT 不通用 | 有界 rowid 子查询代替 |
| 7 | Phase C cron 路径遗漏 | → 被 #10 取代：cron 保留 query-based 不改 |
| 8 | Phase D UPDATE-by-key 竞态 | `embedding_content_hash` 列 + hash 守卫 |
| 9 | Phase C rebuild 无回调机制 | per-turn 主动检查 Core 变化 → rebuild |
| 10 | Phase C cron 改 importance 是行为回退 | cron 保留现有 query-based recall |
| 11 | Phase B hygiene 不走 SqliteMemory | 改 hygiene.rs + MemoryConfig，不改 sqlite.rs |
| 12 | Phase E 退化语义矛盾（"等效 Jaccard" vs 含 bm25_overlap） | embedding 缺失 → 直接切回 Jaccard，不做 renormalization |
| 13 | top_core_memories() default impl 受 LIMIT | SqliteMemory 覆写为必做项，default impl 仅 fallback |

额外补充项（非 blocking 但已纳入文档）：
- Phase C 去重状态从 `HashSet<String>` 改为 `HashMap<String, String>`（key → content hash）
- Phase D `embedding_content_hash` 启动时回填历史数据规则已明确