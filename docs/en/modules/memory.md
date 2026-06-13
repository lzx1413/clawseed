# clawseed-memory — Memory Storage and Retrieval

## Overview

`clawseed-memory` provides SQLite-backed memory storage with hybrid search (BM25 keyword + vector embeddings), Reciprocal Rank Fusion (RRF) ranking, multi-signal conflict detection, deferred embedding, and lifecycle management (consolidation, hygiene, snapshot).

## Architecture

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
   │  (default)  │                   │  (fallback) │
   └──────┬──────┘                   └─────────────┘
          │
   ┌──────┴───────────────────────────────────────────┐
   │               Retrieval Pipeline                  │
   │  ┌─────────┐   ┌──────────────┐  ┌───────────┐ │
   │  │  Hot     │   │   BM25       │  │  Vector   │ │
   │  │  Cache   │──▶│  Keyword     │  │  Cosine   │ │
   │  │  (LRU)   │   │  (FTS5)      │  │  Similarity│ │
   │  └─────────┘   └──────┬───────┘  └─────┬─────┘ │
   │                       └──────┬──────────┘       │
   │                              ↓                   │
   │                     Merge Strategy               │
   │                ┌─────────┬──────────┐           │
   │                │  RRF    │ Weighted  │           │
   │                │(default)│ (legacy)  │           │
   │                └─────────┴──────────┘           │
   └──────────────────────────────────────────────────┘
          │
   ┌──────┴───────────────────────────────────────────┐
   │             Lifecycle Management                  │
   │  ┌──────────────┐ ┌─────────┐ ┌───────────────┐ │
   │  │ Consolidation │ │ Hygiene │ │  Conflict     │ │
   │  │ (Daily→Core) │ │(pruning)│ │  Detection    │ │
   │  └──────────────┘ └─────────┘ └───────────────┘ │
   │  ┌──────────────┐ ┌─────────────────────────┐   │
   │  │  Snapshot    │ │  Deferred Embedding     │   │
   │  │ (export/hydr)│ │  (async backfill)       │   │
   │  └──────────────┘ └─────────────────────────┘   │
   └──────────────────────────────────────────────────┘
```

## Core Modules

### Memory Trait (defined in clawseed-api)

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
    // Default implementations:
    async fn top_core_memories(&self, limit: usize) -> Result<Vec<MemoryEntry>>;  // SqliteMemory overrides
    async fn reindex(&self) -> Result<usize>;
    async fn backfill_embeddings(&self, batch_size: usize) -> Result<usize>;
}
```

### sqlite.rs — SQLite Backend

**Database Schema**:

| Table | Fields | Description |
|-------|--------|-------------|
| `memories` | id, key, content, category, embedding (BLOB), created_at, updated_at, session_id, namespace, importance, superseded_by, embedding_content_hash | Memory storage with inline embeddings |
| `memories_fts` | key, content (FTS5 virtual table on `memories`) | Full-text search index |
| `embedding_cache` | content_hash, embedding (BLOB), created_at, accessed_at | LRU-evicted embedding cache |

**PRAGMA Tuning**: WAL mode (concurrent reads), `synchronous=NORMAL` (2× write speed), `mmap_size=8MB`, `cache_size=-2000`, `temp_store=MEMORY`.

**Migration System**: Auto-upgrades on every open — adds missing columns (`session_id`, `namespace`, `importance`, `superseded_by`, `embedding_content_hash`), backfills `embedding_content_hash` for existing rows. Idempotent and safe to re-run.

### retrieval.rs — Multi-Stage Retrieval Pipeline

Three-stage pipeline with early return optimization:

1. **Cache Stage** — LRU in-memory cache (256 entries, 5-min TTL) keyed by `query:limit:session:namespace`. Cache hit → immediate return.
2. **FTS/Vector Stages** — BM25 keyword search via FTS5, then vector cosine similarity. If top BM25 score > `fts_early_return_score` (0.85) → skip vector stage.
3. **Merge Stage** — Combine results via configured `MergeStrategy`.

**Fallback**: If hybrid/vector returns empty, falls back to LIKE search (splits query into ≤8 keywords, searches content and key columns).

**Search Mode Configuration**:

| Mode | Description |
|------|-------------|
| `Hybrid` | BM25 keyword + vector cosine, merged by strategy (default) |
| `Embedding` | Vector cosine similarity search only |
| `Bm25` | BM25 keyword search only (skip vector stage) |

### vector.rs — Merge Strategies

Two strategies for combining BM25 and vector search results:

| Strategy | Description | Default |
|----------|-------------|---------|
| `Rrf { k }` | Reciprocal Rank Fusion — uses rank positions, not raw scores. Score = `1/(k + rank + 1)`, summed for items in both lists. Eliminates BM25-vector scale mismatch. | `k=60` (paper recommendation), **current default** |
| `Weighted { vector_weight, keyword_weight }` | Weighted average merge — normalizes BM25 by max score, then `vector_weight × v_score + keyword_weight × kw_score`. Legacy behavior. | `vector=0.7, keyword=0.3` |

### embeddings.rs — Embedding Providers

Three embedding provider implementations:

| Provider | Description | Use Case |
|----------|-------------|----------|
| `NoopEmbedding` | Zero-dimensional, no-op embedding | Keyword-only mode when no provider configured |
| `OpenAiEmbedding` | HTTP client with OpenAI-compatible API (OpenAI, OpenRouter, custom endpoints) | Remote embedding with API access |
| `LocalOnnxEmbedding` | ONNX Runtime on-device inference with INT8 quantized models (`local-embedding` feature gate) | Mobile/offline scenarios, zero network latency |

**Embedding Cache**: SHA-256 content hash (first 8 bytes → 16 hex chars) as cache key. LRU-evicted `embedding_cache` table avoids redundant API calls. Cache hit → skip embedding computation.

**Deferred Embedding** (`defer_embedding=true`): Store writes metadata + FTS immediately (millisecond-level), embedding computed asynchronously in background. Content hash guard prevents stale updates — if content changes before embedding completes, the update is silently discarded. Pending task tracking with `AtomicUsize` counter, drain with timeout on shutdown.

### consolidation.rs — Memory Consolidation

Heuristic two-phase extraction that runs after each agent turn (no LLM call):

1. **Daily history** — Creates timestamped Daily entries from conversation context. Key format: `daily_{date}_{uuid}`. Content truncated to 50 chars at word boundaries.
2. **Core promotion** — Promotes high-importance content to Core memory when importance ≥ **0.8** (`CORE_PROMOTION_THRESHOLD`) and content length ≥ 10 characters. Calls conflict detection before storing — conflicts are resolved via supersession marking.

### hygiene.rs — Memory Hygiene

Cadence-gated pruning that runs on a **12-hour** cycle (`HYGIENE_INTERVAL_HOURS`). **Core memories are never pruned.**

**Retention Floors**: Each category has a configurable `retention_floor` (minimum entries to preserve). Algorithm:
1. Count total non-superseded entries in category
2. If total ≤ floor → no pruning
3. Calculate `allowed_to_delete = max(0, total - floor)`
4. Delete oldest eligible entries up to the allowed count (uses rowid subquery for Android SQLite compatibility)

**Superseded entries**: Only non-superseded entries count toward the floor. Superseded entries are still pruned if old enough.

**State Tracking**: `memory_hygiene_state.json` records last-run timestamp and pruning report.

### snapshot.rs — Memory Snapshot & Hydration

- **Snapshot** — Exports all Core memories to `MEMORY_SNAPSHOT.md` at workspace root, preserving timestamps and metadata in structured Markdown format
- **Auto-hydration** — On cold boot, if `brain.db` is missing or < 4KB but `MEMORY_SNAPSHOT.md` exists, parses and re-indexes entries back into SQLite with new UUIDs. Uses `INSERT OR IGNORE` to prevent duplicates

### conflict.rs — Multi-Signal Conflict Detection

Two conflict detection modes:

| Mode | Description |
|------|-------------|
| `Jaccard` | Word-set overlap only (`|intersection| / |union|`). Legacy behavior. |
| `Combined { jaccard_w, cosine_w, bm25_w }` | Weighted fusion of Jaccard + cosine similarity + BM25 overlap. **Current default** (0.4/0.4/0.2). |

**Degradation rule**: If either side lacks embedding → automatically falls back to pure Jaccard (no weight renormalization).

**Contradiction Signal Detection** (boosted on top of similarity score, ×0.3 weight):

| Signal | Boost | Detection |
|--------|-------|-----------|
| Negation reversal | 0.4 | One text has negation words (`not`, `doesn't`, `never`, `no`…), other doesn't, with >0.3 Jaccard overlap |
| Preference change | 0.3 | Both texts have preference keywords (`prefers`, `likes`, `favorite`…) but differ |
| Temporal contradiction | 0.3 | Absolute terms (`always`, `forever`) vs temporal shift terms (`now`, `recently`, `switched`), with >0.2 Jaccard overlap |

**Resolution**: `total_score = combined_similarity + contradiction_boost × 0.3`. If `total_score > threshold` AND content differs → conflict. Older entry marked `[SUPERSEDED by 'newer_key'] {original_content}` (audit trail preserved).

### importance.rs — Importance Scoring

Heuristic (non-LLM) importance computation:

| Category | Base Score |
|----------|-----------|
| Core | 0.7 |
| Custom | 0.4 |
| Daily | 0.3 |
| Conversation | 0.2 |

**Keyword Boost**: High-signal keywords (`decision`, `always`, `never`, `important`, `critical`, `must`, `requirement`, `policy`, `rule`, `principle`) — each match adds +0.1, capped at +0.2. Final: `min(1.0, base + boost)`.

### decay.rs — Time-Based Decay

Exponential decay for non-Core memories:

- **Core**: Never decayed (evergreen)
- **Other categories**: Half-life default 7 days. Formula: `score × 2^(-age_days / half_life_days)`
- At half-life: score → 50%; at 2× half-life: score → 25%

### namespaced.rs — Namespace Isolation

Supports namespace-based memory isolation (e.g., per user/session). Default namespace: `"default"`.

### none.rs — NoneMemory

Graceful degradation backend when SQLite initialization fails. All operations return empty results.

## Data Structures

### MemoryEntry

```rust
pub struct MemoryEntry {
    pub id: String,
    pub key: String,
    pub content: String,
    pub category: MemoryCategory,
    pub timestamp: String,           // RFC3339
    pub session_id: Option<String>,
    pub score: Option<f64>,          // Retrieval score
    pub namespace: String,           // Default: "default"
    pub importance: Option<f64>,     // 0.0–1.0
    pub superseded_by: Option<String>, // Key of superseding entry
    pub embedding: Option<Vec<f32>>,   // Populated via recall_with_embeddings()
}
```

### Key Enums

```rust
pub enum MemoryCategory {
    Core,                      // Persistent, high-signal facts
    Daily,                     // Session-scoped history
    Conversation,              // Ephemeral chat turns
    Custom(String),            // User-defined categories
}

pub enum SearchMode {
    Bm25,        // Keyword only
    Embedding,   // Vector only
    Hybrid,      // Both (default)
}

pub enum MergeStrategy {
    Weighted { vector_weight: f32, keyword_weight: f32 },  // Legacy: 0.7/0.3
    Rrf { k: u32 },                                       // Default: k=60
}

pub enum ConflictMode {
    Jaccard,                                                 // Legacy
    Combined { jaccard_w: f32, cosine_w: f32, bm25_w: f32 }, // Default: 0.4/0.4/0.2
}
```

## Memory Categories

| Category | Description | Decay | Hygiene |
|----------|-------------|-------|---------|
| `Core` | Persistent knowledge (preferences, rules, facts) | Never | Never pruned |
| `Daily` | Session-scoped history entries | 7-day half-life | Pruned after retention period (with floor) |
| `Conversation` | Ephemeral chat context | 7-day half-life | Pruned after retention period (with floor) |
| `Custom(String)` | User-defined categories | 7-day half-life | Not pruned by default hygiene |

## Factory Functions

```rust
// CLI / simple use
pub fn create_memory(config: &MemoryConfig) -> Arc<dyn Memory>

// Gateway / full initialization with embedding providers
pub fn create_memory_with_storage_and_routes(
    config: &MemoryConfig,
    workspace_dir: &Path,
    embedding_provider: Arc<dyn EmbeddingProvider>,
) -> Arc<dyn Memory>
```

- Defaults to `SqliteMemory`, falls back to `NoneMemory` on failure
- Initialization order: hygiene pass → snapshot export → auto-hydration → embedding provider resolution → SqliteMemory construction → optional backfill

## Configuration Example

```toml
[memory]
backend = "sqlite"
auto_save = true
hygiene_enabled = true                    # Enable periodic cleanup (default: true)
conversation_retention_days = 30          # Days before pruning (default: 30)
snapshot_enabled = false                  # Export Core memories to MEMORY_SNAPSHOT.md (default: false)
auto_hydrate = true                       # Re-index from snapshot if brain.db missing (default: true)
conflict_threshold = 0.6                  # Similarity threshold for conflict detection (default: 0.6)
conflict_mode = "combined"                # "jaccard" or "combined" (default: "combined")
search_mode = "hybrid"                    # "hybrid", "embedding", or "bm25" (default: "hybrid")
merge_strategy = "rrf"                    # "rrf" or "weighted" (default: "rrf")
defer_embedding = true                    # Async embedding backfill (default: true if provider exists)
embedding_provider = "local"              # "local", "openai", "openrouter", "custom:<url>", or None
embedding_model = "gte-multilingual-base" # Model name for embedding
embedding_dims = 768                      # Vector dimensions (optional, auto-detected for local)
backfill_on_startup = false               # Batch backfill NULL embeddings on startup (default: false)
# Per-category retention floors (default: no floor)
# min_retention_floor = 50                # Global floor for all categories
# conversation_retention_floor = 30       # Floor for Conversation category
# daily_retention_floor = 20              # Floor for Daily category
```
