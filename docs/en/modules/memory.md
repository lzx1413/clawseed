# clawseed-memory — Memory Storage and Retrieval

## Overview

`clawseed-memory` provides SQLite-backed memory storage with vector search, BM25 keyword search, time-based decay, and importance scoring.

## Architecture

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
  │  (default)  │          │  (fallback) │
  └──────┬──────┘          └─────────────┘
         │
  ┌──────┴──────────────────────────────┐
  │            Retrieval Engine          │
  │  ┌─────────────┐  ┌──────────────┐ │
  │  │   Vector     │  │  BM25        │ │
  │  │  Similarity  │  │  Keyword     │ │
  │  │  (embedding) │  │  Search      │ │
  │  └──────┬──────┘  └──────┬───────┘ │
  │         └─────┬──────────┘         │
  │               ↓                     │
  │         Hybrid Ranking              │
  └─────────────────────────────────────┘
```

## Core Modules

### traits.rs — Memory Trait

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

### sqlite.rs — SQLite Backend

**Database Schema**:

| Table | Fields | Description |
|-------|--------|-------------|
| `messages` | id, content, category, metadata, created_at | Message storage |
| `embeddings` | id, message_id, vector, model | Vector embeddings |

### retrieval.rs — Hybrid Retrieval

Supports three search modes:

| Mode | Description |
|------|-------------|
| `Hybrid` | Vector similarity + BM25 keyword, weighted merge (default) |
| `Embedding` | Vector similarity search only |
| `Bm25` | BM25 keyword search only |

### embeddings.rs — Vector Embeddings

Handles text vector encoding for semantic search.

### chunker.rs — Text Chunking

Breaks large text into manageable pieces for storage and retrieval.

### decay.rs — Time-Based Decay

Time-based memory decay scoring — older memories receive lower weights.

### importance.rs — Importance Scoring

Scores memories based on relevance signals, prioritizing important ones.

### consolidation.rs — Memory Consolidation

Merges related memories to reduce redundancy.

### vector.rs — Vector Storage

Vector indexing and similarity computation.

### namespaced.rs — Namespace Isolation

Supports namespace-based memory isolation (e.g., per user/session).

### none.rs — NoneMemory

Graceful degradation backend when SQLite initialization fails. All operations return empty results.

## Memory Categories

Filter memories via the `category` field:

| Category | Description |
|----------|-------------|
| `context` | Conversation context |
| `user_profile` | User preferences |
| `tool_output` | Tool output |

## Factory Function

```rust
pub fn create_memory(config: &MemoryConfig) -> Arc<dyn Memory>
```

- Defaults to `SqliteMemory`
- Falls back to `NoneMemory` on failure
- Configuration: backend type (sqlite/none), search mode, embedding routes

## Configuration Example

```toml
[memory]
backend = "sqlite"
search_mode = "hybrid"    # hybrid / embedding / bm25

[memory.embedding]
endpoint = "http://localhost:11434/api/embeddings"
model = "nomic-embed-text"
```
