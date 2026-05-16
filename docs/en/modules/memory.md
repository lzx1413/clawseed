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

### Memory Trait (defined in clawseed-api)

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
    // ... more methods with defaults: purge_namespace, purge_session, recall_namespaced, export, store_with_metadata
}
```

### sqlite.rs — SQLite Backend

**Database Schema**:

| Table | Fields | Description |
|-------|--------|-------------|
| `memories` | id, key, content, category, timestamp, session_id, namespace, importance, superseded_by | Memory storage |
| `embeddings` | id, memory_id, vector, model | Vector embeddings |

Uses FTS5 virtual table for BM25 keyword search, and BLOB-stored vectors for cosine similarity.

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

Heuristic two-phase extraction that runs after each agent turn (no LLM call):

1. **Daily history** — Creates timestamped Daily entries from conversation context automatically
2. **Core promotion** — Promotes high-importance content to Core memory when importance ≥ 0.6 and content length ≥ 10 characters

### hygiene.rs — Memory Hygiene

Cadence-gated pruning that runs on a 12-hour cycle. Prunes Conversation and Daily rows older than the configured retention period. **Core memories are never pruned.** Tracks its last-run timestamp in `memory_hygiene_state.json` to avoid redundant scans.

### snapshot.rs — Memory Snapshot & Hydration

- **Snapshot** — Exports all Core memories to `MEMORY_SNAPSHOT.md` in the workspace root, preserving timestamps and metadata
- **Auto-hydration** — On cold boot, if `brain.db` is missing but `MEMORY_SNAPSHOT.md` exists, re-indexes entries back into SQLite

### conflict.rs — Conflict Detection

Uses Jaccard similarity on word overlap to detect contradictory Core memories. When two entries exceed the similarity threshold, the older one is marked `[SUPERSEDED by 'newer_key']` rather than deleted. Only checks Core category entries.

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
| `Core` | Core persistent knowledge |
| `Daily` | Daily/ephemeral information |
| `Conversation` | Conversation context |
| `Custom(String)` | User-defined categories |

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
auto_save = true
hygiene_enabled = true                    # Enable periodic cleanup (default: true)
conversation_retention_days = 30          # Days before Conversation/Daily pruning (default: 30)
snapshot_enabled = false                  # Export Core memories to MEMORY_SNAPSHOT.md (default: false)
auto_hydrate = true                       # Re-index from snapshot if brain.db missing (default: true)
conflict_threshold = 0.6                  # Jaccard threshold for conflict detection (default: 0.6)
```
