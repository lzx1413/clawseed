//! Integration tests for NamespacedMemory with SqliteMemory backend.
//!
//! The unit tests in namespaced.rs only use NoneMemory (no-op). These tests
//! verify that namespace isolation works with a real SQLite backend.

use clawseed_api::memory_traits::{
    ExportFilter, Memory, MemoryCategory, MemoryQuery, MemoryScope, SearchMode,
};
use clawseed_memory::namespaced::NamespacedMemory;
use clawseed_memory::sqlite::SqliteMemory;
use std::sync::Arc;

fn make_namespaced(namespace: &str) -> (tempfile::TempDir, NamespacedMemory) {
    let tmp = tempfile::TempDir::new().unwrap();
    let sqlite = SqliteMemory::new(tmp.path()).unwrap();
    let namespaced = NamespacedMemory::new(Arc::new(sqlite), namespace.to_string());
    (tmp, namespaced)
}

#[tokio::test]
async fn namespaced_sqlite_store_and_get() {
    let (_tmp, mem) = make_namespaced("agent-1");

    mem.store("pref", "Likes Rust", MemoryCategory::Core, None)
        .await
        .unwrap();

    let entry = mem.get("pref").await.unwrap();
    assert!(entry.is_some(), "stored entry should be retrievable");
    assert_eq!(entry.unwrap().content, "Likes Rust");
}

#[tokio::test]
async fn namespaced_sqlite_recall_within_namespace() {
    let (_tmp, mem) = make_namespaced("agent-1");

    mem.store("k1", "Rust programming", MemoryCategory::Core, None)
        .await
        .unwrap();
    mem.store("k2", "Python scripting", MemoryCategory::Core, None)
        .await
        .unwrap();

    let results = mem
        .recall("Rust", 10, None, None, None, None)
        .await
        .unwrap();
    assert!(
        !results.is_empty(),
        "recall should find entries in namespace"
    );
    assert!(
        results.iter().any(|e| e.content.contains("Rust")),
        "recall should find Rust-related entry"
    );
}

#[tokio::test]
async fn namespaced_sqlite_isolation_between_namespaces() {
    let tmp = tempfile::TempDir::new().unwrap();
    let sqlite = Arc::new(SqliteMemory::new(tmp.path()).unwrap());

    let ns_a = NamespacedMemory::new(sqlite.clone(), "agent-a".to_string());
    let ns_b = NamespacedMemory::new(sqlite.clone(), "agent-b".to_string());

    ns_a.store("shared_key", "Agent A secret", MemoryCategory::Core, None)
        .await
        .unwrap();
    ns_b.store("shared_key", "Agent B secret", MemoryCategory::Core, None)
        .await
        .unwrap();

    // Each namespace should only see its own data
    let a_entry = ns_a.get("shared_key").await.unwrap().unwrap();
    assert_eq!(a_entry.content, "Agent A secret");
    assert_eq!(a_entry.namespace, "agent-a");

    let b_entry = ns_b.get("shared_key").await.unwrap().unwrap();
    assert_eq!(b_entry.content, "Agent B secret");
    assert_eq!(b_entry.namespace, "agent-b");

    // Recall should be isolated
    let a_results = ns_a
        .recall("secret", 10, None, None, None, None)
        .await
        .unwrap();
    assert!(a_results.iter().all(|e| e.namespace == "agent-a"));

    let b_results = ns_b
        .recall("secret", 10, None, None, None, None)
        .await
        .unwrap();
    assert!(b_results.iter().all(|e| e.namespace == "agent-b"));
}

#[tokio::test]
async fn same_key_forget_only_removes_the_callers_namespace() {
    let tmp = tempfile::TempDir::new().unwrap();
    let sqlite = Arc::new(SqliteMemory::new(tmp.path()).unwrap());
    let ns_a = NamespacedMemory::new(sqlite.clone(), "agent-a".to_string());
    let ns_b = NamespacedMemory::new(sqlite.clone(), "agent-b".to_string());

    ns_a.store("preference", "A value", MemoryCategory::Core, None)
        .await
        .unwrap();
    ns_b.store("preference", "B value", MemoryCategory::Core, None)
        .await
        .unwrap();

    assert!(ns_a.forget("preference").await.unwrap());
    assert!(ns_a.get("preference").await.unwrap().is_none());
    assert_eq!(
        ns_b.get("preference").await.unwrap().unwrap().content,
        "B value"
    );
}

#[tokio::test]
async fn public_and_private_namespaces_can_share_a_key() {
    let tmp = tempfile::TempDir::new().unwrap();
    let sqlite: Arc<dyn Memory> = Arc::new(SqliteMemory::new(tmp.path()).unwrap());
    let persona = NamespacedMemory::new(sqlite.clone(), "agent-a".to_string());

    sqlite
        .store_with_metadata(
            "project",
            "public project",
            MemoryCategory::Core,
            None,
            Some("public"),
            None,
        )
        .await
        .unwrap();
    persona
        .store("project", "private project", MemoryCategory::Core, None)
        .await
        .unwrap();

    assert_eq!(
        persona.get("project").await.unwrap().unwrap().content,
        "private project"
    );
    assert_eq!(
        sqlite
            .get_scoped(
                MemoryScope {
                    namespace: "public",
                    session_id: None,
                },
                "project"
            )
            .await
            .unwrap()
            .unwrap()
            .content,
        "public project"
    );
}

#[tokio::test]
async fn scoped_recall_filters_before_applying_limit() {
    let tmp = tempfile::TempDir::new().unwrap();
    let sqlite = SqliteMemory::new(tmp.path()).unwrap();

    for index in 0..30 {
        sqlite
            .store_with_metadata(
                &format!("distractor-{index}"),
                "rust rust rust distractor",
                MemoryCategory::Core,
                None,
                Some("other"),
                None,
            )
            .await
            .unwrap();
    }
    for index in 0..3 {
        sqlite
            .store_with_metadata(
                &format!("target-{index}"),
                "rust target",
                MemoryCategory::Daily,
                Some("session-a"),
                Some("target"),
                None,
            )
            .await
            .unwrap();
    }
    sqlite
        .store_with_metadata(
            "wrong-category",
            "rust target",
            MemoryCategory::Core,
            Some("session-a"),
            Some("target"),
            None,
        )
        .await
        .unwrap();
    sqlite
        .store_with_metadata(
            "wrong-session",
            "rust target",
            MemoryCategory::Daily,
            Some("session-b"),
            Some("target"),
            None,
        )
        .await
        .unwrap();

    let results = sqlite
        .recall_scoped(MemoryQuery {
            query: "rust",
            scope: MemoryScope {
                namespace: "target",
                session_id: Some("session-a"),
            },
            category: Some(&MemoryCategory::Daily),
            since: None,
            until: None,
            limit: 3,
            min_relevance_score: None,
            search_mode: Some(SearchMode::Bm25),
            exclude_ids: &[],
            exclude_keys: &[],
        })
        .await
        .unwrap();

    assert_eq!(results.len(), 3);
    assert!(results.iter().all(|entry| {
        entry.namespace == "target"
            && entry.category == MemoryCategory::Daily
            && entry.session_id.as_deref() == Some("session-a")
    }));
}

#[test]
fn migrates_v1_database_without_losing_rows_or_fts() {
    let tmp = tempfile::TempDir::new().unwrap();
    let memory_dir = tmp.path().join("memory");
    std::fs::create_dir_all(&memory_dir).unwrap();
    let db_path = memory_dir.join("brain.db");
    let connection = rusqlite::Connection::open(&db_path).unwrap();
    connection
        .execute_batch(
            "CREATE TABLE memories (
                 id TEXT PRIMARY KEY,
                 key TEXT NOT NULL UNIQUE,
                 content TEXT NOT NULL,
                 category TEXT NOT NULL DEFAULT 'core',
                 embedding BLOB,
                 created_at TEXT NOT NULL,
                 updated_at TEXT NOT NULL
             );
             INSERT INTO memories VALUES
                 ('id-1', 'alpha', 'Rust migration fixture', 'core', NULL,
                  '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
                 ('id-2', 'beta', 'Second fixture', 'daily', NULL,
                  '2026-01-02T00:00:00Z', '2026-01-02T00:00:00Z');
             PRAGMA user_version = 1;",
        )
        .unwrap();
    drop(connection);

    let memory = SqliteMemory::new(tmp.path()).unwrap();
    let connection = memory.connection().lock();
    let version: i64 = connection
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .unwrap();
    let row_count: i64 = connection
        .query_row("SELECT COUNT(*) FROM memories", [], |row| row.get(0))
        .unwrap();
    let id_count: i64 = connection
        .query_row("SELECT COUNT(DISTINCT id) FROM memories", [], |row| {
            row.get(0)
        })
        .unwrap();
    let default_count: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM memories WHERE namespace = 'default'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let fts_count: i64 = connection
        .query_row("SELECT COUNT(*) FROM memories_fts", [], |row| row.get(0))
        .unwrap();
    let schema: String = connection
        .query_row(
            "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'memories'",
            [],
            |row| row.get(0),
        )
        .unwrap();

    assert_eq!(version, 2);
    assert_eq!(
        (row_count, id_count, default_count, fts_count),
        (2, 2, 2, 2)
    );
    assert!(schema.contains("UNIQUE(namespace, key)"));
    assert!(schema.contains("namespace              TEXT NOT NULL"));
}

#[tokio::test]
async fn namespaced_sqlite_count_is_per_namespace() {
    let tmp = tempfile::TempDir::new().unwrap();
    let sqlite = Arc::new(SqliteMemory::new(tmp.path()).unwrap());

    let ns_a = NamespacedMemory::new(sqlite.clone(), "agent-a".to_string());
    let ns_b = NamespacedMemory::new(sqlite.clone(), "agent-b".to_string());

    ns_a.store("a1", "data", MemoryCategory::Core, None)
        .await
        .unwrap();
    ns_a.store("a2", "data", MemoryCategory::Core, None)
        .await
        .unwrap();
    ns_b.store("b1", "data", MemoryCategory::Core, None)
        .await
        .unwrap();

    assert_eq!(ns_a.count().await.unwrap(), 2);
    assert_eq!(ns_b.count().await.unwrap(), 1);
}

#[tokio::test]
async fn namespaced_sqlite_list_filters_by_namespace() {
    let tmp = tempfile::TempDir::new().unwrap();
    let sqlite = Arc::new(SqliteMemory::new(tmp.path()).unwrap());

    let ns_a = NamespacedMemory::new(sqlite.clone(), "agent-a".to_string());
    let ns_b = NamespacedMemory::new(sqlite.clone(), "agent-b".to_string());

    ns_a.store("a1", "core data", MemoryCategory::Core, None)
        .await
        .unwrap();
    ns_a.store("a2", "daily data", MemoryCategory::Daily, None)
        .await
        .unwrap();
    ns_b.store("b1", "core data", MemoryCategory::Core, None)
        .await
        .unwrap();

    let a_list = ns_a.list(None, None).await.unwrap();
    assert_eq!(a_list.len(), 2);
    assert!(a_list.iter().all(|e| e.namespace == "agent-a"));

    let a_core = ns_a.list(Some(&MemoryCategory::Core), None).await.unwrap();
    assert_eq!(a_core.len(), 1);
    assert_eq!(a_core[0].key, "a1");

    let b_list = ns_b.list(None, None).await.unwrap();
    assert_eq!(b_list.len(), 1);
}

#[tokio::test]
async fn namespaced_sqlite_forget_only_own_namespace() {
    let tmp = tempfile::TempDir::new().unwrap();
    let sqlite = Arc::new(SqliteMemory::new(tmp.path()).unwrap());

    let ns_a = NamespacedMemory::new(sqlite.clone(), "agent-a".to_string());
    let ns_b = NamespacedMemory::new(sqlite.clone(), "agent-b".to_string());

    // Different keys in different namespaces
    ns_a.store("a_key", "A data", MemoryCategory::Core, None)
        .await
        .unwrap();
    ns_b.store("b_key", "B data", MemoryCategory::Core, None)
        .await
        .unwrap();

    // ns_a forgets its own key
    let removed = ns_a.forget("a_key").await.unwrap();
    assert!(removed);

    // ns_b should be unaffected
    assert_eq!(ns_b.count().await.unwrap(), 1);
    let b_entry = ns_b.get("b_key").await.unwrap();
    assert!(b_entry.is_some());
}

#[tokio::test]
async fn namespaced_sqlite_forget_nonexistent_returns_false() {
    let (_tmp, mem) = make_namespaced("agent-1");
    let removed = mem.forget("nope").await.unwrap();
    assert!(!removed);
}

#[tokio::test]
async fn namespaced_sqlite_recall_namespaced_own() {
    let (_tmp, mem) = make_namespaced("agent-1");

    mem.store("k1", "data in agent-1", MemoryCategory::Core, None)
        .await
        .unwrap();

    // Recall with matching namespace
    let results = mem
        .recall_namespaced("agent-1", "data", 10, None, None, None, None)
        .await
        .unwrap();
    assert!(!results.is_empty());

    // Recall with different namespace should return empty
    let results = mem
        .recall_namespaced("agent-other", "data", 10, None, None, None, None)
        .await
        .unwrap();
    assert!(results.is_empty());
}

#[tokio::test]
async fn namespaced_sqlite_purge_own_namespace() {
    let tmp = tempfile::TempDir::new().unwrap();
    let sqlite = Arc::new(SqliteMemory::new(tmp.path()).unwrap());

    let ns_a = NamespacedMemory::new(sqlite.clone(), "agent-a".to_string());
    let ns_b = NamespacedMemory::new(sqlite.clone(), "agent-b".to_string());

    ns_a.store("a1", "data", MemoryCategory::Core, None)
        .await
        .unwrap();
    ns_b.store("b1", "data", MemoryCategory::Core, None)
        .await
        .unwrap();

    let count = ns_a.purge_namespace("agent-a").await.unwrap();
    assert_eq!(
        count, 1,
        "purge should remove 1 entry from agent-a namespace"
    );

    // ns_b should be unaffected
    assert_eq!(ns_b.count().await.unwrap(), 1);
}

#[tokio::test]
async fn namespaced_sqlite_purge_other_namespace_is_error() {
    let (_tmp, mem) = make_namespaced("agent-a");
    let result = mem.purge_namespace("agent-b").await;
    assert!(result.is_err());
}

#[tokio::test]
async fn namespaced_sqlite_purge_session_scoped_to_namespace() {
    let tmp = tempfile::TempDir::new().unwrap();
    let sqlite = Arc::new(SqliteMemory::new(tmp.path()).unwrap());

    let ns_a = NamespacedMemory::new(sqlite.clone(), "agent-a".to_string());
    let ns_b = NamespacedMemory::new(sqlite.clone(), "agent-b".to_string());

    ns_a.store(
        "a_sess",
        "session data",
        MemoryCategory::Core,
        Some("sess-1"),
    )
    .await
    .unwrap();
    ns_b.store(
        "b_sess",
        "session data",
        MemoryCategory::Core,
        Some("sess-1"),
    )
    .await
    .unwrap();

    // ns_a purges session-1, should only affect its own entries
    let count = ns_a.purge_session("sess-1").await.unwrap();
    assert_eq!(count, 1);

    // ns_b's entry should survive
    assert_eq!(ns_b.count().await.unwrap(), 1);
}

#[tokio::test]
async fn namespaced_sqlite_store_with_metadata_forces_namespace() {
    let (_tmp, mem) = make_namespaced("agent-1");

    // Store with a different namespace param — should be overridden
    mem.store_with_metadata(
        "k1",
        "data",
        MemoryCategory::Core,
        None,
        Some("other-namespace"), // should be ignored
        Some(0.9),
    )
    .await
    .unwrap();

    let entry = mem.get("k1").await.unwrap().unwrap();
    assert_eq!(
        entry.namespace, "agent-1",
        "namespace should be forced to the configured one"
    );
    assert_eq!(entry.importance, Some(0.9));
}

#[tokio::test]
async fn namespaced_sqlite_health_check() {
    let (_tmp, mem) = make_namespaced("agent-1");
    assert!(mem.health_check().await);
}

#[tokio::test]
async fn namespaced_sqlite_name_delegates() {
    let (_tmp, mem) = make_namespaced("agent-1");
    assert_eq!(mem.name(), "sqlite");
}

#[tokio::test]
async fn namespaced_sqlite_export_with_filter() {
    let tmp = tempfile::TempDir::new().unwrap();
    let sqlite = Arc::new(SqliteMemory::new(tmp.path()).unwrap());

    let ns_a = NamespacedMemory::new(sqlite.clone(), "agent-a".to_string());
    let ns_b = NamespacedMemory::new(sqlite.clone(), "agent-b".to_string());

    ns_a.store("a1", "core data", MemoryCategory::Core, None)
        .await
        .unwrap();
    ns_a.store("a2", "daily data", MemoryCategory::Daily, None)
        .await
        .unwrap();
    ns_b.store("b1", "core data", MemoryCategory::Core, None)
        .await
        .unwrap();

    // Export from ns_a — should only see ns_a entries
    let filter = ExportFilter::default();
    let results = ns_a.export(&filter).await.unwrap();
    assert_eq!(results.len(), 2);
    assert!(results.iter().all(|e| e.namespace == "agent-a"));
}
