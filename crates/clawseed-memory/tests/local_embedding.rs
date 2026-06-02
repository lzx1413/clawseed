//! Integration test for local ONNX embedding using bundled model files.

#[cfg(feature = "local-embedding")]
mod local_embedding {
    use clawseed_memory::embeddings::{EmbeddingProvider, LocalOnnxEmbedding};

    fn model_dir() -> std::path::PathBuf {
        // Relative to workspace root: clients/android/app/src/main/assets/models/gte-multilingual-base
        let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".into());
        let workspace_root = std::path::Path::new(&manifest_dir)
            .parent()
            .and_then(|p| p.parent())
            .expect("expected crates/clawseed-memory to be 2 levels deep from workspace root");
        workspace_root.join("clients/android/app/src/main/assets/models/gte-multilingual-base")
    }

    /// Setup: download ORT .so if missing, then preload it for load-dynamic feature.
    async fn ensure_ort() {
        let dir = model_dir();
        // If ORT .so is not in model_dir, download it via model_cache
        let ort_path = dir.join("libonnxruntime.so");
        if !ort_path.exists() {
            clawseed_memory::model_cache::ensure_ort_lib_available(&dir)
                .await
                .expect("Failed to download ONNX Runtime shared library");
        }
        if ort_path.exists() {
            ort::util::preload_dylib(&ort_path).unwrap();
        }
    }

    #[tokio::test]
    async fn load_model_and_embed_single_text() {
        ensure_ort().await;
        let dir = model_dir();
        let embedder = LocalOnnxEmbedding::new(&dir, None).unwrap_or_else(|e| {
            panic!(
                "Failed to load local ONNX embedding model from {}: {e}",
                dir.display()
            )
        });

        assert_eq!(embedder.name(), "local-onnx");
        assert_eq!(embedder.dimensions(), 768);

        let result = embedder.embed_one("hello world").await.unwrap();
        assert_eq!(result.len(), 768);

        // L2-normalized vector: norm should be ~1.0
        let norm: f32 = result.iter().map(|v| v * v).sum::<f32>().sqrt();
        assert!(
            norm > 0.99 && norm < 1.01,
            "L2 norm = {norm}, expected ~1.0"
        );
    }

    #[tokio::test]
    async fn embed_batch_texts() {
        ensure_ort().await;
        let dir = model_dir();
        let embedder = LocalOnnxEmbedding::new(&dir, None).unwrap();

        let texts = vec!["你好世界", "hello world", "机器学习是人工智能的一个分支"];
        let results = embedder.embed(&texts).await.unwrap();
        assert_eq!(results.len(), 3);
        for v in &results {
            assert_eq!(v.len(), 768);
            let norm: f32 = v.iter().map(|f| f * f).sum::<f32>().sqrt();
            assert!(norm > 0.99 && norm < 1.01);
        }

        // Similar texts should have higher cosine similarity than dissimilar ones
        let sim_similar = cosine_similarity(&results[0], &results[1]);
        let sim_different = cosine_similarity(&results[0], &results[2]);
        assert!(
            sim_similar > sim_different,
            "Similar texts should have higher similarity: similar={sim_similar}, different={sim_different}"
        );
    }

    #[tokio::test]
    async fn embed_chinese_text() {
        ensure_ort().await;
        let dir = model_dir();
        let embedder = LocalOnnxEmbedding::new(&dir, None).unwrap();

        let result = embedder.embed_one("这是一个测试句子").await.unwrap();
        assert_eq!(result.len(), 768);
        let norm: f32 = result.iter().map(|v| v * v).sum::<f32>().sqrt();
        assert!(norm > 0.99 && norm < 1.01, "L2 norm = {norm}");
    }

    #[tokio::test]
    async fn empty_batch_returns_empty() {
        ensure_ort().await;
        let dir = model_dir();
        let embedder = LocalOnnxEmbedding::new(&dir, None).unwrap();
        let results = embedder.embed(&[]).await.unwrap();
        assert!(results.is_empty());
    }

    fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
        let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
        let norm_a: f32 = a.iter().map(|v| v * v).sum::<f32>().sqrt();
        let norm_b: f32 = b.iter().map(|v| v * v).sum::<f32>().sqrt();
        if norm_a == 0.0 || norm_b == 0.0 {
            0.0
        } else {
            dot / (norm_a * norm_b)
        }
    }
}
