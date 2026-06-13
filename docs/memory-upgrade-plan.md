# ClawSeed Memory System Upgrade Plan

## Context

ClawSeed 当前 memory 系统采用 3 层平面分类 (Core/Daily/Conversation) + 启发式合并，搜索用 BM25+向量加权融合(0.7/0.3)，冲突检测用纯 Jaccard 词集相似度。在分析腾讯开源的 TencentDB-Agent-Memory (TDAI) 后，识别出多个可借鉴的工程改进点——特别是 RRF 融合、最小保留守卫、Prompt Cache 分区注入、延迟嵌入等。这些改进不依赖重 LLM 调用，适合移动端 on-device 场景，且可增量交付。

---

## 实施状态总览

| Phase | 目标 | 状态 | 完成日期 |
|-------|------|------|----------|
| Phase A | RRF 混合搜索融合 | ✅ 已完成 | — |
| Phase B | 最小保留量守卫 | ✅ 已完成 | — |
| Phase D | 延迟嵌入 (Deferred Embedding) | ✅ 已完成 | — |
| Phase C | Prompt Cache 优化（稳定区 + 动态区分区注入） | ✅ 已完成 | — |
| Phase E | 改进冲突检测（多信号融合 + 启发式矛盾检测） | ✅ 已完成 | — |
| Phase F | Topic 聚合 & Persona 生成 | 🔜 未来规划 | — |

> 所有 Phase A-E 均已落地实现并通过测试验证。Phase F 依赖 LLM 调用，仅桌面/服务器 opt-in，不在本期范围。

---

## Phase A: RRF 混合搜索融合 ✅

**目标**: 用 Reciprocal Rank Fusion 替代当前加权平均融合，消除 BM25-向量尺度不匹配问题。

**问题**: 当前 `hybrid_merge()` 对 BM25 做逐查询最大值归一化，若 top-1 BM25 分数异常高，其余结果被压缩至近零。RRF 用排名位置而非原始分数，天然免归一化。

**实施结果**:

1. **`vector.rs`** — 新增 `rrf_merge(vector_results, keyword_results, k: u32, limit: usize)`，保留原 `hybrid_merge()` 作为 `Weighted` 回退。

2. **`clawseed-api`** — 新增 `MergeStrategy` enum：`Rrf { k: u32 }` 和 `Weighted { vector_weight, keyword_weight }`。

3. **`clawseed-config`** — `MemoryConfig` 新增 `merge_strategy: Option<MergeStrategy>`，helper `effective_merge_strategy()`：
   - `None` → 若 `vector_weight`/`keyword_weight` 显式设置 → `Weighted`（保留用户意图），否则 → `Rrf { k: 60 }`

4. **`sqlite.rs`** — `SqliteMemory` 新增 `merge_strategy` 字段，`recall()` 合并阶段 dispatch 到 `rrf_merge()` 或 `hybrid_merge()`。

5. **`lib.rs`** — `create_memory_with_storage_and_routes()` 传递 merge_strategy。

**向后兼容**: 默认行为从 Weighted 切换到 RRF。显式设置 `vector_weight = 0.7` → 保持旧行为。

---

## Phase B: 最小保留量守卫 ✅

**目标**: 防止 hygiene 过度清理，确保每个分类始终保留至少 N 条记忆。

**问题**: `prune_category_rows()` 无保留下限。在低频使用的工作空间（特别是移动端），一次 hygiene 可能清空所有 Daily/Conversation。

**实施结果**:

1. **`clawseed-config`** — `MemoryConfig` 新增：
   - `min_retention_floor: Option<usize>` (全局 floor 兜底)
   - `daily_retention_floor: Option<usize>`
   - `conversation_retention_floor: Option<usize>`
   - per-category floor 可覆盖全局 floor

2. **`hygiene.rs`** — `prune_category_rows()` 接受 `retention_floor` 参数：
   - 删除前统计非取代条目总数
   - 计算 `allowed_to_delete = max(0, total - floor)`
   - 若 `eligible ≤ allowed`：直接删除全部过期条目
   - 若 `eligible > allowed`：使用 rowid 子查询删除最旧的 N 条（兼容 Android SQLite）
   - Core 分类不受影响（现有逻辑已排除 Core）
   - 仅非取代条目计入 floor；取代条目仍可修剪

**向后兼容**: `None` floor = 无守卫，与当前行为一致。

---

## Phase D: 延迟嵌入 (Deferred Embedding) ✅

**目标**: store 时先写 metadata+FTS（毫秒级），嵌入异步补填，消除 store 路径嵌入延迟。

**问题**: 当前 `store()` 同步计算嵌入后才写 DB。Android ONNX 推理 50-200ms，远程 API 100-500ms。

**实施结果**:

1. **`clawseed-config`** — `MemoryConfig` 新增 `defer_embedding: Option<bool>`：None 时有 embedding_provider → true，无 → false。

2. **`sqlite.rs`** — Schema migration 新增 `embedding_content_hash TEXT DEFAULT NULL` 列，启动时 Rust 回填已有行的 hash（使用与 `embedding_cache` 一致的 SHA-256(content)[0:8] hex）。

3. **`sqlite.rs`** — `store()` 和 `store_with_metadata()` 修改：
   - 嵌入缓存命中 → 直接写嵌入（无延迟）
   - 未命中 + `defer_embedding=true` → 写行 `embedding=NULL, embedding_content_hash=hash(content)`，FTS trigger 正常触发（立即可 BM25 搜索）
   - 后台任务分两段：async 阶段计算嵌入，blocking 阶段写 DB
   - hash 不匹配（内容已变）→ UPDATE 影响 0 行，静默放弃
   - 嵌入失败 → 保持 NULL，backfill 补救

4. **`sqlite.rs`** — `recall()` 正确处理 `embedding IS NULL`：vector_search 只查非 NULL，BM25/LIKE 不依赖嵌入。

5. **生命周期管理** — `SqliteMemory` 内部用 `Arc<AtomicUsize>` 跟踪 pending 任务，`Drop impl` 等待计数归零或超时（5s）。Gateway 场景 fire-and-forget，chat 场景正确 drain。

**向后兼容**: `defer_embedding=false` 或无 provider → 同步嵌入，与当前行为一致。

---

## Phase C: Prompt Cache 优化（稳定区 + 动态区分区注入） ✅

**目标**: Core 记忆注入系统提示词（可缓存区），per-turn 新记忆注入用户消息（动态区），利用 prompt caching 节省 token。

**问题**: 当前所有记忆注入在 `[Memory context]` 前缀到用户消息（动态区），每轮重新计费。

**实施结果**:

1. **`clawseed-config`** — `MemoryConfig` 新增 `stable_memory_in_system_prompt: Option<bool>`。

2. **`clawseed-api`** — `MemoryEntry` 新增 `embedding: Option<Vec<f32>>` (`#[serde(default)]`); `Memory` trait 新增 default impl `top_core_memories(limit)`。

3. **`sqlite.rs`** — `SqliteMemory` 覆写 `top_core_memories()`：`SELECT ... WHERE category='core' AND superseded_by IS NULL ORDER BY importance DESC LIMIT ?`。default impl 仅作为 NoneMemory 等的 fallback。

4. **`agent.rs`** — `turn()` / `turn_streamed()`：
   - 每轮入口调用 `memory.top_core_memories(auto_recall_limit)`，对比 `injected_core_state: HashMap<String, String>`（key → content hash）
   - key 集合或 content hash 变化 → rebuild system prompt 更新稳定区 + 更新 HashMap
   - dynamic auto-recall 过滤掉 HashMap 中已有的 key
   - 新增 Core → 插入 HashMap；content 更新 → 更新 + rebuild；key superseded → 移除 + rebuild

5. **`prompt.rs`** — `PromptContext` 新增 `stable_core_memories: Vec<MemoryEntry>`; `SystemPromptBuilder` 新增 `StableMemorySection` 渲染器。

6. **`cron/scheduler.rs`** — 不改。cron 保留现有 query-based `memory.recall()`，stable/dynamic 分区仅用于 agent 多轮会话。

**独立实现文档**: `docs/prompt-cache-optimization-implementation.md` 记录了更详细的 Phase 0（DateTime 精度降低）和 Phase 1（稳定/动态分区）的实现细节、验证步骤和已知限制。

**向后兼容**: `stable_memory_in_system_prompt=false` → 全部注入用户消息，与当前一致。

---

## Phase E: 改进冲突检测（多信号融合 + 启发式矛盾检测） ✅

**目标**: 用 embedding cosine + BM25 overlap + Jaccard 加权融合替代纯 Jaccard，加启发式矛盾信号检测。

**问题**: 纯 Jaccard 噪声大——高词重叠非矛盾误触发，低词重叠语义矛盾漏掉。

**实施结果**:

1. **`clawseed-api`** — `MemoryEntry` 新增 `embedding: Option<Vec<f32>>` (`#[serde(default)]`，Phase C 已加); 新增 `ConflictMode` enum：`Jaccard` 和 `Combined { jaccard_w, cosine_w, bm25_w }`。

2. **`conflict.rs`** — 新增 `combined_similarity()`：
   - **退化规则**: 若任一侧 embedding 缺失 → 直接切回 `jaccard_similarity()`，不做 weight renormalization
   - 两侧 embedding 都存在 → `0.4*jaccard + 0.4*cosine + 0.2*bm25_overlap`
   - 新增 `detect_contradiction_signals()` — 启发式矛盾检测（否定词翻转 0.4、偏好变更 0.3、时间矛盾 0.3）
   - 修改 `find_text_conflicts()` 使用 `combined_similarity`

3. **`sqlite.rs`** — `recall()`/`list()` 构建 MemoryEntry 时从 DB 读取 embedding BLOB 反序列化为 `Vec<f32>`。

4. **`clawseed-config`** — `MemoryConfig` 新增 `conflict_mode: Option<ConflictMode>` (None → Combined 默认权重 0.4/0.4/0.2)。

**向后兼容**: `ConflictMode::Jaccard` = 现有行为；embedding 缺失时自动切回 Jaccard。移动端完全可用。

---

## Phase F (未来): Topic 聚合 & Persona 生成 🔜

在 Core 之上新增 Topic (L2) 和 Persona (L3) 层，实现 TDAI 4 层金字塔。需要 LLM 调用，仅桌面/服务器 opt-in。依赖 Phase A-E 稳定。不在本期范围。

---

## 实施顺序（已完成）

```
Phase A (RRF)        ── ✅ 已完成
Phase B (保留守卫)    ── ✅ 已完成
Phase D (延迟嵌入)    ── ✅ 已完成
Phase C (Prompt Cache) ── ✅ 已完成
Phase E (冲突检测)    ── ✅ 已完成
──────────────────────────────────────────────
Phase F (Topic/Persona) ── 🔜 未来规划
```

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

每个 Phase 完成后均已验证：
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
