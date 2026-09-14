//! Namespace isolation for memory operations.
//!
//! Provides a decorator `NamespacedMemory<M>` that wraps any `Memory` backend
//! and enforces a fixed namespace for all operations. Useful for delegate agents
//! to isolate their memory from other agents' memory spaces.
//!
//! All store operations redirect to `store_with_metadata()` with the configured
//! namespace, and all recall operations redirect to `recall_namespaced()`.

use super::traits::{Memory, MemoryCategory, MemoryEntry, MemoryQuery, MemoryScope, SearchMode};
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

    fn accessible_namespaces(&self) -> Vec<String> {
        let mut namespaces = vec![self.namespace.clone()];
        if self.namespace != PUBLIC_NAMESPACE {
            namespaces.push(PUBLIC_NAMESPACE.into());
        }
        namespaces
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
        let query_for = |namespace| MemoryQuery {
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
        };
        let mut entries = self
            .inner
            .recall_scoped_with_embeddings(query_for(&self.namespace))
            .await?;
        entries.extend(
            self.inner
                .recall_scoped_with_embeddings(query_for(PUBLIC_NAMESPACE))
                .await?,
        );
        entries.sort_by(|left, right| {
            right
                .score
                .unwrap_or_default()
                .partial_cmp(&left.score.unwrap_or_default())
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| left.key.cmp(&right.key))
        });
        entries.truncate(limit);
        Ok(entries)
    }

    async fn get(&self, key: &str) -> anyhow::Result<Option<MemoryEntry>> {
        if let Some(entry) = self
            .inner
            .get_scoped(
                MemoryScope {
                    namespace: &self.namespace,
                    session_id: None,
                },
                key,
            )
            .await?
        {
            return Ok(Some(entry));
        }
        self.inner
            .get_scoped(
                MemoryScope {
                    namespace: PUBLIC_NAMESPACE,
                    session_id: None,
                },
                key,
            )
            .await
    }

    async fn get_scoped(
        &self,
        scope: MemoryScope<'_>,
        key: &str,
    ) -> anyhow::Result<Option<MemoryEntry>> {
        if self.can_access_namespace(scope.namespace) {
            self.inner.get_scoped(scope, key).await
        } else {
            Ok(None)
        }
    }

    async fn list(
        &self,
        category: Option<&MemoryCategory>,
        session_id: Option<&str>,
    ) -> anyhow::Result<Vec<MemoryEntry>> {
        let mut entries = self
            .inner
            .list_scoped(
                MemoryScope {
                    namespace: &self.namespace,
                    session_id,
                },
                category,
            )
            .await?;
        entries.extend(
            self.inner
                .list_scoped(
                    MemoryScope {
                        namespace: PUBLIC_NAMESPACE,
                        session_id,
                    },
                    category,
                )
                .await?,
        );
        Ok(entries)
    }

    async fn forget(&self, key: &str) -> anyhow::Result<bool> {
        self.inner
            .forget_scoped(
                MemoryScope {
                    namespace: &self.namespace,
                    session_id: None,
                },
                key,
            )
            .await
    }

    async fn forget_scoped(&self, scope: MemoryScope<'_>, key: &str) -> anyhow::Result<bool> {
        if self.can_access_namespace(scope.namespace) {
            self.inner.forget_scoped(scope, key).await
        } else {
            Ok(false)
        }
    }

    async fn count(&self) -> anyhow::Result<usize> {
        Ok(self.list(None, None).await?.len())
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

    async fn recall_scoped(&self, query: MemoryQuery<'_>) -> anyhow::Result<Vec<MemoryEntry>> {
        if self.can_access_namespace(query.scope.namespace) {
            self.inner.recall_scoped(query).await
        } else {
            Ok(Vec::new())
        }
    }

    async fn recall_scoped_with_embeddings(
        &self,
        query: MemoryQuery<'_>,
    ) -> anyhow::Result<Vec<MemoryEntry>> {
        if self.can_access_namespace(query.scope.namespace) {
            self.inner.recall_scoped_with_embeddings(query).await
        } else {
            Ok(Vec::new())
        }
    }

    async fn top_core_memories(&self, limit: usize) -> anyhow::Result<Vec<MemoryEntry>> {
        let mut entries = self
            .inner
            .top_core_memories_scoped(&self.namespace, limit)
            .await?;
        entries.extend(
            self.inner
                .top_core_memories_scoped(PUBLIC_NAMESPACE, limit)
                .await?,
        );
        entries.sort_by(|left, right| {
            right
                .importance
                .unwrap_or(0.5)
                .partial_cmp(&left.importance.unwrap_or(0.5))
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| right.timestamp.cmp(&left.timestamp))
                .then_with(|| left.key.cmp(&right.key))
        });
        entries.truncate(limit);
        Ok(entries)
    }

    async fn top_core_memories_scoped(
        &self,
        namespace: &str,
        limit: usize,
    ) -> anyhow::Result<Vec<MemoryEntry>> {
        if self.can_access_namespace(namespace) {
            self.inner.top_core_memories_scoped(namespace, limit).await
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
        let entries = self.list(None, Some(session_id)).await?;
        let mut count = 0;
        for entry in entries {
            if self.can_access_namespace(&entry.namespace)
                && self
                    .inner
                    .forget_scoped(
                        MemoryScope {
                            namespace: &entry.namespace,
                            session_id: Some(session_id),
                        },
                        &entry.key,
                    )
                    .await?
            {
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

    #[tokio::test]
    async fn scoped_get_and_forget_do_not_shadow_same_key_across_namespaces() {
        let tmp = tempfile::tempdir().unwrap();
        let inner: Arc<dyn Memory> = Arc::new(SqliteMemory::new(tmp.path()).unwrap());
        for (namespace, content) in [
            ("persona_a", "Private value"),
            (PUBLIC_NAMESPACE, "Public value"),
        ] {
            inner
                .store_with_metadata(
                    "shared_key",
                    content,
                    MemoryCategory::Core,
                    None,
                    Some(namespace),
                    None,
                )
                .await
                .unwrap();
        }
        let persona = NamespacedMemory::new(inner.clone(), "persona_a".into());
        let public_scope = MemoryScope {
            namespace: PUBLIC_NAMESPACE,
            session_id: None,
        };

        let public = persona
            .get_scoped(public_scope, "shared_key")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(public.content, "Public value");
        assert!(
            persona
                .forget_scoped(public_scope, "shared_key")
                .await
                .unwrap()
        );
        assert!(
            inner
                .get_scoped(public_scope, "shared_key")
                .await
                .unwrap()
                .is_none()
        );
        assert_eq!(
            persona.get("shared_key").await.unwrap().unwrap().content,
            "Private value"
        );
    }
}
