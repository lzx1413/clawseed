# clawseed-memory — 记忆存储与检索

## 概述

`clawseed-memory` 提供 SQLite 支持的记忆存储，具备混合搜索（BM25 关键词 + 向量嵌入）、Reciprocal Rank Fusion (RRF) 排序、多信号冲突检测、延迟嵌入、LLM 驱动的记忆策展人、文本分块和生命周期管理（整合、卫生、快照）。

## 架构

```
┌──────────────────────────────────────────────────────────┐
│                    Memory Trait                            │
│  store / recall / forget / purge / export / top_core / … │
└──────────────────────────┬───────────────────────────────┘
                           │
          ┌────────────────┴────────────────┐
          │                                 │
   ┌──────┴──────┐                   ┌──────┴──────┐
   │ SqliteMemory│                   │ NoneMemory  │
   │  (默认)     │                   │  (兜底)     │
   └──────┬──────┘                   └─────────────┘
          │
   ┌──────┴───────────────────────────────────────────┐
   │               检索管线 (Pipeline)                  │
   │  ┌─────────┐   ┌──────────────┐  ┌───────────┐ │
   │  │  热缓存  │   │   BM25       │  │  向量     │ │
   │  │  (LRU)  │──▶│  关键词搜索  │  │  余弦相似 │ │
   │  │         │   │  (FTS5)      │  │  度       │ │
   │  └─────────┘   └──────┬───────┘  └─────┬─────┘ │
   │                       └──────┬──────────┘       │
   │                              ↓                   │
   │                     融合策略 (Merge)             │
   │                ┌─────────┬──────────┐           │
   │                │  RRF    │ Weighted  │           │
   │                │(默认)   │ (旧版)    │           │
   │                └─────────┴──────────┘           │
   └──────────────────────────────────────────────────┘
          │
   ┌──────┴───────────────────────────────────────────┐
   │             生命周期管理                           │
   │  ┌──────────────┐ ┌─────────┐ ┌───────────────┐ │
   │  │ 记忆整合     │ │ 记忆卫生 │ │  冲突检测     │ │
   │  │ (Daily→Core) │ │(定期清理)│ │  (多信号)     │ │
   │  └──────────────┘ └─────────┘ └───────────────┘ │
   │  ┌──────────────┐ ┌─────────────────────────┐   │
   │  │  快照        │ │  延迟嵌入               │   │
   │  │ (导出/水合)  │ │  (异步回填)             │   │
   │  └──────────────┘ └─────────────────────────┘   │
   │  ┌──────────────┐ ┌─────────────────────────┐   │
   │  │  策展人      │ │  分块器                 │   │
   │  │ (LLM清理)    │ │  (Markdown拆分)         │   │
   │  └──────────────┘ └─────────────────────────┘   │
   └──────────────────────────────────────────────────┘
```

## 核心模块

### Memory trait（定义在 clawseed-api）

```rust
#[async_trait]
pub trait Memory: Send + Sync {
    fn name(&self) -> &str;
    async fn store(&self, key: &str, content: &str, category: MemoryCategory, session_id: Option<&str>) -> Result<()>;
    async fn store_with_metadata(&self, key: &str, content: &str, category: MemoryCategory,
                                  session_id: Option<&str>, namespace: &str, importance: Option<f64>) -> Result<()>;
    async fn get(&self, key: &str) -> Result<Option<MemoryEntry>>;
    async fn recall(&self, query: &str, limit: usize, session_id: Option<&str>,
                    since: Option<&str>, until: Option<&str>, search_mode: Option<SearchMode>) -> Result<Vec<MemoryEntry>>;
    async fn recall_with_embeddings(&self, query: &str, limit: usize, session_id: Option<&str>,
                                     since: Option<&str>, until: Option<&str>, search_mode: Option<SearchMode>) -> Result<Vec<MemoryEntry>>;
    async fn recall_namespaced(&self, namespace: &str, query: &str, limit: usize, session_id: Option<&str>,
                                since: Option<&str>, until: Option<&str>, search_mode: Option<SearchMode>) -> Result<Vec<MemoryEntry>>;
    async fn list(&self, category: Option<&MemoryCategory>, session_id: Option<&str>) -> Result<Vec<MemoryEntry>>;
    async fn forget(&self, key: &str) -> Result<bool>;
    async fn purge_namespace(&self, namespace: &str) -> Result<usize>;
    async fn purge_session(&self, session_id: &str) -> Result<usize>;
    async fn count(&self) -> Result<usize>;
    async fn export(&self, filter: Option<MemoryExportFilter>) -> Result<Vec<MemoryEntry>>;
    async fn health_check(&self) -> bool;
    // 默认实现：
    async fn top_core_memories(&self, limit: usize) -> Result<Vec<MemoryEntry>>;  // SqliteMemory 覆写
    async fn reindex(&self) -> Result<usize>;
    async fn backfill_embeddings(&self, batch_size: usize) -> Result<usize>;
}
```

### sqlite.rs — SQLite 后端

**数据库 Schema**：

| 表 | 字段 | 说明 |
|----|------|------|
| `memories` | id, key, content, category, embedding (BLOB), created_at, updated_at, session_id, namespace, importance, superseded_by, embedding_content_hash | 记忆存储（嵌入内联存储） |
| `memories_fts` | key, content（FTS5 虚拟表，基于 `memories`） | 全文搜索索引 |
| `embedding_cache` | content_hash, embedding (BLOB), created_at, accessed_at | LRU 淘汰嵌入缓存 |

**PRAGMA 调优**：WAL 模式（并发读）、`synchronous=NORMAL`（2× 写速度）、`mmap_size=8MB`、`cache_size=-2000`、`temp_store=MEMORY`。

**迁移系统**：每次打开时自动升级——添加缺失列（`session_id`、`namespace`、`importance`、`superseded_by`、`embedding_content_hash`），回填已有行的 `embedding_content_hash`。幂等且可重复执行。

### retrieval.rs — 多阶段检索管线

三阶段管线，支持提前返回优化：

1. **缓存阶段** — LRU 内存缓存（256 条目，5 分钟 TTL），键为 `query:limit:session:namespace`。缓存命中 → 立即返回。
2. **FTS/向量阶段** — BM25 关键词搜索（FTS5），然后向量余弦相似度。若 BM25 最高分 > `fts_early_return_score`（0.85） → 跳过向量阶段。
3. **融合阶段** — 根据配置的 `MergeStrategy` 合并结果。

**兜底机制**：若混合/向量搜索返回空结果，回退到 LIKE 搜索（将查询拆分为 ≤8 个关键词，搜索 content 和 key 列）。

**搜索模式配置**：

| 模式 | 说明 |
|------|------|
| `Hybrid` | BM25 关键词 + 向量余弦，按策略融合（默认） |
| `Embedding` | 仅向量余弦相似度搜索 |
| `Bm25` | 仅 BM25 关键词搜索（跳过向量阶段） |

### vector.rs — 融合策略

两种合并 BM25 和向量搜索结果的策略：

| 策略 | 说明 | 默认值 |
|------|------|--------|
| `Rrf { k }` | Reciprocal Rank Fusion——使用排名位置而非原始分数。分数 = `1/(k + rank + 1)`，两列表共现项累加。消除 BM25-向量尺度不匹配。 | `k=60`（论文推荐），**当前默认** |
| `Weighted { vector_weight, keyword_weight }` | 加权平均融合——BM25 按最大值归一化，然后 `vector_weight × v_score + keyword_weight × kw_score`。旧版行为。 | `vector=0.7, keyword=0.3` |

### embeddings.rs — 嵌入提供者

三种嵌入提供者实现：

| 提供者 | 说明 | 适用场景 |
|--------|------|----------|
| `NoopEmbedding` | 零维，空操作嵌入 | 未配置提供者时的纯关键词模式 |
| `OpenAiEmbedding` | HTTP 客户端，兼容 OpenAI API（OpenAI、OpenRouter、自定义端点） | 有 API 访问的远程嵌入 |
| `LocalOnnxEmbedding` | ONNX Runtime 本地推理，INT8 量化模型（`local-embedding` feature 门控） | 移动端/离线场景，零网络延迟 |

**嵌入缓存**：SHA-256 内容哈希（前 8 字节 → 16 hex 字符）作为缓存键。LRU 淘汰的 `embedding_cache` 表避免冗余 API 调用。缓存命中 → 跳过嵌入计算。

**延迟嵌入**（`defer_embedding=true`）：存储时立即写入元数据 + FTS（毫秒级），嵌入在后台异步计算。内容哈希守卫防止过期更新——若内容在嵌入完成前变更，更新静默丢弃。用 `AtomicUsize` 计数器追踪待处理任务，关闭时超时等待排空。

**嵌入提供者解析**（`resolve_embedding_provider`）：

优先级顺序：
1. `memory.embedding_provider = "local"` → `LocalOnnxEmbedding`（需 `local-embedding` feature）。模型文件缺失时自动从 HuggingFace 下载。
2. `memory.embedding_provider = "openai" | "openrouter" | "custom:URL"` → `OpenAiEmbedding` 远程 API。默认模型：`text-embedding-3-small`，默认维度：1536。
3. `providers.embedding_routes` 非空（旧版路径） → 从第一条路由创建 `OpenAiEmbedding`。
4. 以上均无 → `NoopEmbedding`（纯关键词模式，向后兼容）。

### consolidation.rs — 记忆整合

启发式两阶段提取，在每次 agent turn 后运行（不调用 LLM）：

1. **Daily 历史** — 从对话上下文自动创建带时间戳的 Daily 条目。键格式：`daily_{date}_{uuid}`。内容在词边界截断至 50 字符。
2. **Core 晋升** — 当重要性评分 ≥ **0.8**（`CORE_PROMOTION_THRESHOLD`）且内容长度 ≥ 10 字符时，将高重要性内容晋升为 Core 记忆。存储前调用冲突检测——冲突通过取代标记解决。

### hygiene.rs — 记忆卫生

基于节奏控制的定期清理，每 **12 小时**运行一次（`HYGIENE_INTERVAL_HOURS`）。**Core 记忆永不被修剪。**

**保留量守卫（Retention Floor）**：每个分类有可配置的 `retention_floor`（最小保留条目数）。算法：
1. 统计分类中非取代条目总数
2. 若总数 ≤ floor → 不修剪
3. 计算 `allowed_to_delete = max(0, total - floor)`
4. 按允许数量删除最旧的过期条目（使用 rowid 子查询，兼容 Android SQLite）

**取代条目**：仅非取代条目计入 floor。取代条目若足够旧仍可修剪。

**状态追踪**：`memory_hygiene_state.json` 记录上次运行时间戳和清理报告。

### snapshot.rs — 记忆快照与水合

- **快照** — 将所有 Core 记忆导出到工作区根目录的 `MEMORY_SNAPSHOT.md`，保留时间戳和元数据的结构化 Markdown 格式
- **自动水合** — 冷启动时，若 `brain.db` 不存在或 < 4KB 但 `MEMORY_SNAPSHOT.md` 存在，解析条目并以新 UUID 重新索引回 SQLite。使用 `INSERT OR IGNORE` 防止重复

### conflict.rs — 多信号冲突检测

两种冲突检测模式：

| 模式 | 说明 |
|------|------|
| `Jaccard` | 仅词集重叠（`|交集| / |并集|`）。旧版行为。 |
| `Combined { jaccard_w, cosine_w, bm25_w }` | Jaccard + 余弦相似度 + BM25 overlap 加权融合。**当前默认**（0.4/0.4/0.2）。 |

**退化规则**：若任一侧缺少嵌入 → 自动回退到纯 Jaccard（不做权重重分配）。

**矛盾信号检测**（在相似度基础上增强，×0.3 权重）：

| 信号 | 增强 | 检测方式 |
|------|------|----------|
| 否定翻转 | 0.4 | 一方含否定词（`not`、`doesn't`、`never`、`no`…），另一方不含，且 Jaccard 重叠 >0.3 |
| 偏好变更 | 0.3 | 双方均含偏好词（`prefers`、`likes`、`favorite`…）但内容不同 |
| 时间矛盾 | 0.3 | 绝对词（`always`、`forever`）对时间转变词（`now`、`recently`、`switched`），且 Jaccard 重叠 >0.2 |

**解决机制**：`total_score = combined_similarity + contradiction_boost × 0.3`。若 `total_score > threshold` 且内容不同 → 冲突。旧条目标记为 `[SUPERSEDED by 'newer_key'] {original_content}`（保留审计轨迹）。

### curator.rs — LLM 驱动的记忆策展人

使用配置的 Provider 智能分析记忆的定期清理：

1. **收集** — 收集所有 Core + Daily 记忆
2. **分析** — 将记忆列表发送给 LLM，要求识别重复、冲突和低价值条目
3. **执行** — 删除低价值条目，将重复条目合并为简洁摘要（在词边界截断至 50 字符）

设计为定时任务运行（如每晚 9 点）。返回 `CurateReport`，包含删除的键和合并的分组。

**报告结构**：

```rust
pub struct CurateReport {
    pub deleted: Vec<String>,       // 已删除条目的键
    pub merged: Vec<MergeGroup>,    // 合并为摘要的分组
    pub total_before: usize,
    pub total_after: usize,
}
```

### chunker.rs — Markdown 文本分块器

基于行的 Markdown 分块器，将文档拆分为语义块，用于整合和嵌入准备：

1. 按 `## ` 和 `# ` 标题拆分（标题与内容保持在一起）
2. 若章节超过 `max_tokens`（约 4 字符/token），按空行（段落）拆分
3. 若段落仍超限，按行边界拆分

每个块保留标题上下文（`Rc<str>` 用于子块间共享标题字符串）。

### model_cache.rs — ONNX 模型下载与缓存

Feature 门控模块（`local-embedding`），管理本地嵌入模型文件：

- **模型目录**：`{workspace_dir}/models/{model_name}/`
- **自动下载**：模型文件缺失时，从 HuggingFace 仓库下载
- **支持的模型**：`gte-multilingual-base`（INT8 量化，默认）、`gte-multilingual-base-full`（FP32）
- **ONNX Runtime**：桌面端将 `libonnxruntime.so` 下载到模型目录并通过 `ort::util::preload_dylib()` 预加载。Android 端将 .so 打包在 `jniLibs/` 中，gateway 设置 `ORT_DYLIB_PATH` 环境变量。
- **幂等性**：仅在文件缺失时下载；已存在时跳过

### importance.rs — 重要性评分

启发式（非 LLM）重要性计算：

| 分类 | 基础分数 |
|------|----------|
| Core | 0.7 |
| Custom | 0.4 |
| Daily | 0.3 |
| Conversation | 0.2 |

**关键词增强**：高信号关键词（`decision`、`always`、`never`、`important`、`critical`、`must`、`requirement`、`policy`、`rule`、`principle`）——每次匹配 +0.1，上限 +0.2。最终：`min(1.0, base + boost)`。

### decay.rs — 时间衰减

非 Core 记忆的指数衰减：

- **Core**：永不衰减（常青）
- **其他分类**：半衰期默认 7 天。公式：`score × 2^(-age_days / half_life_days)`
- 半衰期：分数 → 50%；2× 半衰期：分数 → 25%

### namespaced.rs — 命名空间隔离

支持按命名空间隔离记忆（如不同用户/会话）。默认命名空间：`"default"`。

### none.rs — NoneMemory

当 SQLite 初始化失败时的优雅降级后端，所有操作返回空结果。

## 数据结构

### MemoryEntry

```rust
pub struct MemoryEntry {
    pub id: String,
    pub key: String,
    pub content: String,
    pub category: MemoryCategory,
    pub timestamp: String,           // RFC3339
    pub session_id: Option<String>,
    pub score: Option<f64>,          // 检索分数
    pub namespace: String,           // 默认："default"
    pub importance: Option<f64>,     // 0.0–1.0
    pub superseded_by: Option<String>, // 取代条目的键
    pub embedding: Option<Vec<f32>>,   // 通过 recall_with_embeddings() 填充
}
```

### 核心枚举

```rust
pub enum MemoryCategory {
    Core,                      // 持久化、高信号事实
    Daily,                     // 会话范围历史
    Conversation,              // 临时对话上下文
    Custom(String),            // 用户自定义分类
}

pub enum SearchMode {
    Bm25,        // 仅关键词
    Embedding,   // 仅向量
    Hybrid,      // 两者（默认）
}

pub enum MergeStrategy {
    Weighted { vector_weight: f32, keyword_weight: f32 },  // 旧版：0.7/0.3
    Rrf { k: u32 },                                       // 默认：k=60
}

pub enum ConflictMode {
    Jaccard,                                                 // 旧版
    Combined { jaccard_w: f32, cosine_w: f32, bm25_w: f32 }, // 默认：0.4/0.4/0.2
}
```

## 记忆分类

| 分类 | 说明 | 衰减 | 卫生清理 |
|------|------|------|----------|
| `Core` | 持久化知识（偏好、规则、事实） | 永不衰减 | 永不修剪 |
| `Daily` | 会话范围历史条目 | 7 天半衰期 | 保留期后修剪（有 floor 守卫） |
| `Conversation` | 临时对话上下文 | 7 天半衰期 | 保留期后修剪（有 floor 守卫） |
| `Custom(String)` | 用户自定义分类 | 7 天半衰期 | 默认卫生不修剪 |

## 工厂函数

```rust
// CLI / 简单使用 — 无嵌入提供者（纯关键词搜索）
pub fn create_memory(
    config: &MemoryConfig,
    workspace_dir: &Path,
    api_key: Option<&str>,
) -> anyhow::Result<Arc<dyn Memory>>

// Gateway / 带嵌入提供者解析的完整初始化
pub async fn create_memory_with_storage_and_routes(
    config: &MemoryConfig,
    providers_config: &ProvidersConfig,
    storage_config: Option<&StorageConfig>,
    workspace_dir: &Path,
    api_key: Option<&str>,
) -> anyhow::Result<Arc<dyn Memory>>
```

- 默认创建 `SqliteMemory`，失败时降级为 `NoneMemory`
- 初始化顺序：卫生清理 → 快照导出 → 自动水合 → 嵌入提供者解析 → SqliteMemory 构建 → 可选回填
- `create_memory` 使用 `NoopEmbedding`（纯关键词）；`create_memory_with_storage_and_routes` 解析配置的提供者

### 自动保存内容过滤

`should_skip_autosave_content()` 过滤来自自动化任务和系统生成内容的噪音，防止污染记忆：

- 空内容
- 以 `[cron:` 开头的行（定时任务消息）
- 以 `[heartbeat` 开头的行（健康检查消息）
- 以 `[distilled_` 开头的行（整合产物）
- 以 `[memory context]` 开头的行（记忆注入标记）

## 配置示例

```toml
[memory]
backend = "sqlite"
auto_save = true
hygiene_enabled = true                    # 启用定期清理（默认：true）
conversation_retention_days = 30          # 修剪保留天数（默认：30）
snapshot_enabled = false                  # 导出 Core 记忆到 MEMORY_SNAPSHOT.md（默认：false）
auto_hydrate = true                       # brain.db 缺失时从快照重新索引（默认：true）
conflict_threshold = 0.6                  # 冲突检测相似度阈值（默认：0.6）
conflict_mode = "combined"                # "jaccard" 或 "combined"（默认："combined")
search_mode = "hybrid"                    # "hybrid"、"embedding" 或 "bm25"（默认："hybrid")
merge_strategy = "rrf"                    # "rrf" 或 "weighted"（默认："rrf")
defer_embedding = true                    # 异步嵌入回填（默认：有提供者时 true）
embedding_provider = "local"              # "local"、"openai"、"openrouter"、"custom:<url>" 或 None
embedding_model = "gte-multilingual-base" # 嵌入模型名称（默认：local 用 gte-multilingual-base，openai 用 text-embedding-3-small）
embedding_dims = 768                      # 向量维度（可选，local 自动检测）
embedding_cache_max = 10000               # 嵌入缓存大小（默认：10000）
backfill_on_startup = false               # 启动时批量回填 NULL 嵌入（默认：false）
auto_recall = true                        # 每轮自动召回相关记忆（默认：true）
auto_recall_limit = 5                     # 每轮最大召回条目数（默认：5）
stable_memory_in_system_prompt = true     # 将 Core 记忆注入系统提示（默认：auto_recall 时 true）
# 分类保留量守卫（默认：无守卫）
# min_retention_floor = 50                # 全分类 floor
# conversation_retention_floor = 30       # Conversation 分类 floor
# daily_retention_floor = 20              # Daily 分类 floor
```
