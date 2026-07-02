//! Namespace isolation for memory operations.
//!
//! Provides a decorator `NamespacedMemory<M>` that wraps any `Memory` backend
//! and enforces a fixed namespace for all operations. Useful for delegate agents
//! to isolate their memory from other agents' memory spaces.
//!
//! All store operations redirect to `store_with_metadata()` with the configured
//! namespace, and all recall operations redirect to `recall_namespaced()`.

use super::traits::{Memory, MemoryCategory, MemoryEntry, SearchMode};
use async_trait::async_trait;
use std::sync::Arc;

pub const PUBLIC_NAMESPACE: &str = "public";

/// Decorator that wraps a `Memory` backend with namespace isolation.
///
/// When configured with a namespace, all memory operations are scoped to that
/// namespace, preventing cross-contamination between agents with different
/// memory namespaces.
pub struct NamespacedMemory {
    inner: Arc<dyn Memory>,
    namespace: String,
}

impl NamespacedMemory {
    /// Create a new NamespacedMemory wrapping an existing memory backend.
    pub fn new(inner: Arc<dyn Memory>, namespace: String) -> Self {
        Self { inner, namespace }
    }

    /// Get the namespace used by this decorator.
    pub fn namespace(&self) -> &str {
        &self.namespace
    }

    fn can_access_namespace(&self, namespace: &str) -> bool {
        namespace == self.namespace || namespace == PUBLIC_NAMESPACE
    }
}

#[async_trait]
impl Memory for NamespacedMemory {
    fn name(&self) -> &str {
        self.inner.name()
    }

    async fn store(
        &self,
        key: &str,
        content: &str,
        category: MemoryCategory,
        session_id: Option<&str>,
    ) -> anyhow::Result<()> {
        self.inner
            .store_with_metadata(
                key,
                content,
                category,
                session_id,
                Some(&self.namespace),
                None,
            )
            .await
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
        let mut entries = self
            .inner
            .recall_namespaced(
                &self.namespace,
                query,
                limit,
                session_id,
                since,
                until,
                search_mode,
            )
            .await?;
        let public_entries = self
            .inner
            .recall_namespaced(
                PUBLIC_NAMESPACE,
                query,
                limit,
                session_id,
                since,
                until,
                search_mode,
            )
            .await?;
        entries.extend(public_entries);
        entries.sort_by(|a, b| {
            b.score
                .unwrap_or(0.0)
                .partial_cmp(&a.score.unwrap_or(0.0))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        entries.truncate(limit);
        Ok(entries)
    }

    async fn recall_with_embeddings(
        &self,
        query: &str,
        limit: usize,
        session_id: Option<&str>,
        since: Option<&str>,
        until: Option<&str>,
        search_mode: Option<SearchMode>,
    ) -> anyhow::Result<Vec<MemoryEntry>> {
        // Delegate to inner, then filter by namespace
        // We fetch more entries to account for filtering
        let entries = self
            .inner
            .recall_with_embeddings(query, limit * 2, session_id, since, until, search_mode)
            .await?;
        let filtered: Vec<MemoryEntry> = entries
            .into_iter()
            .filter(|e| self.can_access_namespace(&e.namespace))
            .take(limit)
            .collect();
        Ok(filtered)
    }

    async fn get(&self, key: &str) -> anyhow::Result<Option<MemoryEntry>> {
        let entry = self.inner.get(key).await?;
        // Return the entry only if it matches our namespace
        Ok(entry.filter(|e| self.can_access_namespace(&e.namespace)))
    }

    async fn list(
        &self,
        category: Option<&MemoryCategory>,
        session_id: Option<&str>,
    ) -> anyhow::Result<Vec<MemoryEntry>> {
        let entries = self.inner.list(category, session_id).await?;
        // Filter to only entries in our namespace
        Ok(entries
            .into_iter()
            .filter(|e| self.can_access_namespace(&e.namespace))
            .collect())
    }

    async fn forget(&self, key: &str) -> anyhow::Result<bool> {
        // First verify the entry is in our namespace before forgetting
        if let Some(entry) = self.inner.get(key).await?
            && self.can_access_namespace(&entry.namespace)
        {
            return self.inner.forget(key).await;
        }
        Ok(false)
    }

    async fn count(&self) -> anyhow::Result<usize> {
        let entries = self.inner.list(None, None).await?;
        Ok(entries
            .into_iter()
            .filter(|e| self.can_access_namespace(&e.namespace))
            .count())
    }

    async fn health_check(&self) -> bool {
        self.inner.health_check().await
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
        // If the requested namespace is own or public, delegate to the inner memory.
        // Otherwise, return empty results (namespace isolation).
        if self.can_access_namespace(namespace) {
            self.inner
                .recall_namespaced(
                    namespace,
                    query,
                    limit,
                    session_id,
                    since,
                    until,
                    search_mode,
                )
                .await
        } else {
            Ok(Vec::new())
        }
    }

    async fn store_with_metadata(
        &self,
        key: &str,
        content: &str,
        category: MemoryCategory,
        session_id: Option<&str>,
        _namespace: Option<&str>,
        importance: Option<f64>,
    ) -> anyhow::Result<()> {
        let namespace = if _namespace == Some(PUBLIC_NAMESPACE) {
            PUBLIC_NAMESPACE
        } else {
            &self.namespace
        };
        self.inner
            .store_with_metadata(
                key,
                content,
                category,
                session_id,
                Some(namespace),
                importance,
            )
            .await
    }

    async fn purge_namespace(&self, namespace: &str) -> anyhow::Result<usize> {
        // Only allow purging own or public namespace.
        if self.can_access_namespace(namespace) {
            self.inner.purge_namespace(namespace).await
        } else {
            anyhow::bail!(
                "Cannot purge namespace '{}' from isolation context '{}'",
                namespace,
                self.namespace
            )
        }
    }

    async fn purge_session(&self, session_id: &str) -> anyhow::Result<usize> {
        // Purge sessions, but filtered to our namespace
        let entries = self.inner.list(None, Some(session_id)).await?;
        let mut count = 0;
        for entry in entries {
            if self.can_access_namespace(&entry.namespace) && self.inner.forget(&entry.key).await? {
                count += 1;
            }
        }
        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::none::NoneMemory;
    use crate::sqlite::SqliteMemory;

    #[tokio::test]
    async fn namespaced_memory_enforces_namespace_on_store() {
        let inner = Arc::new(NoneMemory::new());
        let namespaced = NamespacedMemory::new(inner, "test_namespace".to_string());

        // Store should succeed
        namespaced
            .store("key1", "value1", MemoryCategory::Core, None)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn namespaced_memory_prevents_cross_namespace_access() {
        let inner = Arc::new(NoneMemory::new());
        let namespaced = NamespacedMemory::new(inner, "test_namespace".to_string());

        // Try to recall from a different namespace (no-op for NoneMemory)
        let results = namespaced
            .recall_namespaced("other_namespace", "query", 10, None, None, None, None)
            .await
            .unwrap();
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn namespaced_memory_delegates_correctly() {
        let inner = Arc::new(NoneMemory::new());
        let namespaced = NamespacedMemory::new(inner, "test_namespace".to_string());

        assert_eq!(namespaced.name(), "none");
        assert!(namespaced.health_check().await);
        assert_eq!(namespaced.count().await.unwrap(), 0);
    }

    // Real-backend isolation test: NoneMemory is a no-op (store does nothing,
    // recall always returns empty), so cross-namespace assertions would pass
    // regardless of whether NamespacedMemory is correct. This test uses a
    // temporary SQLite backend so store/recall actually persist, exercising the
    // real store_with_metadata / recall_namespaced path used by personas.
    #[tokio::test]
    async fn sqlite_two_namespaces_are_isolated() {
        let tmp = tempfile::tempdir().unwrap();
        let inner: Arc<dyn Memory> = Arc::new(SqliteMemory::new(tmp.path()).unwrap());

        let persona_a = NamespacedMemory::new(inner.clone(), "persona_a".into());
        let persona_b = NamespacedMemory::new(inner.clone(), "persona_b".into());

        // Persona A stores a memory.
        persona_a
            .store(
                "k_a",
                "Nova remembers the user likes Rust",
                MemoryCategory::Core,
                None,
            )
            .await
            .unwrap();

        // Persona A recalls its own memory — must find it.
        let a_hits = persona_a
            .recall("Rust", 10, None, None, None, None)
            .await
            .unwrap();
        assert!(
            a_hits.iter().any(|e| e.key == "k_a"),
            "persona A should see its own memory, got: {a_hits:?}"
        );

        // Persona B recalls the same query — must NOT see persona A's memory.
        let b_hits = persona_b
            .recall("Rust", 10, None, None, None, None)
            .await
            .unwrap();
        assert!(
            b_hits.iter().all(|e| e.key != "k_a"),
            "persona B must not see persona A's memory, got: {b_hits:?}"
        );

        // The underlying backend still holds the row (shared storage, namespaced view).
        let total = inner.count().await.unwrap();
        assert_eq!(total, 1, "shared backend should hold 1 row");
        assert_eq!(persona_a.count().await.unwrap(), 1);
        assert_eq!(persona_b.count().await.unwrap(), 0);
    }

    #[tokio::test]
    async fn sqlite_public_namespace_is_visible_to_all_personas() {
        let tmp = tempfile::tempdir().unwrap();
        let inner: Arc<dyn Memory> = Arc::new(SqliteMemory::new(tmp.path()).unwrap());

        let persona_a = NamespacedMemory::new(inner.clone(), "persona_a".into());
        let persona_b = NamespacedMemory::new(inner.clone(), "persona_b".into());

        persona_a
            .store_with_metadata(
                "k_public",
                "The shared project language is Rust",
                MemoryCategory::Core,
                None,
                Some(PUBLIC_NAMESPACE),
                None,
            )
            .await
            .unwrap();
        persona_a
            .store(
                "k_private",
                "Persona A private note about Rust",
                MemoryCategory::Core,
                None,
            )
            .await
            .unwrap();

        let b_hits = persona_b
            .recall("Rust", 10, None, None, None, None)
            .await
            .unwrap();
        assert!(
            b_hits
                .iter()
                .any(|e| e.key == "k_public" && e.namespace == PUBLIC_NAMESPACE),
            "persona B should see public memory, got: {b_hits:?}"
        );
        assert!(
            b_hits.iter().all(|e| e.key != "k_private"),
            "persona B must not see persona A private memory, got: {b_hits:?}"
        );
    }
}
