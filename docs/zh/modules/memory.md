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

### traits.rs — Memory trait

```rust
#[async_trait]
pub trait Memory: Send + Sync {
    async fn store(&self, content: &str, category: &str) -> Result<String>;
    async fn recall(&self, query: &str, limit: usize) -> Result<Vec<MemoryEntry>>;
    async fn forget(&self, id: &str) -> Result<()>;
    async fn purge(&self, before: DateTime<Utc>) -> Result<usize>;
    async fn export(&self) -> Result<Vec<MemoryEntry>>;
}
```

### sqlite.rs — SQLite 后端

**数据库 Schema**：

| 表 | 字段 | 说明 |
|----|------|------|
| `messages` | id, content, category, metadata, created_at | 消息存储 |
| `embeddings` | id, message_id, vector, model | 向量嵌入 |

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

合并相关记忆，减少冗余。

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
| `context` | 对话上下文 |
| `user_profile` | 用户偏好 |
| `tool_output` | 工具输出 |

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
search_mode = "hybrid"    # hybrid / embedding / bm25

[memory.embedding]
endpoint = "http://localhost:11434/api/embeddings"
model = "nomic-embed-text"
```
