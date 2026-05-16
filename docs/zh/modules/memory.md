# clawseed-memory — 记忆存储与检索

## 概述

`clawseed-memory` 提供 SQLite 支持的记忆存储，支持向量搜索、BM25 关键词搜索、时间衰减和重要性评分。

## 架构

```
┌─────────────────────────────────────────────┐
│                Memory Trait                  │
│  store / recall / forget / purge / export   │
└─────────────────────┬───────────────────────┘
                      │
         ┌────────────┴────────────┐
         │                         │
  ┌──────┴──────┐          ┌──────┴──────┐
  │ SqliteMemory│          │ NoneMemory  │
  │  (默认)     │          │  (兜底)     │
  └──────┬──────┘          └─────────────┘
         │
  ┌──────┴──────────────────────────────┐
  │            检索引擎                  │
  │  ┌─────────────┐  ┌──────────────┐ │
  │  │ 向量相似度   │  │  BM25 关键词 │ │
  │  │ (embedding) │  │   搜索       │ │
  │  └──────┬──────┘  └──────┬───────┘ │
  │         └─────┬──────────┘         │
  │               ↓                     │
  │         混合排序 (hybrid)           │
  └─────────────────────────────────────┘
```

## 核心模块

### Memory trait（定义在 clawseed-api）

```rust
#[async_trait]
pub trait Memory: Send + Sync {
    fn name(&self) -> &str;
    async fn store(&self, key: &str, content: &str, category: MemoryCategory, session_id: Option<&str>) -> Result<()>;
    async fn recall(&self, query: &str, limit: usize, session_id: Option<&str>, since: Option<&str>, until: Option<&str>) -> Result<Vec<MemoryEntry>>;
    async fn get(&self, key: &str) -> Result<Option<MemoryEntry>>;
    async fn list(&self, category: Option<&MemoryCategory>, session_id: Option<&str>) -> Result<Vec<MemoryEntry>>;
    async fn forget(&self, key: &str) -> Result<bool>;
    async fn count(&self) -> Result<usize>;
    async fn health_check(&self) -> bool;
    // ... 更多带默认实现的方法：purge_namespace, purge_session, recall_namespaced, export, store_with_metadata
}
```

### sqlite.rs — SQLite 后端

**数据库 Schema**：

| 表 | 字段 | 说明 |
|----|------|------|
| `memories` | id, key, content, category, timestamp, session_id, namespace, importance, superseded_by | 记忆存储 |
| `embeddings` | id, memory_id, vector, model | 向量嵌入 |

使用 FTS5 虚拟表实现 BM25 关键词搜索，BLOB 存储向量实现余弦相似度。

### retrieval.rs — 混合检索

支持三种搜索模式：

| 模式 | 说明 |
|------|------|
| `Hybrid` | 向量相似度 + BM25 关键词，加权合并（默认） |
| `Embedding` | 仅向量相似度搜索 |
| `Bm25` | 仅 BM25 关键词搜索 |

### embeddings.rs — 向量嵌入

处理文本的向量编码，用于语义搜索。

### chunker.rs — 文本分块

将大文本分割为适合存储和检索的块。

### decay.rs — 时间衰减

基于时间的记忆衰减评分，旧记忆权重降低。

### importance.rs — 重要性评分

根据相关性信号对记忆评分，优先保留重要记忆。

### consolidation.rs — 记忆整合

启发式两阶段提取，在每次 agent turn 后运行（不调用 LLM）：

1. **Daily 历史** — 从对话上下文自动创建带时间戳的 Daily 条目
2. **Core 晋升** — 当重要性评分 ≥ 0.6 且内容长度 ≥ 10 字符时，将高重要性内容晋升为 Core 记忆

### hygiene.rs — 记忆卫生

基于节奏控制的定期清理，每 12 小时运行一次。修剪超过配置保留期限的 Conversation 和 Daily 条目。**Core 记忆永不被修剪。** 在 `memory_hygiene_state.json` 中记录上次运行时间戳，避免重复扫描。

### snapshot.rs — 记忆快照与水合

- **快照** — 将所有 Core 记忆导出到工作区根目录的 `MEMORY_SNAPSHOT.md`，保留时间戳和元数据
- **自动水合** — 冷启动时，若 `brain.db` 不存在但 `MEMORY_SNAPSHOT.md` 存在，则将条目重新索引回 SQLite

### conflict.rs — 冲突检测

基于词重叠的 Jaccard 相似度检测矛盾的 Core 记忆。当两个条目超过相似度阈值时，较旧的条目标记为 `[SUPERSEDED by 'newer_key']` 而非删除。仅检查 Core 类别条目。

### vector.rs — 向量存储

向量索引和相似度计算。

### namespaced.rs — 命名空间隔离

支持按命名空间隔离记忆（如不同用户/会话）。

### none.rs — NoneMemory

当 SQLite 初始化失败时的优雅降级后端，所有操作返回空结果。

## 记忆分类

通过 `category` 字段过滤记忆：

| 分类 | 说明 |
|------|------|
| `Core` | 核心持久化知识 |
| `Daily` | 日常/临时信息 |
| `Conversation` | 对话上下文 |
| `Custom(String)` | 用户自定义分类 |

## 工厂函数

```rust
pub fn create_memory(config: &MemoryConfig) -> Arc<dyn Memory>
```

- 默认创建 `SqliteMemory`
- 失败时降级为 `NoneMemory`
- 配置项：后端类型（sqlite/none）、搜索模式、嵌入路由

## 配置示例

```toml
[memory]
backend = "sqlite"
auto_save = true
hygiene_enabled = true                    # 启用定期清理（默认：true）
conversation_retention_days = 30          # Conversation/Daily 修剪保留天数（默认：30）
snapshot_enabled = false                  # 导出 Core 记忆到 MEMORY_SNAPSHOT.md（默认：false）
auto_hydrate = true                       # brain.db 缺失时从快照重新索引（默认：true）
conflict_threshold = 0.6                  # 冲突检测 Jaccard 阈值（默认：0.6）
```
