use super::embeddings::EmbeddingProvider;
use super::traits::{ExportFilter, Memory, MemoryCategory, MemoryEntry};
use super::vector;
use anyhow::Context;
use async_trait::async_trait;
use chrono::Local;
use clawseed_api::memory_traits::{MemoryQuery, MemoryScope, MergeStrategy, SearchMode};
use parking_lot::Mutex;
use rusqlite::{Connection, params};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;
use uuid::Uuid;

/// Maximum allowed open timeout (seconds) to avoid unreasonable waits.
const SQLITE_OPEN_TIMEOUT_CAP_SECS: u64 = 300;

/// SQLite-backed persistent memory — the brain
///
/// Full-stack search engine:
/// - **Vector DB**: embeddings stored as BLOB, cosine similarity search
/// - **Keyword Search**: FTS5 virtual table with BM25 scoring
/// - **Hybrid Merge**: weighted fusion of vector + keyword results
/// - **Embedding Cache**: LRU-evicted cache to avoid redundant API calls
/// - **Safe Reindex**: temp DB → seed → sync → atomic swap → rollback
pub struct SqliteMemory {
    conn: Arc<Mutex<Connection>>,
    _db_path: PathBuf,
    embedder: Arc<dyn EmbeddingProvider>,
    _vector_weight: f32,
    _keyword_weight: f32,
    cache_max: usize,
    search_mode: SearchMode,
    merge_strategy: MergeStrategy,
    defer_embedding: bool,
    pending_embeds: Arc<AtomicUsize>,
}

impl SqliteMemory {
    /// Return the embedding dimensions (0 = no embedding, vector search disabled).
    pub fn dimensions(&self) -> usize {
        self.embedder.dimensions()
    }

    pub fn new(workspace_dir: &Path) -> anyhow::Result<Self> {
        Self::with_embedder(
            workspace_dir,
            Arc::new(super::embeddings::NoopEmbedding),
            0.7,
            0.3,
            10_000,
            None,
            SearchMode::default(),
            MergeStrategy::default(),
            false, // NoopEmbedding → no deferral
        )
    }

    /// Like `new`, but stores data in `{db_name}.db` instead of `brain.db`.
    pub fn new_named(workspace_dir: &Path, db_name: &str) -> anyhow::Result<Self> {
        let db_path = workspace_dir.join("memory").join(format!("{db_name}.db"));
        Self::with_embedder_path(
            &db_path,
            Arc::new(super::embeddings::NoopEmbedding),
            0.7,
            0.3,
            10_000,
            None,
            SearchMode::default(),
            MergeStrategy::default(),
            false, // NoopEmbedding → no deferral
        )
    }

    /// Build SQLite memory with optional open timeout.
    ///
    /// If `open_timeout_secs` is `Some(n)`, opening the database is limited to `n` seconds
    /// (capped at 300). Useful when the DB file may be locked or on slow storage.
    /// `None` = wait indefinitely (default).
    #[allow(clippy::too_many_arguments)]
    pub fn with_embedder(
        workspace_dir: &Path,
        embedder: Arc<dyn EmbeddingProvider>,
        vector_weight: f32,
        keyword_weight: f32,
        cache_max: usize,
        open_timeout_secs: Option<u64>,
        search_mode: SearchMode,
        merge_strategy: MergeStrategy,
        defer_embedding: bool,
    ) -> anyhow::Result<Self> {
        let db_path = workspace_dir.join("memory").join("brain.db");
        Self::with_embedder_path(
            &db_path,
            embedder,
            vector_weight,
            keyword_weight,
            cache_max,
            open_timeout_secs,
            search_mode,
            merge_strategy,
            defer_embedding,
        )
    }

    /// Build SQLite memory with an explicit database path and embedder.
    #[allow(clippy::too_many_arguments)]
    fn with_embedder_path(
        db_path: &Path,
        embedder: Arc<dyn EmbeddingProvider>,
        vector_weight: f32,
        keyword_weight: f32,
        cache_max: usize,
        open_timeout_secs: Option<u64>,
        search_mode: SearchMode,
        merge_strategy: MergeStrategy,
        defer_embedding: bool,
    ) -> anyhow::Result<Self> {
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let conn = Self::open_connection(db_path, open_timeout_secs)?;

        // ── Production-grade PRAGMA tuning ──────────────────────
        // WAL mode: concurrent reads during writes, crash-safe
        // normal sync: 2× write speed, still durable on WAL
        // mmap 8 MB: let the OS page-cache serve hot reads
        // cache 2 MB: keep ~500 hot pages in-process
        // temp_store memory: temp tables never hit disk
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous  = NORMAL;
             PRAGMA mmap_size    = 8388608;
             PRAGMA cache_size   = -2000;
             PRAGMA temp_store   = MEMORY;",
        )?;

        Self::init_schema(&conn)?;

        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
            _db_path: db_path.to_path_buf(),
            embedder,
            _vector_weight: vector_weight,
            _keyword_weight: keyword_weight,
            cache_max,
            search_mode,
            merge_strategy,
            defer_embedding,
            pending_embeds: Arc::new(AtomicUsize::new(0)),
        })
    }

    /// Open SQLite connection, optionally with a timeout (for locked/slow storage).
    fn open_connection(
        db_path: &Path,
        open_timeout_secs: Option<u64>,
    ) -> anyhow::Result<Connection> {
        let path_buf = db_path.to_path_buf();

        let conn = if let Some(secs) = open_timeout_secs {
            let capped = secs.min(SQLITE_OPEN_TIMEOUT_CAP_SECS);
            let (tx, rx) = mpsc::channel();
            thread::spawn(move || {
                let result = Connection::open(&path_buf);
                let _ = tx.send(result);
            });
            match rx.recv_timeout(Duration::from_secs(capped)) {
                Ok(Ok(c)) => c,
                Ok(Err(e)) => return Err(e).context("SQLite failed to open database"),
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    anyhow::bail!("SQLite connection open timed out after {} seconds", capped);
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    anyhow::bail!("SQLite open thread exited unexpectedly");
                }
            }
        } else {
            Connection::open(&path_buf).context("SQLite failed to open database")?
        };

        Ok(conn)
    }

    /// Initialize all tables: memories, FTS5, `embedding_cache`
    fn init_schema(conn: &Connection) -> anyhow::Result<()> {
        const SCHEMA_VERSION: i64 = 2;
        const MEMORY_TABLE_V2: &str = "CREATE TABLE memories (
                id                     TEXT PRIMARY KEY,
                key                    TEXT NOT NULL,
                content                TEXT NOT NULL,
                category               TEXT NOT NULL DEFAULT 'core',
                embedding              BLOB,
                created_at             TEXT NOT NULL,
                updated_at             TEXT NOT NULL,
                session_id             TEXT,
                namespace              TEXT NOT NULL DEFAULT 'default',
                importance             REAL DEFAULT 0.5,
                superseded_by          TEXT,
                embedding_content_hash TEXT DEFAULT NULL,
                UNIQUE(namespace, key)
            );";
        const MEMORY_AUXILIARY_SCHEMA: &str =
            "CREATE INDEX IF NOT EXISTS idx_memories_category ON memories(category);
             CREATE INDEX IF NOT EXISTS idx_memories_key ON memories(key);
             CREATE INDEX IF NOT EXISTS idx_memories_session ON memories(session_id);
             CREATE INDEX IF NOT EXISTS idx_memories_namespace ON memories(namespace);
             CREATE INDEX IF NOT EXISTS idx_memories_scope_category
                 ON memories(namespace, category, session_id);
             CREATE VIRTUAL TABLE IF NOT EXISTS memories_fts USING fts5(
                 key, content, content=memories, content_rowid=rowid
             );
             CREATE TRIGGER IF NOT EXISTS memories_ai AFTER INSERT ON memories BEGIN
                 INSERT INTO memories_fts(rowid, key, content)
                 VALUES (new.rowid, new.key, new.content);
             END;
             CREATE TRIGGER IF NOT EXISTS memories_ad AFTER DELETE ON memories BEGIN
                 INSERT INTO memories_fts(memories_fts, rowid, key, content)
                 VALUES ('delete', old.rowid, old.key, old.content);
             END;
             CREATE TRIGGER IF NOT EXISTS memories_au AFTER UPDATE ON memories BEGIN
                 INSERT INTO memories_fts(memories_fts, rowid, key, content)
                 VALUES ('delete', old.rowid, old.key, old.content);
                 INSERT INTO memories_fts(rowid, key, content)
                 VALUES (new.rowid, new.key, new.content);
             END;";

        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS embedding_cache (
                content_hash TEXT PRIMARY KEY,
                embedding    BLOB NOT NULL,
                created_at   TEXT NOT NULL,
                accessed_at  TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_cache_accessed ON embedding_cache(accessed_at);",
        )?;

        let memory_table_exists: i64 = conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='memories'",
            [],
            |row| row.get(0),
        )?;
        if memory_table_exists == 0 {
            conn.execute_batch(MEMORY_TABLE_V2)?;
            conn.execute_batch(MEMORY_AUXILIARY_SCHEMA)?;
            conn.pragma_update(None, "user_version", SCHEMA_VERSION)?;
            return Ok(());
        }

        let schema_sql: String = conn
            .prepare("SELECT sql FROM sqlite_master WHERE type='table' AND name='memories'")?
            .query_row([], |row| row.get::<_, String>(0))?;

        if !schema_sql.contains("session_id") {
            conn.execute_batch(
                "ALTER TABLE memories ADD COLUMN session_id TEXT;
                 CREATE INDEX IF NOT EXISTS idx_memories_session ON memories(session_id);",
            )?;
        }

        if !schema_sql.contains("namespace") {
            conn.execute_batch(
                "ALTER TABLE memories ADD COLUMN namespace TEXT DEFAULT 'default';",
            )?;
        }

        if !schema_sql.contains("importance") {
            conn.execute_batch("ALTER TABLE memories ADD COLUMN importance REAL DEFAULT 0.5;")?;
        }

        if !schema_sql.contains("superseded_by") {
            conn.execute_batch("ALTER TABLE memories ADD COLUMN superseded_by TEXT;")?;
        }

        if !schema_sql.contains("embedding_content_hash") {
            conn.execute_batch(
                "ALTER TABLE memories ADD COLUMN embedding_content_hash TEXT DEFAULT NULL;",
            )?;
        }

        let current_version: i64 =
            conn.pragma_query_value(None, "user_version", |row| row.get(0))?;
        let has_composite_key = schema_sql
            .split_whitespace()
            .collect::<String>()
            .contains("UNIQUE(namespace,key)");
        if current_version < SCHEMA_VERSION || !has_composite_key {
            conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")?;
            conn.execute_batch("BEGIN IMMEDIATE;")?;
            let migration = (|| -> anyhow::Result<()> {
                let before_count: i64 =
                    conn.query_row("SELECT COUNT(*) FROM memories", [], |row| row.get(0))?;
                let before_ids: i64 =
                    conn.query_row("SELECT COUNT(DISTINCT id) FROM memories", [], |row| {
                        row.get(0)
                    })?;

                conn.execute_batch(
                    "DROP TRIGGER IF EXISTS memories_ai;
                     DROP TRIGGER IF EXISTS memories_ad;
                     DROP TRIGGER IF EXISTS memories_au;
                     DROP TABLE IF EXISTS memories_fts;
                     CREATE TABLE memories_v2 (
                         id                     TEXT PRIMARY KEY,
                         key                    TEXT NOT NULL,
                         content                TEXT NOT NULL,
                         category               TEXT NOT NULL DEFAULT 'core',
                         embedding              BLOB,
                         created_at             TEXT NOT NULL,
                         updated_at             TEXT NOT NULL,
                         session_id             TEXT,
                         namespace              TEXT NOT NULL DEFAULT 'default',
                         importance             REAL DEFAULT 0.5,
                         superseded_by          TEXT,
                         embedding_content_hash TEXT DEFAULT NULL,
                         UNIQUE(namespace, key)
                     );
                     INSERT INTO memories_v2 (
                         id, key, content, category, embedding, created_at, updated_at,
                         session_id, namespace, importance, superseded_by, embedding_content_hash
                     )
                     SELECT id, key, content, category, embedding, created_at, updated_at,
                            session_id, COALESCE(namespace, 'default'), importance,
                            superseded_by, embedding_content_hash
                     FROM memories;",
                )?;

                let after_count: i64 =
                    conn.query_row("SELECT COUNT(*) FROM memories_v2", [], |row| row.get(0))?;
                let after_ids: i64 =
                    conn.query_row("SELECT COUNT(DISTINCT id) FROM memories_v2", [], |row| {
                        row.get(0)
                    })?;
                let null_namespaces: i64 = conn.query_row(
                    "SELECT COUNT(*) FROM memories_v2 WHERE namespace IS NULL",
                    [],
                    |row| row.get(0),
                )?;
                if before_count != after_count || before_ids != after_ids || null_namespaces != 0 {
                    anyhow::bail!(
                        "memory schema migration validation failed: rows {before_count}/{after_count}, ids {before_ids}/{after_ids}, null namespaces {null_namespaces}"
                    );
                }

                conn.execute_batch(
                    "DROP TABLE memories;
                     ALTER TABLE memories_v2 RENAME TO memories;",
                )?;
                conn.execute_batch(MEMORY_AUXILIARY_SCHEMA)?;
                conn.execute_batch("INSERT INTO memories_fts(memories_fts) VALUES('rebuild');")?;

                let foreign_key_errors: i64 =
                    conn.query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |row| {
                        row.get(0)
                    })?;
                let integrity: String =
                    conn.query_row("PRAGMA integrity_check", [], |row| row.get(0))?;
                if foreign_key_errors != 0 || integrity != "ok" {
                    anyhow::bail!(
                        "memory schema migration integrity failure: foreign keys={foreign_key_errors}, integrity={integrity}"
                    );
                }
                conn.pragma_update(None, "user_version", SCHEMA_VERSION)?;
                Ok(())
            })();

            match migration {
                Ok(()) => conn.execute_batch("COMMIT;")?,
                Err(error) => {
                    let _ = conn.execute_batch("ROLLBACK;");
                    return Err(error);
                }
            }
        } else {
            conn.execute_batch(MEMORY_AUXILIARY_SCHEMA)?;
        }

        let rows: Vec<(String, String)> = {
            let mut stmt = conn.prepare(
                "SELECT id, content FROM memories WHERE embedding IS NOT NULL AND embedding_content_hash IS NULL",
            )?;

            stmt.query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?
        };
        for (id, content) in rows {
            let hash = Self::content_hash(&content);
            conn.execute(
                "UPDATE memories SET embedding_content_hash = ?1 WHERE id = ?2",
                params![hash, id],
            )?;
        }

        Ok(())
    }

    fn category_to_str(cat: &MemoryCategory) -> String {
        match cat {
            MemoryCategory::Core => "core".into(),
            MemoryCategory::Daily => "daily".into(),
            MemoryCategory::Conversation => "conversation".into(),
            MemoryCategory::Custom(name) => name.clone(),
        }
    }

    fn str_to_category(s: &str) -> MemoryCategory {
        match s {
            "core" => MemoryCategory::Core,
            "daily" => MemoryCategory::Daily,
            "conversation" => MemoryCategory::Conversation,
            other => MemoryCategory::Custom(other.to_string()),
        }
    }

    /// Deterministic content hash for embedding cache.
    /// Uses SHA-256 (truncated) instead of DefaultHasher, which is
    /// explicitly documented as unstable across Rust versions.
    pub fn content_hash(text: &str) -> String {
        use sha2::{Digest, Sha256};
        let hash = Sha256::digest(text.as_bytes());
        // First 8 bytes → 16 hex chars, matching previous format length
        format!(
            "{:016x}",
            u64::from_be_bytes(
                hash[..8]
                    .try_into()
                    .expect("SHA-256 always produces >= 8 bytes")
            )
        )
    }

    /// Provide access to the connection for advanced queries (e.g. retrieval pipeline).
    pub fn connection(&self) -> &Arc<Mutex<Connection>> {
        &self.conn
    }

    /// Number of pending deferred embedding tasks.
    pub fn pending_embeds_count(&self) -> usize {
        self.pending_embeds.load(Ordering::Relaxed)
    }

    /// Wait for all pending deferred embedding tasks to complete (up to `timeout_secs`).
    pub async fn drain_pending_embeds(&self, timeout_secs: u64) {
        let deadline = tokio::time::Instant::now() + tokio::time::Duration::from_secs(timeout_secs);
        while self.pending_embeds.load(Ordering::Relaxed) > 0 {
            if tokio::time::Instant::now() >= deadline {
                tracing::warn!(
                    "drain_pending_embeds timed out after {timeout_secs}s, {} tasks still pending",
                    self.pending_embeds.load(Ordering::Relaxed)
                );
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    }

    /// Spawn a background task to compute and fill a deferred embedding.
    ///
    /// The content_hash guards against stale updates: if the content has changed
    /// by the time the embedding is ready, the UPDATE affects 0 rows and the
    /// task silently discards the result.
    fn spawn_deferred_embedding(
        &self,
        namespace: String,
        key: String,
        content: String,
        content_hash: String,
    ) {
        self.pending_embeds.fetch_add(1, Ordering::Relaxed);
        let embedder = self.embedder.clone();
        let conn = self.conn.clone();
        let pending = self.pending_embeds.clone();

        tokio::spawn(async move {
            // Phase 1: async embedding computation
            let embedding_result = embedder.embed_one(&content).await;
            let key_for_log = key.clone();

            match embedding_result {
                Ok(emb) => {
                    let bytes = vector::vec_to_bytes(&emb);
                    // Phase 2: blocking DB update
                    let updated = tokio::task::spawn_blocking(move || -> anyhow::Result<usize> {
                        let conn = conn.lock();
                        let affected = conn.execute(
                            "UPDATE memories SET embedding = ?1 WHERE namespace = ?2 AND key = ?3 AND embedding_content_hash = ?4",
                            params![bytes, namespace, key, content_hash],
                        )?;
                        Ok(affected)
                    })
                    .await;

                    match updated {
                        Ok(Ok(n)) if n > 0 => {
                            tracing::debug!("deferred embedding filled for key={key_for_log}");
                        }
                        Ok(Ok(_)) => {
                            // content_hash mismatch — content changed, silently discard
                            tracing::debug!(
                                "deferred embedding discarded for key={key_for_log} (content changed)"
                            );
                        }
                        Ok(Err(e)) => {
                            tracing::warn!(
                                "deferred embedding DB write failed for key={key_for_log}: {e}"
                            );
                        }
                        Err(e) => {
                            tracing::warn!(
                                "deferred embedding blocking task failed for key={key_for_log}: {e}"
                            );
                        }
                    }
                }
                Err(e) => {
                    // Embedding computation failed — leave NULL, backfill will retry later
                    tracing::warn!(
                        "deferred embedding computation failed for key={key_for_log}: {e}"
                    );
                }
            }

            pending.fetch_sub(1, Ordering::Relaxed);
        });
    }

    /// Get embedding from cache, or compute + cache it
    pub async fn get_or_compute_embedding(&self, text: &str) -> anyhow::Result<Option<Vec<f32>>> {
        if self.embedder.dimensions() == 0 {
            return Ok(None); // Noop embedder
        }

        let hash = Self::content_hash(text);
        let now = Local::now().to_rfc3339();

        // Check cache (offloaded to blocking thread)
        let conn = self.conn.clone();
        let hash_c = hash.clone();
        let now_c = now.clone();
        let cached = tokio::task::spawn_blocking(move || -> anyhow::Result<Option<Vec<f32>>> {
            let conn = conn.lock();
            let mut stmt =
                conn.prepare("SELECT embedding FROM embedding_cache WHERE content_hash = ?1")?;
            let blob: Option<Vec<u8>> = stmt.query_row(params![hash_c], |row| row.get(0)).ok();
            if let Some(bytes) = blob {
                conn.execute(
                    "UPDATE embedding_cache SET accessed_at = ?1 WHERE content_hash = ?2",
                    params![now_c, hash_c],
                )?;
                return Ok(Some(vector::bytes_to_vec(&bytes)));
            }
            Ok(None)
        })
        .await??;

        if cached.is_some() {
            return Ok(cached);
        }

        // Compute embedding (async I/O)
        let embedding = self.embedder.embed_one(text).await?;
        let bytes = vector::vec_to_bytes(&embedding);

        // Store in cache + LRU eviction (offloaded to blocking thread)
        let conn = self.conn.clone();
        #[allow(clippy::cast_possible_wrap)]
        let cache_max = self.cache_max as i64;
        tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
            let conn = conn.lock();
            conn.execute(
                "INSERT OR REPLACE INTO embedding_cache (content_hash, embedding, created_at, accessed_at)
                 VALUES (?1, ?2, ?3, ?4)",
                params![hash, bytes, now, now],
            )?;
            conn.execute(
                "DELETE FROM embedding_cache WHERE content_hash IN (
                    SELECT content_hash FROM embedding_cache
                    ORDER BY accessed_at ASC
                    LIMIT MAX(0, (SELECT COUNT(*) FROM embedding_cache) - ?1)
                )",
                params![cache_max],
            )?;
            Ok(())
        })
        .await??;

        Ok(Some(embedding))
    }

    /// FTS5 BM25 keyword search
    pub fn fts5_search(
        conn: &Connection,
        query: &str,
        limit: usize,
    ) -> anyhow::Result<Vec<(String, f32)>> {
        // Escape FTS5 special chars and build query
        let fts_query: String = query
            .split_whitespace()
            .map(|w| format!("\"{w}\""))
            .collect::<Vec<_>>()
            .join(" OR ");

        if fts_query.is_empty() {
            return Ok(Vec::new());
        }

        let sql = "SELECT m.id, bm25(memories_fts) as score
                   FROM memories_fts f
                   JOIN memories m ON m.rowid = f.rowid
                   WHERE memories_fts MATCH ?1
                   ORDER BY score
                   LIMIT ?2";

        let mut stmt = conn.prepare(sql)?;
        #[allow(clippy::cast_possible_wrap)]
        let limit_i64 = limit as i64;

        let rows = stmt.query_map(params![fts_query, limit_i64], |row| {
            let id: String = row.get(0)?;
            let score: f64 = row.get(1)?;
            // BM25 returns negative scores (lower = better), negate for ranking
            #[allow(clippy::cast_possible_truncation)]
            Ok((id, (-score) as f32))
        })?;

        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }
        Ok(results)
    }

    /// Vector similarity search: scan embeddings and compute cosine similarity.
    ///
    /// Optional `category` and `session_id` filters reduce full-table scans
    /// when the caller already knows the scope of relevant memories.
    pub fn vector_search(
        conn: &Connection,
        query_embedding: &[f32],
        limit: usize,
        category: Option<&str>,
        session_id: Option<&str>,
    ) -> anyhow::Result<Vec<(String, f32)>> {
        let mut sql = "SELECT id, embedding FROM memories WHERE embedding IS NOT NULL".to_string();
        let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
        let mut idx = 1;

        if let Some(cat) = category {
            let _ = write!(sql, " AND category = ?{idx}");
            param_values.push(Box::new(cat.to_string()));
            idx += 1;
        }
        if let Some(sid) = session_id {
            let _ = write!(sql, " AND session_id = ?{idx}");
            param_values.push(Box::new(sid.to_string()));
        }

        let mut stmt = conn.prepare(&sql)?;
        let params_ref: Vec<&dyn rusqlite::types::ToSql> =
            param_values.iter().map(AsRef::as_ref).collect();
        let rows = stmt.query_map(params_ref.as_slice(), |row| {
            let id: String = row.get(0)?;
            let blob: Vec<u8> = row.get(1)?;
            Ok((id, blob))
        })?;

        let mut scored: Vec<(String, f32)> = Vec::new();
        for row in rows {
            let (id, blob) = row?;
            let emb = vector::bytes_to_vec(&blob);
            let sim = vector::cosine_similarity(query_embedding, &emb);
            if sim > 0.0 {
                scored.push((id, sim));
            }
        }

        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(limit);
        Ok(scored)
    }

    /// Safe reindex: rebuild FTS5 + embeddings with rollback on failure
    pub async fn reindex(&self) -> anyhow::Result<usize> {
        // Step 1: Rebuild FTS5
        {
            let conn = self.conn.clone();
            tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
                let conn = conn.lock();
                conn.execute_batch("INSERT INTO memories_fts(memories_fts) VALUES('rebuild');")?;
                Ok(())
            })
            .await??;
        }

        // Step 2: Re-embed all memories that lack embeddings
        if self.embedder.dimensions() == 0 {
            return Ok(0);
        }

        let conn = self.conn.clone();
        let entries: Vec<(String, String)> = tokio::task::spawn_blocking(move || {
            let conn = conn.lock();
            let mut stmt =
                conn.prepare("SELECT id, content FROM memories WHERE embedding IS NULL")?;
            let rows = stmt.query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?;
            Ok::<_, anyhow::Error>(rows.filter_map(std::result::Result::ok).collect())
        })
        .await??;

        let mut count = 0;
        for (id, content) in &entries {
            if let Ok(Some(emb)) = self.get_or_compute_embedding(content).await {
                let bytes = vector::vec_to_bytes(&emb);
                let conn = self.conn.clone();
                let id = id.clone();
                tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
                    let conn = conn.lock();
                    conn.execute(
                        "UPDATE memories SET embedding = ?1 WHERE id = ?2",
                        params![bytes, id],
                    )?;
                    Ok(())
                })
                .await??;
                count += 1;
            }
        }

        Ok(count)
    }

    /// Backfill embeddings for memories that lack them.
    ///
    /// Processes `batch_size` memories at a time to avoid API rate limits
    /// and long blocking operations. Returns the total number of memories
    /// that were successfully embedded.
    pub async fn backfill_embeddings(&self, batch_size: usize) -> anyhow::Result<usize> {
        if self.embedder.dimensions() == 0 {
            return Ok(0);
        }

        let mut total_count = 0;

        loop {
            let conn = self.conn.clone();
            let entries: Vec<(String, String)> = tokio::task::spawn_blocking(move || {
                let conn = conn.lock();
                let mut stmt = conn
                    .prepare("SELECT id, content FROM memories WHERE embedding IS NULL LIMIT ?1")?;
                let rows = stmt.query_map(params![batch_size], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })?;
                Ok::<_, anyhow::Error>(rows.filter_map(std::result::Result::ok).collect())
            })
            .await??;

            if entries.is_empty() {
                break;
            }

            let batch_len = entries.len();
            let mut batch_count = 0;

            for (id, content) in &entries {
                if let Ok(Some(emb)) = self.get_or_compute_embedding(content).await {
                    let bytes = vector::vec_to_bytes(&emb);
                    let conn = self.conn.clone();
                    let id = id.clone();
                    tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
                        let conn = conn.lock();
                        conn.execute(
                            "UPDATE memories SET embedding = ?1 WHERE id = ?2",
                            params![bytes, id],
                        )?;
                        Ok(())
                    })
                    .await??;
                    batch_count += 1;
                }
            }

            total_count += batch_count;
            tracing::info!(
                "Backfill batch: embedded {batch_count}/{batch_len} memories (total: {total_count})"
            );

            // If we got fewer entries than batch_size, we've exhausted all NULL rows.
            if entries.len() < batch_size {
                break;
            }
        }

        Ok(total_count)
    }
}

#[async_trait]
impl Memory for SqliteMemory {
    fn name(&self) -> &str {
        "sqlite"
    }

    async fn store(
        &self,
        key: &str,
        content: &str,
        category: MemoryCategory,
        session_id: Option<&str>,
    ) -> anyhow::Result<()> {
        self.store_with_metadata(
            key,
            content,
            category,
            session_id,
            Some("default"),
            Some(0.5),
        )
        .await
    }

    async fn top_core_memories(&self, limit: usize) -> anyhow::Result<Vec<MemoryEntry>> {
        self.top_core_memories_scoped("default", limit).await
    }

    async fn top_core_memories_scoped(
        &self,
        namespace: &str,
        limit: usize,
    ) -> anyhow::Result<Vec<MemoryEntry>> {
        let conn = self.conn.clone();
        let namespace = namespace.to_string();

        tokio::task::spawn_blocking(move || -> anyhow::Result<Vec<MemoryEntry>> {
            let conn = conn.lock();
            let mut stmt = conn.prepare(
                "SELECT id, key, content, category, created_at, session_id, namespace,
                        importance, superseded_by
                 FROM memories
                 WHERE namespace = ?1 AND category = 'core' AND superseded_by IS NULL
                 ORDER BY importance DESC, updated_at DESC, key ASC
                 LIMIT ?2",
            )?;
            #[allow(clippy::cast_possible_wrap)]
            let rows = stmt.query_map(params![namespace, limit as i64], |row| {
                Ok(MemoryEntry {
                    id: row.get(0)?,
                    key: row.get(1)?,
                    content: row.get(2)?,
                    category: Self::str_to_category(&row.get::<_, String>(3)?),
                    timestamp: row.get(4)?,
                    session_id: row.get(5)?,
                    score: None,
                    namespace: row.get(6)?,
                    importance: row.get(7)?,
                    superseded_by: row.get(8)?,
                    embedding: None,
                })
            })?;
            rows.collect::<rusqlite::Result<Vec<_>>>()
                .map_err(Into::into)
        })
        .await?
    }

    /// Recall memories with embedding vectors included.
    ///
    /// Same as `recall()` but populates the `embedding` field on each entry.
    /// Used by conflict detection (Phase E) which needs embeddings for
    /// cosine similarity computation.
    async fn recall_with_embeddings(
        &self,
        query: &str,
        limit: usize,
        session_id: Option<&str>,
        since: Option<&str>,
        until: Option<&str>,
        search_mode: Option<SearchMode>,
    ) -> anyhow::Result<Vec<MemoryEntry>> {
        self.recall_scoped_with_embeddings(MemoryQuery {
            query,
            scope: MemoryScope {
                namespace: "default",
                session_id,
            },
            category: None,
            since,
            until,
            limit,
            min_relevance_score: None,
            search_mode,
            exclude_ids: &[],
            exclude_keys: &[],
        })
        .await
    }

    async fn recall_scoped_with_embeddings(
        &self,
        query: MemoryQuery<'_>,
    ) -> anyhow::Result<Vec<MemoryEntry>> {
        let entries = self.recall_scoped(query).await?;

        if entries.is_empty() || self.embedder.dimensions() == 0 {
            return Ok(entries);
        }

        let ids: Vec<String> = entries.iter().map(|e| e.id.clone()).collect();
        let conn = self.conn.clone();

        let embeddings_map: std::collections::HashMap<String, Vec<f32>> =
            tokio::task::spawn_blocking(move || -> anyhow::Result<std::collections::HashMap<String, Vec<f32>>> {
                let conn = conn.lock();

                // Build IN clause with placeholders
                let placeholders: String = ids
                    .iter()
                    .enumerate()
                    .map(|(i, _)| format!("?{}", i + 1))
                    .collect::<Vec<_>>()
                    .join(",");
                let sql = format!(
                    "SELECT id, embedding FROM memories WHERE id IN ({placeholders}) AND embedding IS NOT NULL"
                );
                let mut stmt = conn.prepare(&sql)?;
                let params: Vec<String> = ids;
                let params_ref: Vec<&dyn rusqlite::types::ToSql> = params
                    .iter()
                    .map(|id| id as &dyn rusqlite::types::ToSql)
                    .collect();

                let mut map = std::collections::HashMap::new();
                let rows = stmt.query_map(params_ref.as_slice(), |row| {
                    let id: String = row.get(0)?;
                    let blob: Vec<u8> = row.get(1)?;
                    Ok((id, vector::bytes_to_vec(&blob)))
                })?;
                for row in rows {
                    let (id, emb) = row?;
                    map.insert(id, emb);
                }
                Ok(map)
            })
            .await??;

        let mut entries = entries;
        for entry in &mut entries {
            if let Some(emb) = embeddings_map.get(&entry.id) {
                entry.embedding = Some(emb.clone());
            }
        }
        Ok(entries)
    }

    async fn recall(
        &self,
        query: &str,
        limit: usize,
        session_id: Option<&str>,
        since: Option<&str>,
        until: Option<&str>,
        search_mode: Option<SearchMode>,
    ) -> anyhow::Result<Vec<MemoryEntry>> {
        self.recall_scoped(MemoryQuery {
            query,
            scope: MemoryScope {
                namespace: "default",
                session_id,
            },
            category: None,
            since,
            until,
            limit,
            min_relevance_score: None,
            search_mode,
            exclude_ids: &[],
            exclude_keys: &[],
        })
        .await
    }

    async fn get(&self, key: &str) -> anyhow::Result<Option<MemoryEntry>> {
        self.get_scoped(MemoryScope::default(), key).await
    }

    async fn get_scoped(
        &self,
        scope: MemoryScope<'_>,
        key: &str,
    ) -> anyhow::Result<Option<MemoryEntry>> {
        let conn = self.conn.clone();
        let key = key.to_string();
        let namespace = scope.namespace.to_string();
        let session_id = scope.session_id.map(String::from);

        tokio::task::spawn_blocking(move || -> anyhow::Result<Option<MemoryEntry>> {
            let conn = conn.lock();
            let mut stmt = conn.prepare(
                "SELECT id, key, content, category, created_at, session_id, namespace, importance, superseded_by
                 FROM memories
                 WHERE namespace = ?1 AND key = ?2 AND (?3 IS NULL OR session_id = ?3)",
            )?;

            let mut rows = stmt.query_map(params![namespace, key, session_id], |row| {
                Ok(MemoryEntry {
                    id: row.get(0)?,
                    key: row.get(1)?,
                    content: row.get(2)?,
                    category: Self::str_to_category(&row.get::<_, String>(3)?),
                    timestamp: row.get(4)?,
                    session_id: row.get(5)?,
                    score: None,
                    namespace: row.get::<_, Option<String>>(6)?.unwrap_or_else(|| "default".into()),
                    importance: row.get(7)?,
                    superseded_by: row.get(8)?,
                    embedding: None,
                })
            })?;

            match rows.next() {
                Some(Ok(entry)) => Ok(Some(entry)),
                _ => Ok(None),
            }
        })
        .await?
    }

    async fn list(
        &self,
        category: Option<&MemoryCategory>,
        session_id: Option<&str>,
    ) -> anyhow::Result<Vec<MemoryEntry>> {
        self.list_scoped(
            MemoryScope {
                namespace: "default",
                session_id,
            },
            category,
        )
        .await
    }

    async fn list_scoped(
        &self,
        scope: MemoryScope<'_>,
        category: Option<&MemoryCategory>,
    ) -> anyhow::Result<Vec<MemoryEntry>> {
        const DEFAULT_LIST_LIMIT: i64 = 1000;
        let conn = self.conn.clone();
        let namespace = scope.namespace.to_string();
        let session_id = scope.session_id.map(String::from);
        let category = category.map(Self::category_to_str);

        tokio::task::spawn_blocking(move || -> anyhow::Result<Vec<MemoryEntry>> {
            let conn = conn.lock();
            let mut stmt = conn.prepare(
                "SELECT id, key, content, category, created_at, session_id, namespace,
                        importance, superseded_by
                 FROM memories
                 WHERE superseded_by IS NULL
                   AND namespace = ?1
                   AND (?2 IS NULL OR category = ?2)
                   AND (?3 IS NULL OR session_id = ?3)
                 ORDER BY updated_at DESC, key ASC
                 LIMIT ?4",
            )?;
            let rows = stmt.query_map(
                params![namespace, category, session_id, DEFAULT_LIST_LIMIT],
                |row| {
                    Ok(MemoryEntry {
                        id: row.get(0)?,
                        key: row.get(1)?,
                        content: row.get(2)?,
                        category: Self::str_to_category(&row.get::<_, String>(3)?),
                        timestamp: row.get(4)?,
                        session_id: row.get(5)?,
                        score: None,
                        namespace: row.get(6)?,
                        importance: row.get(7)?,
                        superseded_by: row.get(8)?,
                        embedding: None,
                    })
                },
            )?;
            rows.collect::<rusqlite::Result<Vec<_>>>()
                .map_err(Into::into)
        })
        .await?
    }

    async fn forget(&self, key: &str) -> anyhow::Result<bool> {
        self.forget_scoped(MemoryScope::default(), key).await
    }

    async fn forget_scoped(&self, scope: MemoryScope<'_>, key: &str) -> anyhow::Result<bool> {
        let conn = self.conn.clone();
        let key = key.to_string();
        let namespace = scope.namespace.to_string();
        let session_id = scope.session_id.map(String::from);

        tokio::task::spawn_blocking(move || -> anyhow::Result<bool> {
            let conn = conn.lock();
            let affected = if let Some(session_id) = session_id {
                conn.execute(
                    "DELETE FROM memories WHERE namespace = ?1 AND key = ?2 AND session_id = ?3",
                    params![namespace, key, session_id],
                )?
            } else {
                conn.execute(
                    "DELETE FROM memories WHERE namespace = ?1 AND key = ?2",
                    params![namespace, key],
                )?
            };
            Ok(affected > 0)
        })
        .await?
    }

    async fn purge_namespace(&self, namespace: &str) -> anyhow::Result<usize> {
        let conn = self.conn.clone();
        let namespace = namespace.to_string();

        tokio::task::spawn_blocking(move || -> anyhow::Result<usize> {
            let conn = conn.lock();
            let affected = conn.execute(
                "DELETE FROM memories WHERE namespace = ?1",
                params![namespace],
            )?;
            #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
            Ok(affected)
        })
        .await?
    }

    async fn purge_session(&self, session_id: &str) -> anyhow::Result<usize> {
        let conn = self.conn.clone();
        let session_id = session_id.to_string();

        tokio::task::spawn_blocking(move || -> anyhow::Result<usize> {
            let conn = conn.lock();
            let affected = conn.execute(
                "DELETE FROM memories WHERE session_id = ?1",
                params![session_id],
            )?;
            #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
            Ok(affected)
        })
        .await?
    }

    async fn count(&self) -> anyhow::Result<usize> {
        let conn = self.conn.clone();

        tokio::task::spawn_blocking(move || -> anyhow::Result<usize> {
            let conn = conn.lock();
            let count: i64 =
                conn.query_row("SELECT COUNT(*) FROM memories", [], |row| row.get(0))?;
            #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
            Ok(count as usize)
        })
        .await?
    }

    async fn health_check(&self) -> bool {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || conn.lock().execute_batch("SELECT 1").is_ok())
            .await
            .unwrap_or(false)
    }

    async fn export(&self, filter: &ExportFilter) -> anyhow::Result<Vec<MemoryEntry>> {
        let conn = self.conn.clone();
        let filter = filter.clone();

        tokio::task::spawn_blocking(move || -> anyhow::Result<Vec<MemoryEntry>> {
            let conn = conn.lock();
            let mut sql =
                "SELECT id, key, content, category, created_at, session_id, namespace, importance, superseded_by \
                 FROM memories WHERE 1=1"
                    .to_string();
            let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
            let mut idx = 1;

            if let Some(ref ns) = filter.namespace {
                let _ = write!(sql, " AND namespace = ?{idx}");
                param_values.push(Box::new(ns.clone()));
                idx += 1;
            }
            if let Some(ref sid) = filter.session_id {
                let _ = write!(sql, " AND session_id = ?{idx}");
                param_values.push(Box::new(sid.clone()));
                idx += 1;
            }
            if let Some(ref cat) = filter.category {
                let _ = write!(sql, " AND category = ?{idx}");
                param_values.push(Box::new(Self::category_to_str(cat)));
                idx += 1;
            }
            if let Some(ref since) = filter.since {
                let _ = write!(sql, " AND created_at >= ?{idx}");
                param_values.push(Box::new(since.clone()));
                idx += 1;
            }
            if let Some(ref until) = filter.until {
                let _ = write!(sql, " AND created_at <= ?{idx}");
                param_values.push(Box::new(until.clone()));
                let _ = idx;
            }
            sql.push_str(" ORDER BY created_at ASC");

            let mut stmt = conn.prepare(&sql)?;
            let params_ref: Vec<&dyn rusqlite::types::ToSql> =
                param_values.iter().map(AsRef::as_ref).collect();
            let rows = stmt.query_map(params_ref.as_slice(), |row| {
                Ok(MemoryEntry {
                    id: row.get(0)?,
                    key: row.get(1)?,
                    content: row.get(2)?,
                    category: Self::str_to_category(&row.get::<_, String>(3)?),
                    timestamp: row.get(4)?,
                    session_id: row.get(5)?,
                    score: None,
                    namespace: row.get::<_, Option<String>>(6)?.unwrap_or_else(|| "default".into()),
                    importance: row.get(7)?,
                    superseded_by: row.get(8)?,
                    embedding: None,
                })
            })?;

            let mut results = Vec::new();
            for row in rows {
                results.push(row?);
            }
            Ok(results)
        })
        .await?
    }

    async fn recall_namespaced(
        &self,
        namespace: &str,
        query: &str,
        limit: usize,
        session_id: Option<&str>,
        since: Option<&str>,
        until: Option<&str>,
        search_mode: Option<SearchMode>,
    ) -> anyhow::Result<Vec<MemoryEntry>> {
        self.recall_scoped(MemoryQuery {
            query,
            scope: MemoryScope {
                namespace,
                session_id,
            },
            category: None,
            since,
            until,
            limit,
            min_relevance_score: None,
            search_mode,
            exclude_ids: &[],
            exclude_keys: &[],
        })
        .await
    }

    async fn recall_scoped(&self, query: MemoryQuery<'_>) -> anyhow::Result<Vec<MemoryEntry>> {
        if query.limit == 0 {
            return Ok(Vec::new());
        }

        let effective_mode = query.search_mode.unwrap_or(self.search_mode);
        let query_embedding = if query.query.trim().is_empty() || effective_mode == SearchMode::Bm25
        {
            None
        } else {
            self.get_or_compute_embedding(query.query).await?
        };

        let conn = self.conn.clone();
        let query_text = query.query.to_string();
        let namespace = query.scope.namespace.to_string();
        let category = query.category.map(Self::category_to_str);
        let session_id = query.scope.session_id.map(String::from);
        let since = query.since.map(String::from);
        let until = query.until.map(String::from);
        let exclude_ids = serde_json::to_string(query.exclude_ids)?;
        let exclude_keys = serde_json::to_string(query.exclude_keys)?;
        let minimum = query.min_relevance_score;
        let limit = query.limit;
        let merge_strategy = self.merge_strategy.clone();

        tokio::task::spawn_blocking(move || -> anyhow::Result<Vec<MemoryEntry>> {
            let conn = conn.lock();
            #[allow(clippy::cast_possible_wrap)]
            let candidate_limit = limit.saturating_mul(2) as i64;

            let map_entry = |row: &rusqlite::Row<'_>| -> rusqlite::Result<MemoryEntry> {
                Ok(MemoryEntry {
                    id: row.get(0)?,
                    key: row.get(1)?,
                    content: row.get(2)?,
                    category: Self::str_to_category(&row.get::<_, String>(3)?),
                    timestamp: row.get(4)?,
                    session_id: row.get(5)?,
                    score: None,
                    namespace: row.get(6)?,
                    importance: row.get(7)?,
                    superseded_by: row.get(8)?,
                    embedding: None,
                })
            };

            if query_text.trim().is_empty() {
                if minimum.is_some() {
                    return Ok(Vec::new());
                }
                let mut stmt = conn.prepare(
                    "SELECT id, key, content, category, created_at, session_id, namespace,
                            importance, superseded_by
                     FROM memories
                     WHERE superseded_by IS NULL
                       AND namespace = ?1
                       AND (?2 IS NULL OR category = ?2)
                       AND (?3 IS NULL OR session_id = ?3)
                       AND (?4 IS NULL OR created_at >= ?4)
                       AND (?5 IS NULL OR created_at <= ?5)
                       AND NOT EXISTS (SELECT 1 FROM json_each(?6) WHERE value = memories.id)
                       AND NOT EXISTS (SELECT 1 FROM json_each(?7) WHERE value = memories.key)
                     ORDER BY updated_at DESC, key ASC
                     LIMIT ?8",
                )?;
                let rows = stmt.query_map(
                    params![
                        namespace,
                        category,
                        session_id,
                        since,
                        until,
                        exclude_ids,
                        exclude_keys,
                        candidate_limit
                    ],
                    map_entry,
                )?;
                return rows
                    .take(limit)
                    .collect::<rusqlite::Result<Vec<_>>>()
                    .map_err(Into::into);
            }

            let fts_query = query_text
                .split_whitespace()
                .map(|word| format!("\"{}\"", word.replace('"', "\"\"")))
                .collect::<Vec<_>>()
                .join(" OR ");

            let lexical_query = super::relevance::LexicalQuery::new(&query_text);
            let mut lexical_scores = std::collections::HashMap::new();
            let keyword_results = if effective_mode == SearchMode::Embedding || fts_query.is_empty()
            {
                Vec::new()
            } else {
                let mut stmt = conn.prepare(
                    "SELECT m.id, bm25(memories_fts) AS score, m.key, m.content
                     FROM memories_fts f
                     JOIN memories m ON m.rowid = f.rowid
                     WHERE memories_fts MATCH ?1
                       AND m.superseded_by IS NULL
                       AND m.namespace = ?2
                       AND (?3 IS NULL OR m.category = ?3)
                       AND (?4 IS NULL OR m.session_id = ?4)
                       AND (?5 IS NULL OR m.created_at >= ?5)
                       AND (?6 IS NULL OR m.created_at <= ?6)
                       AND NOT EXISTS (SELECT 1 FROM json_each(?7) WHERE value = m.id)
                       AND NOT EXISTS (SELECT 1 FROM json_each(?8) WHERE value = m.key)
                     ORDER BY score, m.updated_at DESC, m.key ASC
                     LIMIT ?9",
                )?;
                let rows = stmt.query_map(
                    params![
                        fts_query,
                        namespace,
                        category,
                        session_id,
                        since,
                        until,
                        exclude_ids,
                        exclude_keys,
                        if minimum.is_some() {
                            -1
                        } else {
                            candidate_limit
                        }
                    ],
                    |row| {
                        let score: f64 = row.get(1)?;
                        #[allow(clippy::cast_possible_truncation)]
                        Ok((
                            row.get::<_, String>(0)?,
                            (-score) as f32,
                            row.get::<_, String>(2)?,
                            row.get::<_, String>(3)?,
                        ))
                    },
                )?;
                let mut candidates = Vec::new();
                for row in rows {
                    let (id, rank, key, content) = row?;
                    let score = lexical_query.score(&key, &content);
                    lexical_scores.insert(id.clone(), score);
                    if minimum.is_none_or(|minimum| score > 0.0 && score >= minimum) {
                        candidates.push((id, rank));
                        if candidates.len() >= limit.saturating_mul(2) {
                            break;
                        }
                    }
                }
                candidates
            };

            let vector_results = if effective_mode == SearchMode::Bm25 {
                Vec::new()
            } else if let Some(ref query_embedding) = query_embedding {
                let mut stmt = conn.prepare(
                    "SELECT id, embedding
                     FROM memories
                     WHERE embedding IS NOT NULL
                       AND superseded_by IS NULL
                       AND namespace = ?1
                       AND (?2 IS NULL OR category = ?2)
                       AND (?3 IS NULL OR session_id = ?3)
                       AND (?4 IS NULL OR created_at >= ?4)
                       AND (?5 IS NULL OR created_at <= ?5)
                       AND NOT EXISTS (SELECT 1 FROM json_each(?6) WHERE value = memories.id)
                       AND NOT EXISTS (SELECT 1 FROM json_each(?7) WHERE value = memories.key)",
                )?;
                let rows = stmt.query_map(
                    params![
                        namespace,
                        category,
                        session_id,
                        since,
                        until,
                        exclude_ids,
                        exclude_keys
                    ],
                    |row| Ok((row.get::<_, String>(0)?, row.get::<_, Vec<u8>>(1)?)),
                )?;
                let mut scored = rows
                    .collect::<rusqlite::Result<Vec<_>>>()?
                    .into_iter()
                    .filter_map(|(id, bytes)| {
                        let score = vector::cosine_similarity(
                            query_embedding,
                            &vector::bytes_to_vec(&bytes),
                        );
                        (score > 0.0).then_some((id, score))
                    })
                    .collect::<Vec<_>>();
                scored.sort_by(|left, right| {
                    right
                        .1
                        .partial_cmp(&left.1)
                        .unwrap_or(std::cmp::Ordering::Equal)
                        .then_with(|| left.0.cmp(&right.0))
                });
                scored.truncate(limit.saturating_mul(2));
                scored
            } else {
                Vec::new()
            };

            // Fuse all candidates first. Ranking scores (especially RRF) are not
            // relevance scores, and filtering must precede the final result limit.
            let merge_limit = vector_results.len() + keyword_results.len();
            let mut merged = if vector_results.is_empty() {
                keyword_results
                    .into_iter()
                    .map(|(id, score)| vector::ScoredResult {
                        id,
                        vector_score: None,
                        keyword_score: Some(score),
                        final_score: score,
                    })
                    .collect::<Vec<_>>()
            } else if keyword_results.is_empty() {
                vector_results
                    .into_iter()
                    .map(|(id, score)| vector::ScoredResult {
                        id,
                        vector_score: Some(score),
                        keyword_score: None,
                        final_score: score,
                    })
                    .collect::<Vec<_>>()
            } else {
                match merge_strategy {
                    MergeStrategy::Rrf { k } => {
                        vector::rrf_merge(&vector_results, &keyword_results, k, merge_limit)
                    }
                    MergeStrategy::Weighted {
                        vector_weight,
                        keyword_weight,
                    } => vector::hybrid_merge(
                        &vector_results,
                        &keyword_results,
                        vector_weight,
                        keyword_weight,
                        merge_limit,
                    ),
                }
            };
            let relevance = |entry: &vector::ScoredResult| {
                f64::from(entry.vector_score.unwrap_or_default())
                    .max(lexical_scores.get(&entry.id).copied().unwrap_or_default())
            };
            if let Some(minimum) = minimum {
                merged.retain(|entry| relevance(entry) > 0.0 && relevance(entry) >= minimum);
            }
            merged.truncate(limit);

            let mut results = Vec::with_capacity(merged.len());
            if !merged.is_empty() {
                let placeholders = (1..=merged.len())
                    .map(|index| format!("?{index}"))
                    .collect::<Vec<_>>()
                    .join(",");
                let sql = format!(
                    "SELECT id, key, content, category, created_at, session_id, namespace,
                            importance, superseded_by
                     FROM memories WHERE id IN ({placeholders})"
                );
                let mut stmt = conn.prepare(&sql)?;
                let values = merged
                    .iter()
                    .map(|entry| entry.id.clone())
                    .collect::<Vec<_>>();
                let params = values
                    .iter()
                    .map(|value| value as &dyn rusqlite::types::ToSql)
                    .collect::<Vec<_>>();
                let entries = stmt
                    .query_map(params.as_slice(), map_entry)?
                    .collect::<rusqlite::Result<Vec<_>>>()?
                    .into_iter()
                    .map(|entry| (entry.id.clone(), entry))
                    .collect::<std::collections::HashMap<_, _>>();
                for scored in merged {
                    if let Some(mut entry) = entries.get(&scored.id).cloned() {
                        entry.score = Some(relevance(&scored));
                        results.push(entry);
                    }
                }
            }

            if results.is_empty() && effective_mode != SearchMode::Embedding {
                let literal = query_text
                    .trim()
                    .replace('\\', "\\\\")
                    .replace('%', "\\%")
                    .replace('_', "\\_");
                let pattern = format!("%{literal}%");
                let mut stmt = conn.prepare(
                    "SELECT id, key, content, category, created_at, session_id, namespace,
                            importance, superseded_by
                     FROM memories
                     WHERE superseded_by IS NULL
                       AND (content LIKE ?1 ESCAPE '\\' OR key LIKE ?1 ESCAPE '\\')
                       AND namespace = ?2
                       AND (?3 IS NULL OR category = ?3)
                       AND (?4 IS NULL OR session_id = ?4)
                       AND (?5 IS NULL OR created_at >= ?5)
                       AND (?6 IS NULL OR created_at <= ?6)
                       AND NOT EXISTS (SELECT 1 FROM json_each(?7) WHERE value = memories.id)
                       AND NOT EXISTS (SELECT 1 FROM json_each(?8) WHERE value = memories.key)
                     ORDER BY updated_at DESC, key ASC
                     LIMIT ?9",
                )?;
                let rows = stmt.query_map(
                    params![
                        pattern,
                        namespace,
                        category,
                        session_id,
                        since,
                        until,
                        exclude_ids,
                        exclude_keys,
                        candidate_limit
                    ],
                    map_entry,
                )?;
                results = rows
                    .collect::<rusqlite::Result<Vec<_>>>()?
                    .into_iter()
                    .filter_map(|mut entry| {
                        let score = lexical_query.score(&entry.key, &entry.content);
                        entry.score = Some(score);
                        minimum
                            .is_none_or(|minimum| score > 0.0 && score >= minimum)
                            .then_some(entry)
                    })
                    .take(limit)
                    .collect();
            }

            Ok(results)
        })
        .await?
    }

    async fn store_with_metadata(
        &self,
        key: &str,
        content: &str,
        category: MemoryCategory,
        session_id: Option<&str>,
        namespace: Option<&str>,
        importance: Option<f64>,
    ) -> anyhow::Result<()> {
        let cached_embedding = self.get_or_compute_embedding(content).await?;

        if let Some(emb) = cached_embedding {
            let embedding_bytes = vector::vec_to_bytes(&emb);
            let conn = self.conn.clone();
            let key = key.to_string();
            let content = content.to_string();
            let sid = session_id.map(String::from);
            let ns = namespace.unwrap_or("default").to_string();
            let imp = importance.unwrap_or(0.5);
            let content_hash = Self::content_hash(&content);

            tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
                let conn = conn.lock();
                let now = Local::now().to_rfc3339();
                let cat = Self::category_to_str(&category);
                let id = Uuid::new_v4().to_string();

                conn.execute(
                    "INSERT INTO memories (id, key, content, category, embedding, created_at, updated_at, session_id, namespace, importance, embedding_content_hash)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
                     ON CONFLICT(namespace, key) DO UPDATE SET
                        content = excluded.content,
                        category = excluded.category,
                        embedding = excluded.embedding,
                        updated_at = excluded.updated_at,
                        session_id = excluded.session_id,
                        namespace = excluded.namespace,
                        importance = excluded.importance,
                        embedding_content_hash = excluded.embedding_content_hash",
                    params![id, key, content, cat, embedding_bytes, now, now, sid, ns, imp, content_hash],
                )?;
                Ok(())
            })
            .await?
        } else if self.defer_embedding && self.embedder.dimensions() > 0 {
            let content_hash = Self::content_hash(content);
            let content_hash_for_db = content_hash.clone();
            let conn = self.conn.clone();
            let key_for_db = key.to_string();
            let content_for_db = content.to_string();
            let sid = session_id.map(String::from);
            let ns = namespace.unwrap_or("default").to_string();
            let imp = importance.unwrap_or(0.5);

            let _ = tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
                let conn = conn.lock();
                let now = Local::now().to_rfc3339();
                let cat = Self::category_to_str(&category);
                let id = Uuid::new_v4().to_string();

                conn.execute(
                    "INSERT INTO memories (id, key, content, category, embedding, created_at, updated_at, session_id, namespace, importance, embedding_content_hash)
                     VALUES (?1, ?2, ?3, ?4, NULL, ?5, ?6, ?7, ?8, ?9, ?10)
                     ON CONFLICT(namespace, key) DO UPDATE SET
                        content = excluded.content,
                        category = excluded.category,
                        embedding = NULL,
                        updated_at = excluded.updated_at,
                        session_id = excluded.session_id,
                        namespace = excluded.namespace,
                        importance = excluded.importance,
                        embedding_content_hash = excluded.embedding_content_hash",
                    params![id, key_for_db, content_for_db, cat, now, now, sid, ns, imp, content_hash_for_db],
                )?;
                Ok(())
            })
            .await;

            self.spawn_deferred_embedding(
                namespace.unwrap_or("default").to_string(),
                key.to_string(),
                content.to_string(),
                content_hash,
            );
            Ok(())
        } else {
            let conn = self.conn.clone();
            let key = key.to_string();
            let content = content.to_string();
            let sid = session_id.map(String::from);
            let ns = namespace.unwrap_or("default").to_string();
            let imp = importance.unwrap_or(0.5);
            let content_hash = Self::content_hash(&content);

            tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
                let conn = conn.lock();
                let now = Local::now().to_rfc3339();
                let cat = Self::category_to_str(&category);
                let id = Uuid::new_v4().to_string();

                conn.execute(
                    "INSERT INTO memories (id, key, content, category, embedding, created_at, updated_at, session_id, namespace, importance, embedding_content_hash)
                     VALUES (?1, ?2, ?3, ?4, NULL, ?5, ?6, ?7, ?8, ?9, ?10)
                     ON CONFLICT(namespace, key) DO UPDATE SET
                        content = excluded.content,
                        category = excluded.category,
                        embedding = excluded.embedding,
                        updated_at = excluded.updated_at,
                        session_id = excluded.session_id,
                        namespace = excluded.namespace,
                        importance = excluded.importance,
                        embedding_content_hash = excluded.embedding_content_hash",
                    params![id, key, content, cat, now, now, sid, ns, imp, content_hash],
                )?;
                Ok(())
            })
            .await?
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn temp_sqlite() -> (TempDir, SqliteMemory) {
        let tmp = TempDir::new().unwrap();
        let mem = SqliteMemory::new(tmp.path()).unwrap();
        (tmp, mem)
    }

    #[tokio::test]
    async fn sqlite_name() {
        let (_tmp, mem) = temp_sqlite();
        assert_eq!(mem.name(), "sqlite");
    }

    #[tokio::test]
    async fn sqlite_health() {
        let (_tmp, mem) = temp_sqlite();
        assert!(mem.health_check().await);
    }

    #[tokio::test]
    async fn sqlite_store_and_get() {
        let (_tmp, mem) = temp_sqlite();
        mem.store("user_lang", "Prefers Rust", MemoryCategory::Core, None)
            .await
            .unwrap();

        let entry = mem.get("user_lang").await.unwrap();
        assert!(entry.is_some());
        let entry = entry.unwrap();
        assert_eq!(entry.key, "user_lang");
        assert_eq!(entry.content, "Prefers Rust");
        assert_eq!(entry.category, MemoryCategory::Core);
    }

    #[tokio::test]
    async fn sqlite_store_upsert() {
        let (_tmp, mem) = temp_sqlite();
        mem.store("pref", "likes Rust", MemoryCategory::Core, None)
            .await
            .unwrap();
        mem.store("pref", "loves Rust", MemoryCategory::Core, None)
            .await
            .unwrap();

        let entry = mem.get("pref").await.unwrap().unwrap();
        assert_eq!(entry.content, "loves Rust");
        assert_eq!(mem.count().await.unwrap(), 1);
    }

    #[tokio::test]
    async fn sqlite_recall_keyword() {
        let (_tmp, mem) = temp_sqlite();
        mem.store("a", "Rust is fast and safe", MemoryCategory::Core, None)
            .await
            .unwrap();
        mem.store("b", "Python is interpreted", MemoryCategory::Core, None)
            .await
            .unwrap();
        mem.store(
            "c",
            "Rust has zero-cost abstractions",
            MemoryCategory::Core,
            None,
        )
        .await
        .unwrap();

        let results = mem
            .recall("Rust", 10, None, None, None, None)
            .await
            .unwrap();
        assert_eq!(results.len(), 2);
        assert!(
            results
                .iter()
                .all(|r| r.content.to_lowercase().contains("rust"))
        );
    }

    #[tokio::test]
    async fn sqlite_recall_multi_keyword() {
        let (_tmp, mem) = temp_sqlite();
        mem.store("a", "Rust is fast", MemoryCategory::Core, None)
            .await
            .unwrap();
        mem.store("b", "Rust is safe and fast", MemoryCategory::Core, None)
            .await
            .unwrap();

        let results = mem
            .recall("fast safe", 10, None, None, None, None)
            .await
            .unwrap();
        assert!(!results.is_empty());
        // Entry with both keywords should score higher
        assert!(results[0].content.contains("safe") && results[0].content.contains("fast"));
    }

    #[tokio::test]
    async fn sqlite_recall_no_match() {
        let (_tmp, mem) = temp_sqlite();
        mem.store("a", "Rust rocks", MemoryCategory::Core, None)
            .await
            .unwrap();
        let results = mem
            .recall("javascript", 10, None, None, None, None)
            .await
            .unwrap();
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn sqlite_forget() {
        let (_tmp, mem) = temp_sqlite();
        mem.store("temp", "temporary data", MemoryCategory::Conversation, None)
            .await
            .unwrap();
        assert_eq!(mem.count().await.unwrap(), 1);

        let removed = mem.forget("temp").await.unwrap();
        assert!(removed);
        assert_eq!(mem.count().await.unwrap(), 0);
    }

    #[tokio::test]
    async fn sqlite_forget_nonexistent() {
        let (_tmp, mem) = temp_sqlite();
        let removed = mem.forget("nope").await.unwrap();
        assert!(!removed);
    }

    #[tokio::test]
    async fn sqlite_list_all() {
        let (_tmp, mem) = temp_sqlite();
        mem.store("a", "one", MemoryCategory::Core, None)
            .await
            .unwrap();
        mem.store("b", "two", MemoryCategory::Daily, None)
            .await
            .unwrap();
        mem.store("c", "three", MemoryCategory::Conversation, None)
            .await
            .unwrap();

        let all = mem.list(None, None).await.unwrap();
        assert_eq!(all.len(), 3);
    }

    #[tokio::test]
    async fn sqlite_list_by_category() {
        let (_tmp, mem) = temp_sqlite();
        mem.store("a", "core1", MemoryCategory::Core, None)
            .await
            .unwrap();
        mem.store("b", "core2", MemoryCategory::Core, None)
            .await
            .unwrap();
        mem.store("c", "daily1", MemoryCategory::Daily, None)
            .await
            .unwrap();

        let core = mem.list(Some(&MemoryCategory::Core), None).await.unwrap();
        assert_eq!(core.len(), 2);

        let daily = mem.list(Some(&MemoryCategory::Daily), None).await.unwrap();
        assert_eq!(daily.len(), 1);
    }

    #[tokio::test]
    async fn sqlite_count_empty() {
        let (_tmp, mem) = temp_sqlite();
        assert_eq!(mem.count().await.unwrap(), 0);
    }

    #[tokio::test]
    async fn sqlite_get_nonexistent() {
        let (_tmp, mem) = temp_sqlite();
        assert!(mem.get("nope").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn sqlite_db_persists() {
        let tmp = TempDir::new().unwrap();

        {
            let mem = SqliteMemory::new(tmp.path()).unwrap();
            mem.store("persist", "I survive restarts", MemoryCategory::Core, None)
                .await
                .unwrap();
        }

        // Reopen
        let mem2 = SqliteMemory::new(tmp.path()).unwrap();
        let entry = mem2.get("persist").await.unwrap();
        assert!(entry.is_some());
        assert_eq!(entry.unwrap().content, "I survive restarts");
    }

    #[tokio::test]
    async fn sqlite_category_roundtrip() {
        let (_tmp, mem) = temp_sqlite();
        let categories = [
            MemoryCategory::Core,
            MemoryCategory::Daily,
            MemoryCategory::Conversation,
            MemoryCategory::Custom("project".into()),
        ];

        for (i, cat) in categories.iter().enumerate() {
            mem.store(&format!("k{i}"), &format!("v{i}"), cat.clone(), None)
                .await
                .unwrap();
        }

        for (i, cat) in categories.iter().enumerate() {
            let entry = mem.get(&format!("k{i}")).await.unwrap().unwrap();
            assert_eq!(&entry.category, cat);
        }
    }

    // ── FTS5 search tests ────────────────────────────────────────

    #[tokio::test]
    async fn fts5_bm25_ranking() {
        let (_tmp, mem) = temp_sqlite();
        mem.store(
            "a",
            "Rust is a systems programming language",
            MemoryCategory::Core,
            None,
        )
        .await
        .unwrap();
        mem.store(
            "b",
            "Python is great for scripting",
            MemoryCategory::Core,
            None,
        )
        .await
        .unwrap();
        mem.store(
            "c",
            "Rust and Rust and Rust everywhere",
            MemoryCategory::Core,
            None,
        )
        .await
        .unwrap();

        let results = mem
            .recall("Rust", 10, None, None, None, None)
            .await
            .unwrap();
        assert!(results.len() >= 2);
        // All results should contain "Rust"
        for r in &results {
            assert!(
                r.content.to_lowercase().contains("rust"),
                "Expected 'rust' in: {}",
                r.content
            );
        }
    }

    #[tokio::test]
    async fn fts5_multi_word_query() {
        let (_tmp, mem) = temp_sqlite();
        mem.store("a", "The quick brown fox jumps", MemoryCategory::Core, None)
            .await
            .unwrap();
        mem.store("b", "A lazy dog sleeps", MemoryCategory::Core, None)
            .await
            .unwrap();
        mem.store("c", "The quick dog runs fast", MemoryCategory::Core, None)
            .await
            .unwrap();

        let results = mem
            .recall("quick dog", 10, None, None, None, None)
            .await
            .unwrap();
        assert!(!results.is_empty());
        // "The quick dog runs fast" matches both terms
        assert!(results[0].content.contains("quick"));
    }

    #[tokio::test]
    async fn recall_empty_query_returns_recent_entries() {
        let (_tmp, mem) = temp_sqlite();
        mem.store("a", "data", MemoryCategory::Core, None)
            .await
            .unwrap();
        // Empty query = time-only mode: returns recent entries
        let results = mem.recall("", 10, None, None, None, None).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].key, "a");
    }

    #[tokio::test]
    async fn recall_whitespace_query_returns_recent_entries() {
        let (_tmp, mem) = temp_sqlite();
        mem.store("a", "data", MemoryCategory::Core, None)
            .await
            .unwrap();
        // Whitespace-only query = time-only mode: returns recent entries
        let results = mem.recall("   ", 10, None, None, None, None).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].key, "a");
    }

    // ── Embedding cache tests ────────────────────────────────────

    #[test]
    fn content_hash_deterministic() {
        let h1 = SqliteMemory::content_hash("hello world");
        let h2 = SqliteMemory::content_hash("hello world");
        assert_eq!(h1, h2);
    }

    #[test]
    fn content_hash_different_inputs() {
        let h1 = SqliteMemory::content_hash("hello");
        let h2 = SqliteMemory::content_hash("world");
        assert_ne!(h1, h2);
    }

    // ── Schema tests ─────────────────────────────────────────────

    #[tokio::test]
    async fn schema_has_fts5_table() {
        let (_tmp, mem) = temp_sqlite();
        let conn = mem.conn.lock();
        // FTS5 table should exist
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='memories_fts'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn schema_has_embedding_cache() {
        let (_tmp, mem) = temp_sqlite();
        let conn = mem.conn.lock();
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='embedding_cache'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn schema_memories_has_embedding_column() {
        let (_tmp, mem) = temp_sqlite();
        let conn = mem.conn.lock();
        // Check that embedding column exists by querying it
        let result = conn.execute_batch("SELECT embedding FROM memories LIMIT 0");
        assert!(result.is_ok());
    }

    // ── FTS5 sync trigger tests ──────────────────────────────────

    #[tokio::test]
    async fn fts5_syncs_on_insert() {
        let (_tmp, mem) = temp_sqlite();
        mem.store(
            "test_key",
            "unique_searchterm_xyz",
            MemoryCategory::Core,
            None,
        )
        .await
        .unwrap();

        let conn = mem.conn.lock();
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM memories_fts WHERE memories_fts MATCH '\"unique_searchterm_xyz\"'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn fts5_syncs_on_delete() {
        let (_tmp, mem) = temp_sqlite();
        mem.store(
            "del_key",
            "deletable_content_abc",
            MemoryCategory::Core,
            None,
        )
        .await
        .unwrap();
        mem.forget("del_key").await.unwrap();

        let conn = mem.conn.lock();
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM memories_fts WHERE memories_fts MATCH '\"deletable_content_abc\"'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn fts5_syncs_on_update() {
        let (_tmp, mem) = temp_sqlite();
        mem.store(
            "upd_key",
            "original_content_111",
            MemoryCategory::Core,
            None,
        )
        .await
        .unwrap();
        mem.store("upd_key", "updated_content_222", MemoryCategory::Core, None)
            .await
            .unwrap();

        let conn = mem.conn.lock();
        // Old content should not be findable
        let old: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM memories_fts WHERE memories_fts MATCH '\"original_content_111\"'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(old, 0);

        // New content should be findable
        let new: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM memories_fts WHERE memories_fts MATCH '\"updated_content_222\"'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(new, 1);
    }

    // ── Open timeout tests ────────────────────────────────────────

    #[test]
    fn open_with_timeout_succeeds_when_fast() {
        let tmp = TempDir::new().unwrap();
        let embedder = Arc::new(super::super::embeddings::NoopEmbedding);
        let mem = SqliteMemory::with_embedder(
            tmp.path(),
            embedder,
            0.7,
            0.3,
            1000,
            Some(5),
            SearchMode::default(),
            MergeStrategy::default(),
            false,
        );
        assert!(
            mem.is_ok(),
            "open with 5s timeout should succeed on fast path"
        );
        assert_eq!(mem.unwrap().name(), "sqlite");
    }

    #[tokio::test]
    async fn open_with_timeout_store_recall_unchanged() {
        let tmp = TempDir::new().unwrap();
        let mem = SqliteMemory::with_embedder(
            tmp.path(),
            Arc::new(super::super::embeddings::NoopEmbedding),
            0.7,
            0.3,
            1000,
            Some(2),
            SearchMode::default(),
            MergeStrategy::default(),
            false,
        )
        .unwrap();
        mem.store(
            "timeout_key",
            "value with timeout",
            MemoryCategory::Core,
            None,
        )
        .await
        .unwrap();
        let entry = mem.get("timeout_key").await.unwrap().unwrap();
        assert_eq!(entry.content, "value with timeout");
    }

    // ── With-embedder constructor test ───────────────────────────

    #[test]
    fn with_embedder_noop() {
        let tmp = TempDir::new().unwrap();
        let embedder = Arc::new(super::super::embeddings::NoopEmbedding);
        let mem = SqliteMemory::with_embedder(
            tmp.path(),
            embedder,
            0.7,
            0.3,
            1000,
            None,
            SearchMode::default(),
            MergeStrategy::default(),
            false,
        );
        assert!(mem.is_ok());
        assert_eq!(mem.unwrap().name(), "sqlite");
    }

    // ── Reindex test ─────────────────────────────────────────────

    #[tokio::test]
    async fn reindex_rebuilds_fts() {
        let (_tmp, mem) = temp_sqlite();
        mem.store("r1", "reindex test alpha", MemoryCategory::Core, None)
            .await
            .unwrap();
        mem.store("r2", "reindex test beta", MemoryCategory::Core, None)
            .await
            .unwrap();

        // Reindex should succeed (noop embedder → 0 re-embedded)
        let count = mem.reindex().await.unwrap();
        assert_eq!(count, 0);

        // FTS should still work after rebuild
        let results = mem
            .recall("reindex", 10, None, None, None, None)
            .await
            .unwrap();
        assert_eq!(results.len(), 2);
    }

    // ── Recall limit test ────────────────────────────────────────

    #[tokio::test]
    async fn recall_respects_limit() {
        let (_tmp, mem) = temp_sqlite();
        for i in 0..20 {
            mem.store(
                &format!("k{i}"),
                &format!("common keyword item {i}"),
                MemoryCategory::Core,
                None,
            )
            .await
            .unwrap();
        }

        let results = mem
            .recall("common keyword", 5, None, None, None, None)
            .await
            .unwrap();
        assert!(results.len() <= 5);
    }

    // ── Score presence test ──────────────────────────────────────

    #[tokio::test]
    async fn recall_results_have_scores() {
        let (_tmp, mem) = temp_sqlite();
        mem.store("s1", "scored result test", MemoryCategory::Core, None)
            .await
            .unwrap();

        let results = mem
            .recall("scored", 10, None, None, None, None)
            .await
            .unwrap();
        assert!(!results.is_empty());
        for r in &results {
            assert!(r.score.is_some(), "Expected score on result: {:?}", r.key);
        }
    }

    // ── Edge cases: FTS5 special characters ──────────────────────

    #[tokio::test]
    async fn recall_with_quotes_in_query() {
        let (_tmp, mem) = temp_sqlite();
        mem.store("q1", "He said hello world", MemoryCategory::Core, None)
            .await
            .unwrap();
        // Quotes in query should not crash FTS5
        let results = mem
            .recall("\"hello\"", 10, None, None, None, None)
            .await
            .unwrap();
        // May or may not match depending on FTS5 escaping, but must not error
        assert!(results.len() <= 10);
    }

    #[tokio::test]
    async fn recall_with_asterisk_in_query() {
        let (_tmp, mem) = temp_sqlite();
        mem.store("a1", "wildcard test content", MemoryCategory::Core, None)
            .await
            .unwrap();
        let results = mem
            .recall("wild*", 10, None, None, None, None)
            .await
            .unwrap();
        assert!(results.len() <= 10);
    }

    #[tokio::test]
    async fn recall_with_parentheses_in_query() {
        let (_tmp, mem) = temp_sqlite();
        mem.store("p1", "function call test", MemoryCategory::Core, None)
            .await
            .unwrap();
        let results = mem
            .recall("function()", 10, None, None, None, None)
            .await
            .unwrap();
        assert!(results.len() <= 10);
    }

    #[tokio::test]
    async fn recall_with_sql_injection_attempt() {
        let (_tmp, mem) = temp_sqlite();
        mem.store("safe", "normal content", MemoryCategory::Core, None)
            .await
            .unwrap();
        // Should not crash or leak data
        let results = mem
            .recall("'; DROP TABLE memories; --", 10, None, None, None, None)
            .await
            .unwrap();
        assert!(results.len() <= 10);
        // Table should still exist
        assert_eq!(mem.count().await.unwrap(), 1);
    }

    // ── Edge cases: store ────────────────────────────────────────

    #[tokio::test]
    async fn store_empty_content() {
        let (_tmp, mem) = temp_sqlite();
        mem.store("empty", "", MemoryCategory::Core, None)
            .await
            .unwrap();
        let entry = mem.get("empty").await.unwrap().unwrap();
        assert_eq!(entry.content, "");
    }

    #[tokio::test]
    async fn store_empty_key() {
        let (_tmp, mem) = temp_sqlite();
        mem.store("", "content for empty key", MemoryCategory::Core, None)
            .await
            .unwrap();
        let entry = mem.get("").await.unwrap().unwrap();
        assert_eq!(entry.content, "content for empty key");
    }

    #[tokio::test]
    async fn store_very_long_content() {
        let (_tmp, mem) = temp_sqlite();
        let long_content = "x".repeat(100_000);
        mem.store("long", &long_content, MemoryCategory::Core, None)
            .await
            .unwrap();
        let entry = mem.get("long").await.unwrap().unwrap();
        assert_eq!(entry.content.len(), 100_000);
    }

    #[tokio::test]
    async fn store_unicode_and_emoji() {
        let (_tmp, mem) = temp_sqlite();
        mem.store(
            "emoji_key_🦀",
            "こんにちは 🚀 Ñoño",
            MemoryCategory::Core,
            None,
        )
        .await
        .unwrap();
        let entry = mem.get("emoji_key_🦀").await.unwrap().unwrap();
        assert_eq!(entry.content, "こんにちは 🚀 Ñoño");
    }

    #[tokio::test]
    async fn store_content_with_newlines_and_tabs() {
        let (_tmp, mem) = temp_sqlite();
        let content = "line1\nline2\ttab\rcarriage\n\nnewparagraph";
        mem.store("whitespace", content, MemoryCategory::Core, None)
            .await
            .unwrap();
        let entry = mem.get("whitespace").await.unwrap().unwrap();
        assert_eq!(entry.content, content);
    }

    // ── Edge cases: recall ───────────────────────────────────────

    #[tokio::test]
    async fn recall_single_character_query() {
        let (_tmp, mem) = temp_sqlite();
        mem.store("a", "x marks the spot", MemoryCategory::Core, None)
            .await
            .unwrap();
        // Single char may not match FTS5 but LIKE fallback should work
        let results = mem.recall("x", 10, None, None, None, None).await.unwrap();
        // Should not crash; may or may not find results
        assert!(results.len() <= 10);
    }

    #[tokio::test]
    async fn recall_limit_zero() {
        let (_tmp, mem) = temp_sqlite();
        mem.store("a", "some content", MemoryCategory::Core, None)
            .await
            .unwrap();
        let results = mem.recall("some", 0, None, None, None, None).await.unwrap();
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn recall_limit_one() {
        let (_tmp, mem) = temp_sqlite();
        mem.store("a", "matching content alpha", MemoryCategory::Core, None)
            .await
            .unwrap();
        mem.store("b", "matching content beta", MemoryCategory::Core, None)
            .await
            .unwrap();
        let results = mem
            .recall("matching content", 1, None, None, None, None)
            .await
            .unwrap();
        assert_eq!(results.len(), 1);
    }

    #[tokio::test]
    async fn recall_matches_by_key_not_just_content() {
        let (_tmp, mem) = temp_sqlite();
        mem.store(
            "rust_preferences",
            "User likes systems programming",
            MemoryCategory::Core,
            None,
        )
        .await
        .unwrap();
        // "rust" appears in key but not content — LIKE fallback checks key too
        let results = mem
            .recall("rust", 10, None, None, None, None)
            .await
            .unwrap();
        assert!(!results.is_empty(), "Should match by key");
    }

    #[tokio::test]
    async fn recall_unicode_query() {
        let (_tmp, mem) = temp_sqlite();
        mem.store("jp", "日本語のテスト", MemoryCategory::Core, None)
            .await
            .unwrap();
        let results = mem
            .recall("日本語", 10, None, None, None, None)
            .await
            .unwrap();
        assert!(!results.is_empty());
    }

    // ── Edge cases: schema idempotency ───────────────────────────

    #[tokio::test]
    async fn schema_idempotent_reopen() {
        let tmp = TempDir::new().unwrap();
        {
            let mem = SqliteMemory::new(tmp.path()).unwrap();
            mem.store("k1", "v1", MemoryCategory::Core, None)
                .await
                .unwrap();
        }
        // Open again — init_schema runs again on existing DB
        let mem2 = SqliteMemory::new(tmp.path()).unwrap();
        let entry = mem2.get("k1").await.unwrap();
        assert!(entry.is_some());
        assert_eq!(entry.unwrap().content, "v1");
        // Store more data — should work fine
        mem2.store("k2", "v2", MemoryCategory::Daily, None)
            .await
            .unwrap();
        assert_eq!(mem2.count().await.unwrap(), 2);
    }

    #[tokio::test]
    async fn schema_triple_open() {
        let tmp = TempDir::new().unwrap();
        let _m1 = SqliteMemory::new(tmp.path()).unwrap();
        let _m2 = SqliteMemory::new(tmp.path()).unwrap();
        let m3 = SqliteMemory::new(tmp.path()).unwrap();
        assert!(m3.health_check().await);
    }

    // ── Edge cases: forget + FTS5 consistency ────────────────────

    #[tokio::test]
    async fn forget_then_recall_no_ghost_results() {
        let (_tmp, mem) = temp_sqlite();
        mem.store(
            "ghost",
            "phantom memory content",
            MemoryCategory::Core,
            None,
        )
        .await
        .unwrap();
        mem.forget("ghost").await.unwrap();
        let results = mem
            .recall("phantom memory", 10, None, None, None, None)
            .await
            .unwrap();
        assert!(
            results.is_empty(),
            "Deleted memory should not appear in recall"
        );
    }

    #[tokio::test]
    async fn forget_and_re_store_same_key() {
        let (_tmp, mem) = temp_sqlite();
        mem.store("cycle", "version 1", MemoryCategory::Core, None)
            .await
            .unwrap();
        mem.forget("cycle").await.unwrap();
        mem.store("cycle", "version 2", MemoryCategory::Core, None)
            .await
            .unwrap();
        let entry = mem.get("cycle").await.unwrap().unwrap();
        assert_eq!(entry.content, "version 2");
        assert_eq!(mem.count().await.unwrap(), 1);
    }

    // ── Edge cases: reindex ──────────────────────────────────────

    #[tokio::test]
    async fn reindex_empty_db() {
        let (_tmp, mem) = temp_sqlite();
        let count = mem.reindex().await.unwrap();
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn reindex_twice_is_safe() {
        let (_tmp, mem) = temp_sqlite();
        mem.store("r1", "reindex data", MemoryCategory::Core, None)
            .await
            .unwrap();
        mem.reindex().await.unwrap();
        let count = mem.reindex().await.unwrap();
        assert_eq!(count, 0); // Noop embedder → nothing to re-embed
        // Data should still be intact
        let results = mem
            .recall("reindex", 10, None, None, None, None)
            .await
            .unwrap();
        assert_eq!(results.len(), 1);
    }

    // ── Edge cases: content_hash ─────────────────────────────────

    #[test]
    fn content_hash_empty_string() {
        let h = SqliteMemory::content_hash("");
        assert!(!h.is_empty());
        assert_eq!(h.len(), 16); // 16 hex chars
    }

    #[test]
    fn content_hash_unicode() {
        let h1 = SqliteMemory::content_hash("🦀");
        let h2 = SqliteMemory::content_hash("🦀");
        assert_eq!(h1, h2);
        let h3 = SqliteMemory::content_hash("🚀");
        assert_ne!(h1, h3);
    }

    #[test]
    fn content_hash_long_input() {
        let long = "a".repeat(1_000_000);
        let h = SqliteMemory::content_hash(&long);
        assert_eq!(h.len(), 16);
    }

    // ── Edge cases: category helpers ─────────────────────────────

    #[test]
    fn category_roundtrip_custom_with_spaces() {
        let cat = MemoryCategory::Custom("my custom category".into());
        let s = SqliteMemory::category_to_str(&cat);
        assert_eq!(s, "my custom category");
        let back = SqliteMemory::str_to_category(&s);
        assert_eq!(back, cat);
    }

    #[test]
    fn category_roundtrip_empty_custom() {
        let cat = MemoryCategory::Custom(String::new());
        let s = SqliteMemory::category_to_str(&cat);
        assert_eq!(s, "");
        let back = SqliteMemory::str_to_category(&s);
        assert_eq!(back, MemoryCategory::Custom(String::new()));
    }

    // ── Edge cases: list ─────────────────────────────────────────

    #[tokio::test]
    async fn list_custom_category() {
        let (_tmp, mem) = temp_sqlite();
        mem.store(
            "c1",
            "custom1",
            MemoryCategory::Custom("project".into()),
            None,
        )
        .await
        .unwrap();
        mem.store(
            "c2",
            "custom2",
            MemoryCategory::Custom("project".into()),
            None,
        )
        .await
        .unwrap();
        mem.store("c3", "other", MemoryCategory::Core, None)
            .await
            .unwrap();

        let project = mem
            .list(Some(&MemoryCategory::Custom("project".into())), None)
            .await
            .unwrap();
        assert_eq!(project.len(), 2);
    }

    #[tokio::test]
    async fn list_empty_db() {
        let (_tmp, mem) = temp_sqlite();
        let all = mem.list(None, None).await.unwrap();
        assert!(all.is_empty());
    }

    // ── Bulk deletion tests ───────────────────────────────────────

    #[tokio::test]
    async fn sqlite_purge_namespace_removes_all_matching_entries() {
        let (_tmp, mem) = temp_sqlite();
        mem.store_with_metadata("a1", "data1", MemoryCategory::Core, None, Some("ns1"), None)
            .await
            .unwrap();
        mem.store_with_metadata("a2", "data2", MemoryCategory::Core, None, Some("ns1"), None)
            .await
            .unwrap();
        mem.store_with_metadata("b1", "data3", MemoryCategory::Core, None, Some("ns2"), None)
            .await
            .unwrap();

        let count = mem.purge_namespace("ns1").await.unwrap();
        assert_eq!(count, 2);
        assert_eq!(mem.count().await.unwrap(), 1);
    }

    #[tokio::test]
    async fn sqlite_purge_namespace_preserves_other_namespaces() {
        let (_tmp, mem) = temp_sqlite();
        mem.store_with_metadata("a1", "data1", MemoryCategory::Core, None, Some("ns1"), None)
            .await
            .unwrap();
        mem.store_with_metadata("b1", "data2", MemoryCategory::Core, None, Some("ns2"), None)
            .await
            .unwrap();
        mem.store("c1", "data3", MemoryCategory::Core, None)
            .await
            .unwrap();
        mem.store("d1", "data4", MemoryCategory::Daily, None)
            .await
            .unwrap();

        let count = mem.purge_namespace("ns1").await.unwrap();
        assert_eq!(count, 1);
        assert_eq!(mem.count().await.unwrap(), 3);

        let remaining = mem.list(None, None).await.unwrap();
        assert!(remaining.iter().all(|e| e.namespace != "ns1"));
    }

    #[tokio::test]
    async fn sqlite_purge_namespace_returns_count() {
        let (_tmp, mem) = temp_sqlite();
        for i in 0..5 {
            mem.store_with_metadata(
                &format!("k{i}"),
                "data",
                MemoryCategory::Core,
                None,
                Some("target"),
                None,
            )
            .await
            .unwrap();
        }

        let count = mem.purge_namespace("target").await.unwrap();
        assert_eq!(count, 5);
    }

    #[tokio::test]
    async fn sqlite_purge_session_removes_all_matching_entries() {
        let (_tmp, mem) = temp_sqlite();
        mem.store("a1", "data1", MemoryCategory::Core, Some("sess-a"))
            .await
            .unwrap();
        mem.store("a2", "data2", MemoryCategory::Core, Some("sess-a"))
            .await
            .unwrap();
        mem.store("b1", "data3", MemoryCategory::Core, Some("sess-b"))
            .await
            .unwrap();

        let count = mem.purge_session("sess-a").await.unwrap();
        assert_eq!(count, 2);
        assert_eq!(mem.count().await.unwrap(), 1);
    }

    #[tokio::test]
    async fn sqlite_purge_session_preserves_other_sessions() {
        let (_tmp, mem) = temp_sqlite();
        mem.store("a1", "data1", MemoryCategory::Core, Some("sess-a"))
            .await
            .unwrap();
        mem.store("b1", "data2", MemoryCategory::Core, Some("sess-b"))
            .await
            .unwrap();
        mem.store("c1", "data3", MemoryCategory::Core, None)
            .await
            .unwrap();

        let count = mem.purge_session("sess-a").await.unwrap();
        assert_eq!(count, 1);
        assert_eq!(mem.count().await.unwrap(), 2);

        let remaining = mem.list(None, None).await.unwrap();
        assert!(
            remaining
                .iter()
                .all(|e| e.session_id.as_deref() != Some("sess-a"))
        );
    }

    #[tokio::test]
    async fn sqlite_purge_session_returns_count() {
        let (_tmp, mem) = temp_sqlite();
        for i in 0..3 {
            mem.store(
                &format!("k{i}"),
                "data",
                MemoryCategory::Core,
                Some("target-sess"),
            )
            .await
            .unwrap();
        }

        let count = mem.purge_session("target-sess").await.unwrap();
        assert_eq!(count, 3);
    }

    #[tokio::test]
    async fn sqlite_purge_namespace_empty_namespace_is_noop() {
        let (_tmp, mem) = temp_sqlite();
        mem.store("a", "data", MemoryCategory::Core, None)
            .await
            .unwrap();

        let count = mem.purge_namespace("").await.unwrap();
        assert_eq!(count, 0);
        assert_eq!(mem.count().await.unwrap(), 1);
    }

    #[tokio::test]
    async fn sqlite_purge_session_empty_session_is_noop() {
        let (_tmp, mem) = temp_sqlite();
        mem.store("a", "data", MemoryCategory::Core, Some("sess"))
            .await
            .unwrap();

        let count = mem.purge_session("").await.unwrap();
        assert_eq!(count, 0);
        assert_eq!(mem.count().await.unwrap(), 1);
    }

    // ── Session isolation ─────────────────────────────────────────

    #[tokio::test]
    async fn store_and_recall_with_session_id() {
        let (_tmp, mem) = temp_sqlite();
        mem.store("k1", "session A fact", MemoryCategory::Core, Some("sess-a"))
            .await
            .unwrap();
        mem.store("k2", "session B fact", MemoryCategory::Core, Some("sess-b"))
            .await
            .unwrap();
        mem.store("k3", "no session fact", MemoryCategory::Core, None)
            .await
            .unwrap();

        // Recall with session-a filter returns only session-a entry
        let results = mem
            .recall("fact", 10, Some("sess-a"), None, None, None)
            .await
            .unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].key, "k1");
        assert_eq!(results[0].session_id.as_deref(), Some("sess-a"));
    }

    #[tokio::test]
    async fn recall_no_session_filter_returns_all() {
        let (_tmp, mem) = temp_sqlite();
        mem.store("k1", "alpha fact", MemoryCategory::Core, Some("sess-a"))
            .await
            .unwrap();
        mem.store("k2", "beta fact", MemoryCategory::Core, Some("sess-b"))
            .await
            .unwrap();
        mem.store("k3", "gamma fact", MemoryCategory::Core, None)
            .await
            .unwrap();

        // Recall without session filter returns all matching entries
        let results = mem
            .recall("fact", 10, None, None, None, None)
            .await
            .unwrap();
        assert_eq!(results.len(), 3);
    }

    #[tokio::test]
    async fn cross_session_recall_isolation() {
        let (_tmp, mem) = temp_sqlite();
        mem.store(
            "secret",
            "session A secret data",
            MemoryCategory::Core,
            Some("sess-a"),
        )
        .await
        .unwrap();

        // Session B cannot see session A data
        let results = mem
            .recall("secret", 10, Some("sess-b"), None, None, None)
            .await
            .unwrap();
        assert!(results.is_empty());

        // Session A can see its own data
        let results = mem
            .recall("secret", 10, Some("sess-a"), None, None, None)
            .await
            .unwrap();
        assert_eq!(results.len(), 1);
    }

    #[tokio::test]
    async fn list_with_session_filter() {
        let (_tmp, mem) = temp_sqlite();
        mem.store("k1", "a1", MemoryCategory::Core, Some("sess-a"))
            .await
            .unwrap();
        mem.store("k2", "a2", MemoryCategory::Conversation, Some("sess-a"))
            .await
            .unwrap();
        mem.store("k3", "b1", MemoryCategory::Core, Some("sess-b"))
            .await
            .unwrap();
        mem.store("k4", "none1", MemoryCategory::Core, None)
            .await
            .unwrap();

        // List with session-a filter
        let results = mem.list(None, Some("sess-a")).await.unwrap();
        assert_eq!(results.len(), 2);
        assert!(
            results
                .iter()
                .all(|e| e.session_id.as_deref() == Some("sess-a"))
        );

        // List with session-a + category filter
        let results = mem
            .list(Some(&MemoryCategory::Core), Some("sess-a"))
            .await
            .unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].key, "k1");
    }

    #[tokio::test]
    async fn schema_migration_idempotent_on_reopen() {
        let tmp = TempDir::new().unwrap();

        // First open: creates schema + migration
        {
            let mem = SqliteMemory::new(tmp.path()).unwrap();
            mem.store("k1", "before reopen", MemoryCategory::Core, Some("sess-x"))
                .await
                .unwrap();
        }

        // Second open: migration runs again but is idempotent
        {
            let mem = SqliteMemory::new(tmp.path()).unwrap();
            let results = mem
                .recall("reopen", 10, Some("sess-x"), None, None, None)
                .await
                .unwrap();
            assert_eq!(results.len(), 1);
            assert_eq!(results[0].key, "k1");
            assert_eq!(results[0].session_id.as_deref(), Some("sess-x"));
        }
    }

    // ── §4.1 Concurrent write contention tests ──────────────

    #[tokio::test]
    async fn sqlite_concurrent_writes_no_data_loss() {
        let (_tmp, mem) = temp_sqlite();
        let mem = std::sync::Arc::new(mem);

        let mut handles = Vec::new();
        for i in 0..10 {
            let mem = std::sync::Arc::clone(&mem);
            handles.push(tokio::spawn(async move {
                mem.store(
                    &format!("concurrent_key_{i}"),
                    &format!("value_{i}"),
                    MemoryCategory::Core,
                    None,
                )
                .await
                .unwrap();
            }));
        }

        for handle in handles {
            handle.await.unwrap();
        }

        let count = mem.count().await.unwrap();
        assert_eq!(
            count, 10,
            "all 10 concurrent writes must succeed without data loss"
        );
    }

    #[tokio::test]
    async fn sqlite_concurrent_read_write_no_panic() {
        let (_tmp, mem) = temp_sqlite();
        let mem = std::sync::Arc::new(mem);

        // Pre-populate
        mem.store("shared_key", "initial", MemoryCategory::Core, None)
            .await
            .unwrap();

        let mut handles = Vec::new();

        // Concurrent reads
        for _ in 0..5 {
            let mem = std::sync::Arc::clone(&mem);
            handles.push(tokio::spawn(async move {
                let _ = mem.get("shared_key").await.unwrap();
            }));
        }

        // Concurrent writes
        for i in 0..5 {
            let mem = std::sync::Arc::clone(&mem);
            handles.push(tokio::spawn(async move {
                mem.store(
                    &format!("key_{i}"),
                    &format!("val_{i}"),
                    MemoryCategory::Core,
                    None,
                )
                .await
                .unwrap();
            }));
        }

        for handle in handles {
            handle.await.unwrap();
        }

        // Should have 6 total entries (1 pre-existing + 5 new)
        assert_eq!(mem.count().await.unwrap(), 6);
    }

    // ── Export (GDPR Art. 20) tests ─────────────────────────

    #[tokio::test]
    async fn export_no_filter_returns_all_entries() {
        let (_tmp, mem) = temp_sqlite();
        mem.store("a", "one", MemoryCategory::Core, None)
            .await
            .unwrap();
        mem.store("b", "two", MemoryCategory::Daily, None)
            .await
            .unwrap();
        mem.store("c", "three", MemoryCategory::Conversation, None)
            .await
            .unwrap();

        let filter = ExportFilter::default();
        let results = mem.export(&filter).await.unwrap();
        assert_eq!(results.len(), 3);
    }

    #[tokio::test]
    async fn export_with_namespace_filter() {
        let (_tmp, mem) = temp_sqlite();
        mem.store_with_metadata(
            "a",
            "ns1 data",
            MemoryCategory::Core,
            None,
            Some("ns1"),
            None,
        )
        .await
        .unwrap();
        mem.store_with_metadata(
            "b",
            "ns2 data",
            MemoryCategory::Core,
            None,
            Some("ns2"),
            None,
        )
        .await
        .unwrap();

        let filter = ExportFilter {
            namespace: Some("ns1".into()),
            ..Default::default()
        };
        let results = mem.export(&filter).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].namespace, "ns1");
    }

    #[tokio::test]
    async fn export_with_session_id_filter() {
        let (_tmp, mem) = temp_sqlite();
        mem.store("a", "sess-a data", MemoryCategory::Core, Some("sess-a"))
            .await
            .unwrap();
        mem.store("b", "sess-b data", MemoryCategory::Core, Some("sess-b"))
            .await
            .unwrap();

        let filter = ExportFilter {
            session_id: Some("sess-a".into()),
            ..Default::default()
        };
        let results = mem.export(&filter).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].key, "a");
    }

    #[tokio::test]
    async fn export_with_category_filter() {
        let (_tmp, mem) = temp_sqlite();
        mem.store("a", "core data", MemoryCategory::Core, None)
            .await
            .unwrap();
        mem.store("b", "daily data", MemoryCategory::Daily, None)
            .await
            .unwrap();

        let filter = ExportFilter {
            category: Some(MemoryCategory::Core),
            ..Default::default()
        };
        let results = mem.export(&filter).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].category, MemoryCategory::Core);
    }

    #[tokio::test]
    async fn export_with_time_range() {
        let (_tmp, mem) = temp_sqlite();
        // Store entries — created_at is set to Local::now() by store()
        mem.store("a", "old data", MemoryCategory::Core, None)
            .await
            .unwrap();
        mem.store("b", "new data", MemoryCategory::Core, None)
            .await
            .unwrap();

        // Export with a time range that covers everything
        let filter = ExportFilter {
            since: Some("2000-01-01T00:00:00Z".into()),
            until: Some("2099-12-31T23:59:59Z".into()),
            ..Default::default()
        };
        let results = mem.export(&filter).await.unwrap();
        assert_eq!(results.len(), 2);

        // Export with a time range in the far future (no results)
        let filter = ExportFilter {
            since: Some("2099-01-01T00:00:00Z".into()),
            ..Default::default()
        };
        let results = mem.export(&filter).await.unwrap();
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn export_with_combined_filters() {
        let (_tmp, mem) = temp_sqlite();
        mem.store_with_metadata(
            "a",
            "match",
            MemoryCategory::Core,
            Some("sess-a"),
            Some("ns1"),
            None,
        )
        .await
        .unwrap();
        mem.store_with_metadata(
            "b",
            "no match ns",
            MemoryCategory::Core,
            Some("sess-a"),
            Some("ns2"),
            None,
        )
        .await
        .unwrap();
        mem.store_with_metadata(
            "c",
            "no match sess",
            MemoryCategory::Core,
            None,
            Some("ns1"),
            None,
        )
        .await
        .unwrap();

        let filter = ExportFilter {
            namespace: Some("ns1".into()),
            session_id: Some("sess-a".into()),
            category: Some(MemoryCategory::Core),
            since: Some("2000-01-01T00:00:00Z".into()),
            until: Some("2099-12-31T23:59:59Z".into()),
        };
        let results = mem.export(&filter).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].key, "a");
    }

    #[tokio::test]
    async fn export_empty_database_returns_empty_vec() {
        let (_tmp, mem) = temp_sqlite();
        let filter = ExportFilter::default();
        let results = mem.export(&filter).await.unwrap();
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn export_ordering_is_chronological() {
        let (_tmp, mem) = temp_sqlite();
        mem.store("first", "data1", MemoryCategory::Core, None)
            .await
            .unwrap();
        // Small delay to ensure different timestamps
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        mem.store("second", "data2", MemoryCategory::Core, None)
            .await
            .unwrap();

        let filter = ExportFilter::default();
        let results = mem.export(&filter).await.unwrap();
        assert_eq!(results.len(), 2);
        assert!(
            results[0].timestamp <= results[1].timestamp,
            "Export must be ordered by created_at ASC"
        );
    }

    #[tokio::test]
    async fn export_preserves_field_integrity() {
        let (_tmp, mem) = temp_sqlite();
        mem.store_with_metadata(
            "roundtrip_key",
            "roundtrip content",
            MemoryCategory::Custom("custom_cat".into()),
            Some("sess-rt"),
            Some("ns-rt"),
            Some(0.9),
        )
        .await
        .unwrap();

        let filter = ExportFilter::default();
        let results = mem.export(&filter).await.unwrap();
        assert_eq!(results.len(), 1);
        let e = &results[0];
        assert_eq!(e.key, "roundtrip_key");
        assert_eq!(e.content, "roundtrip content");
        assert_eq!(e.category, MemoryCategory::Custom("custom_cat".into()));
        assert_eq!(e.session_id.as_deref(), Some("sess-rt"));
        assert_eq!(e.namespace, "ns-rt");
        assert_eq!(e.importance, Some(0.9));
    }

    // ── §4.2 Reindex / corruption recovery tests ────────────

    #[tokio::test]
    async fn sqlite_reindex_preserves_data() {
        let (_tmp, mem) = temp_sqlite();
        mem.store("a", "Rust is fast", MemoryCategory::Core, None)
            .await
            .unwrap();
        mem.store("b", "Python is interpreted", MemoryCategory::Core, None)
            .await
            .unwrap();

        mem.reindex().await.unwrap();

        let count = mem.count().await.unwrap();
        assert_eq!(count, 2, "reindex must preserve all entries");

        let entry = mem.get("a").await.unwrap();
        assert!(entry.is_some());
        assert_eq!(entry.unwrap().content, "Rust is fast");
    }

    #[tokio::test]
    async fn sqlite_reindex_idempotent() {
        let (_tmp, mem) = temp_sqlite();
        mem.store("x", "test data", MemoryCategory::Core, None)
            .await
            .unwrap();

        // Multiple reindex calls should be safe
        mem.reindex().await.unwrap();
        mem.reindex().await.unwrap();
        mem.reindex().await.unwrap();

        assert_eq!(mem.count().await.unwrap(), 1);
    }

    // ── SearchMode tests ─────────────────────────────────────────

    #[tokio::test]
    async fn search_mode_bm25_only() {
        let tmp = TempDir::new().unwrap();
        let mem = SqliteMemory::with_embedder(
            tmp.path(),
            Arc::new(super::super::embeddings::NoopEmbedding),
            0.7,
            0.3,
            1000,
            None,
            SearchMode::Bm25,
            MergeStrategy::default(),
            false,
        )
        .unwrap();
        mem.store(
            "lang",
            "User prefers Rust programming",
            MemoryCategory::Core,
            None,
        )
        .await
        .unwrap();
        mem.store("food", "User likes pizza", MemoryCategory::Core, None)
            .await
            .unwrap();

        let results = mem
            .recall("Rust", 10, None, None, None, None)
            .await
            .unwrap();
        assert!(!results.is_empty(), "BM25 mode should find keyword matches");
        assert!(
            results.iter().any(|e| e.content.contains("Rust")),
            "BM25 should match on keyword 'Rust'"
        );
    }

    #[tokio::test]
    async fn search_mode_embedding_only() {
        let tmp = TempDir::new().unwrap();
        // NoopEmbedding has no vectors; embedding-only mode must return no matches.
        let mem = SqliteMemory::with_embedder(
            tmp.path(),
            Arc::new(super::super::embeddings::NoopEmbedding),
            0.7,
            0.3,
            1000,
            None,
            SearchMode::Embedding,
            MergeStrategy::default(),
            false,
        )
        .unwrap();
        mem.store(
            "lang",
            "User prefers Rust programming",
            MemoryCategory::Core,
            None,
        )
        .await
        .unwrap();

        // With NoopEmbedding, vector search returns empty, and FTS is skipped.
        // Lexical fallback would violate the requested search mode.
        let results = mem
            .recall("Rust", 10, None, None, None, None)
            .await
            .unwrap();
        assert!(
            results.is_empty(),
            "Embedding mode with noop must not silently return lexical results"
        );
    }

    #[tokio::test]
    async fn search_mode_hybrid_default() {
        let tmp = TempDir::new().unwrap();
        let mem = SqliteMemory::new(tmp.path()).unwrap();
        // Default search mode should be Hybrid
        assert_eq!(mem.search_mode, SearchMode::Hybrid);

        mem.store(
            "lang",
            "User prefers Rust programming",
            MemoryCategory::Core,
            None,
        )
        .await
        .unwrap();

        let results = mem
            .recall("Rust", 10, None, None, None, None)
            .await
            .unwrap();
        assert!(!results.is_empty(), "Hybrid mode should find results");
    }
}
