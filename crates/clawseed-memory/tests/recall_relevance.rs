use async_trait::async_trait;
use clawseed_api::memory_traits::{
    Memory, MemoryCategory, MemoryQuery, MemoryScope, MergeStrategy, SearchMode,
};
use clawseed_memory::{embeddings::EmbeddingProvider, sqlite::SqliteMemory};
use std::sync::Arc;

async fn recall(
    mem: &SqliteMemory,
    text: &str,
    mode: SearchMode,
    minimum: f64,
    limit: usize,
) -> Vec<clawseed_api::memory_traits::MemoryEntry> {
    mem.recall_scoped(MemoryQuery {
        query: text,
        scope: MemoryScope::default(),
        category: Some(&MemoryCategory::Core),
        since: None,
        until: None,
        limit,
        min_relevance_score: Some(minimum),
        search_mode: Some(mode),
        exclude_ids: &[],
        exclude_keys: &[],
    })
    .await
    .unwrap()
}

#[tokio::test]
async fn substring_fallback_does_not_promote_weak_matches_to_full_relevance() {
    let dir = tempfile::tempdir().unwrap();
    let mem = SqliteMemory::new(dir.path()).unwrap();
    mem.store("hobby", "用户的爱好是喝咖啡", MemoryCategory::Core, None)
        .await
        .unwrap();
    mem.store("other", "cartography", MemoryCategory::Core, None)
        .await
        .unwrap();
    for query in ["好", "car", "%", "_"] {
        assert!(
            recall(&mem, query, SearchMode::Bm25, 0.3, 5)
                .await
                .is_empty(),
            "unexpected recall for {query}"
        );
    }
    let related = recall(&mem, "咖啡", SearchMode::Bm25, 0.3, 5).await;
    assert_eq!(
        related.len(),
        1,
        "meaningful Chinese substrings remain searchable"
    );
    assert_eq!(related[0].key, "hobby");
    assert!(
        mem.recall("%", 5, None, None, None, None)
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        mem.recall("_", 5, None, None, None, None)
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn keyword_relevance_is_query_coverage_not_corpus_dependent_bm25() {
    let dir = tempfile::tempdir().unwrap();
    let mem = SqliteMemory::new(dir.path()).unwrap();
    mem.store("preference", "Rust programming", MemoryCategory::Core, None)
        .await
        .unwrap();
    assert_eq!(
        recall(&mem, "programming Rust", SearchMode::Bm25, 0.8, 5)
            .await
            .len(),
        1
    );
    assert!(
        recall(&mem, "Rust weather travel music", SearchMode::Bm25, 0.3, 5)
            .await
            .is_empty()
    );
    assert!(
        recall(&mem, "  ", SearchMode::Bm25, 0.3, 5)
            .await
            .is_empty()
    );
    assert_eq!(
        mem.recall("", 5, None, None, None, None)
            .await
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        recall(&mem, "\"Rust\"", SearchMode::Bm25, 0.3, 5)
            .await
            .len(),
        1
    );
}

struct TestEmbedding;

#[async_trait]
impl EmbeddingProvider for TestEmbedding {
    fn name(&self) -> &str {
        "test-relevance"
    }
    fn dimensions(&self) -> usize {
        2
    }
    async fn embed(&self, texts: &[&str]) -> anyhow::Result<Vec<Vec<f32>>> {
        Ok(texts
            .iter()
            .map(|text| {
                if text.contains("weak") {
                    vec![0.2, 0.98]
                } else if text.contains("semantic") {
                    vec![0.9, 0.436]
                } else {
                    vec![1.0, 0.0]
                }
            })
            .collect())
    }
}

#[tokio::test]
async fn hybrid_filters_relevance_before_limiting_without_thresholding_rank_scores() {
    for strategy in [
        MergeStrategy::Rrf { k: 60 },
        MergeStrategy::Weighted {
            vector_weight: 0.1,
            keyword_weight: 0.9,
        },
    ] {
        let dir = tempfile::tempdir().unwrap();
        let mem = SqliteMemory::with_embedder(
            dir.path(),
            Arc::new(TestEmbedding),
            0.7,
            0.3,
            100,
            None,
            SearchMode::Hybrid,
            strategy,
            false,
        )
        .unwrap();
        mem.store("weak", "weak Rust", MemoryCategory::Core, None)
            .await
            .unwrap();
        mem.store(
            "relevant",
            "semantic preference",
            MemoryCategory::Core,
            None,
        )
        .await
        .unwrap();
        let results = recall(
            &mem,
            "Rust weather travel music",
            SearchMode::Hybrid,
            0.8,
            1,
        )
        .await;
        assert_eq!(
            results.len(),
            1,
            "RRF rank scale must not discard semantic matches"
        );
        assert_eq!(
            results[0].key, "relevant",
            "reject the weak lexical match before applying limit"
        );
        assert!(results[0].score.unwrap() > 0.8);
        assert_eq!(
            recall(
                &mem,
                "Rust weather travel music",
                SearchMode::Embedding,
                0.8,
                1
            )
            .await[0]
                .key,
            "relevant"
        );
        mem.store("both", "Rust programming", MemoryCategory::Core, None)
            .await
            .unwrap();
        let fused = recall(&mem, "programming Rust", SearchMode::Hybrid, 0.8, 1).await;
        assert_eq!(
            fused[0].key, "both",
            "a match in both lists must survive the RRF score scale"
        );
        assert!(fused[0].score.unwrap() >= 0.8);
    }
}

#[tokio::test]
async fn embedding_mode_never_falls_back_to_lexical_matches_below_threshold() {
    let dir = tempfile::tempdir().unwrap();
    let mem = SqliteMemory::with_embedder(
        dir.path(),
        Arc::new(TestEmbedding),
        0.7,
        0.3,
        100,
        None,
        SearchMode::Embedding,
        MergeStrategy::default(),
        false,
    )
    .unwrap();
    mem.store("weak", "weak Rust", MemoryCategory::Core, None)
        .await
        .unwrap();
    assert!(
        recall(&mem, "Rust", SearchMode::Embedding, 0.8, 5)
            .await
            .is_empty()
    );
}
