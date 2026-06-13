//! Memory trait and related types.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Filter criteria for bulk memory export.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ExportFilter {
    pub namespace: Option<String>,
    pub session_id: Option<String>,
    pub category: Option<MemoryCategory>,
    pub since: Option<String>,
    pub until: Option<String>,
}

/// A single memory entry.
#[derive(Clone, Serialize, Deserialize)]
pub struct MemoryEntry {
    pub id: String,
    pub key: String,
    pub content: String,
    pub category: MemoryCategory,
    pub timestamp: String,
    pub session_id: Option<String>,
    pub score: Option<f64>,
    #[serde(default = "default_namespace")]
    pub namespace: String,
    #[serde(default)]
    pub importance: Option<f64>,
    #[serde(default)]
    pub superseded_by: Option<String>,
    /// Embedding vector (if available). Used for conflict detection in Phase E.
    #[serde(default)]
    pub embedding: Option<Vec<f32>>,
}

fn default_namespace() -> String {
    "default".into()
}

impl std::fmt::Debug for MemoryEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MemoryEntry")
            .field("id", &self.id)
            .field("key", &self.key)
            .field("content", &self.content)
            .field("category", &self.category)
            .field("timestamp", &self.timestamp)
            .field("score", &self.score)
            .field("namespace", &self.namespace)
            .field("importance", &self.importance)
            .finish_non_exhaustive()
    }
}

/// Merge strategy for combining vector and keyword search results.
///
/// In TOML config, represented as a simple string:
/// - `"rrf"` — RRF with default k=60
/// - `"rrf(k=60)"` — RRF with custom k
/// - `"weighted(v=0.7,kw=0.3)"` — Weighted with custom weights
/// - `"weighted"` — Weighted with defaults (v=0.7, kw=0.3)
///
/// Custom serde allows simple string representation in TOML.
#[derive(Debug, Clone, PartialEq)]
pub enum MergeStrategy {
    /// Reciprocal Rank Fusion — uses rank positions instead of raw scores,
    /// eliminating BM25-vector scale mismatch. `k` is the RRF constant
    /// (paper recommends 60).
    Rrf { k: u32 },
    /// Weighted fusion — normalize each score set, then combine with weights.
    Weighted {
        vector_weight: f32,
        keyword_weight: f32,
    },
}

impl Default for MergeStrategy {
    fn default() -> Self {
        // RRF is the new default — eliminates BM25 scale normalization issues.
        Self::Rrf { k: 60 }
    }
}

impl std::fmt::Display for MergeStrategy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Rrf { k } => write!(f, "rrf(k={k})"),
            Self::Weighted {
                vector_weight,
                keyword_weight,
            } => {
                write!(f, "weighted(v={vector_weight},kw={keyword_weight})")
            }
        }
    }
}

impl std::str::FromStr for MergeStrategy {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let lower = s.to_ascii_lowercase();
        if lower == "rrf" {
            return Ok(Self::Rrf { k: 60 });
        }
        if lower == "weighted" {
            return Ok(Self::Weighted {
                vector_weight: 0.7,
                keyword_weight: 0.3,
            });
        }
        // Parse "rrf(k=60)" format
        if lower.starts_with("rrf(") && lower.ends_with(')') {
            let inner = &lower["rrf(".len()..lower.len() - 1];
            for part in inner.split(',') {
                let part = part.trim();
                if let Some(val) = part.strip_prefix("k=")
                    && let Ok(k) = val.parse::<u32>()
                {
                    return Ok(Self::Rrf { k });
                }
            }
            return Err(format!("invalid rrf format: '{s}'"));
        }
        // Parse "weighted(v=0.7,kw=0.3)" format
        if lower.starts_with("weighted(") && lower.ends_with(')') {
            let inner = &lower["weighted(".len()..lower.len() - 1];
            let mut v = None;
            let mut kw = None;
            for part in inner.split(',') {
                let part = part.trim();
                if let Some(val) = part.strip_prefix("v=") {
                    v = val.parse::<f32>().ok();
                } else if let Some(val) = part.strip_prefix("kw=") {
                    kw = val.parse::<f32>().ok();
                }
            }
            if let (Some(vw), Some(kww)) = (v, kw) {
                return Ok(Self::Weighted {
                    vector_weight: vw,
                    keyword_weight: kww,
                });
            }
            return Err(format!("invalid weighted format: '{s}'"));
        }
        Err(format!(
            "unknown MergeStrategy: '{s}', expected 'rrf', 'weighted', 'rrf(k=...)', or 'weighted(v=...,kw=...)'"
        ))
    }
}

impl serde::Serialize for MergeStrategy {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> serde::Deserialize<'de> for MergeStrategy {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        s.parse::<Self>().map_err(serde::de::Error::custom)
    }
}

/// Conflict detection mode for memory consolidation.
///
/// In TOML config, represented as a simple string:
/// - `"jaccard"` — pure Jaccard word-set overlap (legacy behavior)
/// - `"combined"` — weighted combination with default weights (j=0.4, c=0.4, b=0.2)
/// - `"combined(j=0.4,c=0.4,b=0.2)"` — weighted combination with custom weights
///
/// Internally stored as the full enum for programmatic use.
/// Custom serde implementation allows simple string representation in TOML
/// while preserving the full struct form in JSON.
#[derive(Debug, Clone, PartialEq)]
pub enum ConflictMode {
    /// Pure Jaccard word-set overlap (legacy behavior).
    Jaccard,
    /// Weighted combination of Jaccard + cosine similarity + BM25 overlap.
    Combined {
        jaccard_w: f32,
        cosine_w: f32,
        bm25_w: f32,
    },
}

impl Default for ConflictMode {
    fn default() -> Self {
        Self::Combined {
            jaccard_w: 0.4,
            cosine_w: 0.4,
            bm25_w: 0.2,
        }
    }
}

impl std::fmt::Display for ConflictMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Jaccard => write!(f, "jaccard"),
            Self::Combined {
                jaccard_w,
                cosine_w,
                bm25_w,
            } => {
                write!(f, "combined(j={jaccard_w},c={cosine_w},b={bm25_w})")
            }
        }
    }
}

impl std::str::FromStr for ConflictMode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let lower = s.to_ascii_lowercase();
        if lower == "jaccard" {
            return Ok(Self::Jaccard);
        }
        if lower == "combined" {
            return Ok(Self::default());
        }
        // Parse "combined(j=0.4,c=0.4,b=0.2)" format
        if lower.starts_with("combined(") && lower.ends_with(')') {
            let inner = &lower["combined(".len()..lower.len() - 1];
            let mut j = None;
            let mut c = None;
            let mut b = None;
            for part in inner.split(',') {
                let part = part.trim();
                if let Some(val) = part.strip_prefix("j=") {
                    j = val.parse::<f32>().ok();
                } else if let Some(val) = part.strip_prefix("c=") {
                    c = val.parse::<f32>().ok();
                } else if let Some(val) = part.strip_prefix("b=") {
                    b = val.parse::<f32>().ok();
                }
            }
            if let (Some(jw), Some(cw), Some(bw)) = (j, c, b) {
                return Ok(Self::Combined {
                    jaccard_w: jw,
                    cosine_w: cw,
                    bm25_w: bw,
                });
            }
            return Err(format!("invalid combined weights: '{s}'"));
        }
        Err(format!(
            "unknown ConflictMode: '{s}', expected 'jaccard', 'combined', or 'combined(j=...,c=...,b=...)'"
        ))
    }
}

impl serde::Serialize for ConflictMode {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> serde::Deserialize<'de> for ConflictMode {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        s.parse::<Self>().map_err(serde::de::Error::custom)
    }
}

/// Search mode for memory recall operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum SearchMode {
    /// Combine vector similarity and keyword (BM25) search.
    #[default]
    Hybrid,
    /// Vector embedding similarity only.
    Embedding,
    /// BM25 keyword search only.
    Bm25,
}

impl std::fmt::Display for SearchMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SearchMode::Hybrid => write!(f, "hybrid"),
            SearchMode::Embedding => write!(f, "embedding"),
            SearchMode::Bm25 => write!(f, "bm25"),
        }
    }
}

/// Memory categories for organization.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MemoryCategory {
    Core,
    Daily,
    Conversation,
    Custom(String),
}

impl serde::Serialize for MemoryCategory {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> serde::Deserialize<'de> for MemoryCategory {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        Ok(match s.as_str() {
            "core" => Self::Core,
            "daily" => Self::Daily,
            "conversation" => Self::Conversation,
            _ => Self::Custom(s),
        })
    }
}

impl std::fmt::Display for MemoryCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Core => write!(f, "core"),
            Self::Daily => write!(f, "daily"),
            Self::Conversation => write!(f, "conversation"),
            Self::Custom(name) => write!(f, "{name}"),
        }
    }
}

/// Core memory trait — implement for any persistence backend.
#[async_trait]
#[allow(clippy::too_many_arguments)]
pub trait Memory: Send + Sync {
    fn name(&self) -> &str;

    async fn store(
        &self,
        key: &str,
        content: &str,
        category: MemoryCategory,
        session_id: Option<&str>,
    ) -> anyhow::Result<()>;

    async fn recall(
        &self,
        query: &str,
        limit: usize,
        session_id: Option<&str>,
        since: Option<&str>,
        until: Option<&str>,
        search_mode: Option<SearchMode>,
    ) -> anyhow::Result<Vec<MemoryEntry>>;

    async fn get(&self, key: &str) -> anyhow::Result<Option<MemoryEntry>>;

    async fn list(
        &self,
        category: Option<&MemoryCategory>,
        session_id: Option<&str>,
    ) -> anyhow::Result<Vec<MemoryEntry>>;

    async fn forget(&self, key: &str) -> anyhow::Result<bool>;

    async fn purge_namespace(&self, _namespace: &str) -> anyhow::Result<usize> {
        anyhow::bail!("purge_namespace not supported by this memory backend")
    }

    async fn purge_session(&self, _session_id: &str) -> anyhow::Result<usize> {
        anyhow::bail!("purge_session not supported by this memory backend")
    }

    async fn count(&self) -> anyhow::Result<usize>;

    async fn health_check(&self) -> bool;

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
        let entries = self
            .recall(query, limit * 2, session_id, since, until, search_mode)
            .await?;
        let filtered: Vec<MemoryEntry> = entries
            .into_iter()
            .filter(|e| e.namespace == namespace)
            .take(limit)
            .collect();
        Ok(filtered)
    }

    async fn export(&self, filter: &ExportFilter) -> anyhow::Result<Vec<MemoryEntry>> {
        let entries = self
            .list(filter.category.as_ref(), filter.session_id.as_deref())
            .await?;
        let filtered: Vec<MemoryEntry> = entries
            .into_iter()
            .filter(|e| {
                if let Some(ref ns) = filter.namespace
                    && e.namespace != *ns
                {
                    return false;
                }
                if let Some(ref since) = filter.since
                    && e.timestamp.as_str() < since.as_str()
                {
                    return false;
                }
                if let Some(ref until) = filter.until
                    && e.timestamp.as_str() > until.as_str()
                {
                    return false;
                }
                true
            })
            .collect();
        Ok(filtered)
    }

    async fn store_with_metadata(
        &self,
        key: &str,
        content: &str,
        category: MemoryCategory,
        session_id: Option<&str>,
        _namespace: Option<&str>,
        _importance: Option<f64>,
    ) -> anyhow::Result<()> {
        self.store(key, content, category, session_id).await
    }

    /// Retrieve the top N Core memories by importance.
    ///
    /// Default implementation lists all Core memories and sorts by importance.
    /// SqliteMemory overrides this with a direct SQL query for efficiency
    /// (avoids the DEFAULT_LIST_LIMIT=1000 cap).
    async fn top_core_memories(&self, limit: usize) -> anyhow::Result<Vec<MemoryEntry>> {
        let entries = self.list(Some(&MemoryCategory::Core), None).await?;
        let mut sorted = entries;
        sorted.sort_by(|a, b| {
            b.importance
                .unwrap_or(0.5)
                .partial_cmp(&a.importance.unwrap_or(0.5))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        sorted.truncate(limit);
        Ok(sorted)
    }

    /// Recall memories with embedding vectors included.
    ///
    /// Same as `recall()` but populates the `embedding` field on each entry.
    /// Used by conflict detection (Phase E) which needs embeddings for
    /// cosine similarity computation.
    ///
    /// Default implementation delegates to `recall()` which sets `embedding: None`.
    async fn recall_with_embeddings(
        &self,
        query: &str,
        limit: usize,
        session_id: Option<&str>,
        since: Option<&str>,
        until: Option<&str>,
        search_mode: Option<SearchMode>,
    ) -> anyhow::Result<Vec<MemoryEntry>> {
        self.recall(query, limit, session_id, since, until, search_mode)
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_category_display() {
        assert_eq!(MemoryCategory::Core.to_string(), "core");
        assert_eq!(MemoryCategory::Daily.to_string(), "daily");
        assert_eq!(MemoryCategory::Conversation.to_string(), "conversation");
        assert_eq!(MemoryCategory::Custom("notes".into()).to_string(), "notes");
    }

    #[test]
    fn memory_category_serde_roundtrip() {
        let core = serde_json::to_string(&MemoryCategory::Core).unwrap();
        assert_eq!(core, "\"core\"");
        let parsed: MemoryCategory = serde_json::from_str(&core).unwrap();
        assert_eq!(parsed, MemoryCategory::Core);
    }

    #[test]
    fn memory_entry_roundtrip() {
        let entry = MemoryEntry {
            id: "id-1".into(),
            key: "favorite_language".into(),
            content: "Rust".into(),
            category: MemoryCategory::Core,
            timestamp: "2026-02-16T00:00:00Z".into(),
            session_id: Some("session-abc".into()),
            score: Some(0.98),
            namespace: "default".into(),
            importance: Some(0.7),
            superseded_by: None,
            embedding: None,
        };
        let json = serde_json::to_string(&entry).unwrap();
        let parsed: MemoryEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.id, "id-1");
        assert_eq!(parsed.key, "favorite_language");
        assert_eq!(parsed.content, "Rust");
    }
}
