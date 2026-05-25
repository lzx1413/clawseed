use async_trait::async_trait;
use std::sync::Arc;

/// Trait for embedding providers — convert text to vectors
#[async_trait]
pub trait EmbeddingProvider: Send + Sync {
    /// Provider name
    fn name(&self) -> &str;

    /// Embedding dimensions
    fn dimensions(&self) -> usize;

    /// Embed a batch of texts into vectors
    async fn embed(&self, texts: &[&str]) -> anyhow::Result<Vec<Vec<f32>>>;

    /// Embed a single text
    async fn embed_one(&self, text: &str) -> anyhow::Result<Vec<f32>> {
        let mut results = self.embed(&[text]).await?;
        results
            .pop()
            .ok_or_else(|| anyhow::anyhow!("Empty embedding result"))
    }
}

// ── Noop provider (keyword-only fallback) ────────────────────

pub struct NoopEmbedding;

#[async_trait]
impl EmbeddingProvider for NoopEmbedding {
    fn name(&self) -> &str {
        "none"
    }

    fn dimensions(&self) -> usize {
        0
    }

    async fn embed(&self, _texts: &[&str]) -> anyhow::Result<Vec<Vec<f32>>> {
        Ok(Vec::new())
    }
}

// ── OpenAI-compatible embedding provider ─────────────────────

pub struct OpenAiEmbedding {
    base_url: String,
    api_key: String,
    model: String,
    dims: usize,
}

impl OpenAiEmbedding {
    pub fn new(base_url: &str, api_key: &str, model: &str, dims: usize) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            api_key: api_key.to_string(),
            model: model.to_string(),
            dims,
        }
    }

    fn http_client(&self) -> reqwest::Client {
        clawseed_config::schema::build_runtime_proxy_client("memory.embeddings")
    }

    fn has_explicit_api_path(&self) -> bool {
        let Ok(url) = reqwest::Url::parse(&self.base_url) else {
            return false;
        };

        let path = url.path().trim_end_matches('/');
        !path.is_empty() && path != "/"
    }

    fn has_embeddings_endpoint(&self) -> bool {
        let Ok(url) = reqwest::Url::parse(&self.base_url) else {
            return false;
        };

        url.path().trim_end_matches('/').ends_with("/embeddings")
    }

    fn embeddings_url(&self) -> String {
        if self.has_embeddings_endpoint() {
            return self.base_url.clone();
        }

        if self.has_explicit_api_path() {
            format!("{}/embeddings", self.base_url)
        } else {
            format!("{}/v1/embeddings", self.base_url)
        }
    }
}

#[async_trait]
impl EmbeddingProvider for OpenAiEmbedding {
    fn name(&self) -> &str {
        "openai"
    }

    fn dimensions(&self) -> usize {
        self.dims
    }

    async fn embed(&self, texts: &[&str]) -> anyhow::Result<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }

        let body = serde_json::json!({
            "model": self.model,
            "input": texts,
        });

        let resp = self
            .http_client()
            .post(self.embeddings_url())
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            anyhow::bail!("Embedding API error {status}: {text}");
        }

        let json: serde_json::Value = resp.json().await?;
        let data = json
            .get("data")
            .and_then(|d| d.as_array())
            .ok_or_else(|| anyhow::anyhow!("Invalid embedding response: missing 'data'"))?;

        let mut embeddings = Vec::with_capacity(data.len());
        for item in data {
            let embedding = item
                .get("embedding")
                .and_then(|e| e.as_array())
                .ok_or_else(|| anyhow::anyhow!("Invalid embedding item"))?;

            #[allow(clippy::cast_possible_truncation)]
            let vec: Vec<f32> = embedding
                .iter()
                .filter_map(|v| v.as_f64().map(|f| f as f32))
                .collect();

            embeddings.push(vec);
        }

        Ok(embeddings)
    }
}

// ── Local ONNX embedding provider ────────────────────────────

#[cfg(feature = "local-embedding")]
pub struct LocalOnnxEmbedding {
    session: std::sync::Mutex<ort::session::Session>,
    tokenizer: tokenizers::Tokenizer,
    dims: usize,
}

#[cfg(feature = "local-embedding")]
impl LocalOnnxEmbedding {
    /// Create a local ONNX embedding provider from model files in `model_dir`.
    ///
    /// `model_dir` should contain `model.onnx` (or `model_int8.onnx`) and
    /// `tokenizer.json`. The `dims` parameter overrides the detected
    /// dimension size; when None, defaults to 768 (gte-multilingual-base).
    pub fn new(model_dir: &std::path::Path, dims_override: Option<usize>) -> anyhow::Result<Self> {
        // Prefer INT8 quantized model for smaller size and faster inference.
        let onnx_path = model_dir.join("model_int8.onnx");
        let onnx_path = if onnx_path.exists() {
            onnx_path
        } else {
            model_dir.join("model.onnx")
        };

        if !onnx_path.exists() {
            anyhow::bail!(
                "No ONNX model found in {} (expected model.onnx or model_int8.onnx)",
                model_dir.display()
            );
        }

        let tokenizer_path = model_dir.join("tokenizer.json");
        if !tokenizer_path.exists() {
            anyhow::bail!("No tokenizer.json found in {}", model_dir.display());
        }

        tracing::info!(
            "Loading local ONNX embedding model from {}",
            onnx_path.display()
        );

        let session = ort::session::Session::builder()?.commit_from_file(&onnx_path)?;

        let tokenizer = tokenizers::Tokenizer::from_file(&tokenizer_path)
            .map_err(|e| anyhow::anyhow!("Failed to load tokenizer: {e}"))?;

        let dims = dims_override.unwrap_or(768);

        Ok(Self {
            session: std::sync::Mutex::new(session),
            tokenizer,
            dims,
        })
    }
}

#[cfg(feature = "local-embedding")]
#[async_trait]
impl EmbeddingProvider for LocalOnnxEmbedding {
    fn name(&self) -> &str {
        "local-onnx"
    }

    fn dimensions(&self) -> usize {
        self.dims
    }

    async fn embed(&self, texts: &[&str]) -> anyhow::Result<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }

        // Detect model input/output names from the session
        let (has_token_type_ids, has_sentence_embedding) = {
            let session = self.session.lock().unwrap();
            let input_names: Vec<&str> = session.inputs().iter().map(|o| o.name()).collect();
            let output_names: Vec<&str> = session.outputs().iter().map(|o| o.name()).collect();
            let has_tti = input_names.contains(&"token_type_ids");
            let has_se = output_names.contains(&"sentence_embedding");
            (has_tti, has_se)
        };

        // Tokenize all inputs in batch
        let texts_vec: Vec<String> = texts.iter().map(|s| s.to_string()).collect();
        let encodings = self
            .tokenizer
            .encode_batch(texts_vec, true)
            .map_err(|e| anyhow::anyhow!("Tokenization error: {e}"))?;

        let mut results = Vec::with_capacity(texts.len());

        for encoding in encodings {
            let token_ids = encoding.get_ids();
            let attention_mask = encoding.get_attention_mask();

            // Convert to i64 for ONNX input
            let input_ids: Vec<i64> = token_ids.iter().map(|t| i64::from(*t)).collect();
            let attn_mask: Vec<i64> = attention_mask.iter().map(|m| i64::from(*m)).collect();
            let token_type_ids: Vec<i64> = vec![0; input_ids.len()];

            let seq_len = input_ids.len();

            // Build ONNX inputs — only include inputs the model actually expects
            let input_ids_arr = ndarray::Array2::from_shape_vec((1, seq_len), input_ids)?;
            let attn_mask_arr = ndarray::Array2::from_shape_vec((1, seq_len), attn_mask)?;
            let input_ids_value = ort::value::Value::from_array(input_ids_arr)?;
            let attn_mask_value = ort::value::Value::from_array(attn_mask_arr)?;

            let mut inputs_vec: Vec<(String, ort::session::SessionInputValue)> = vec![
                (
                    "input_ids".into(),
                    ort::session::SessionInputValue::from(input_ids_value),
                ),
                (
                    "attention_mask".into(),
                    ort::session::SessionInputValue::from(attn_mask_value),
                ),
            ];
            if has_token_type_ids {
                let token_type_ids_arr =
                    ndarray::Array2::from_shape_vec((1, seq_len), token_type_ids)?;
                let token_type_ids_value = ort::value::Value::from_array(token_type_ids_arr)?;
                inputs_vec.push((
                    "token_type_ids".into(),
                    ort::session::SessionInputValue::from(token_type_ids_value),
                ));
            }

            let inputs = ort::session::SessionInputs::from(inputs_vec);

            let mut session = self.session.lock().unwrap();
            let outputs = session.run(inputs)?;

            if has_sentence_embedding {
                // Model provides pre-computed sentence_embedding (already L2-normalized)
                let (_shape, sentence_emb) =
                    outputs["sentence_embedding"].try_extract_tensor::<f32>()?;
                results.push(sentence_emb.to_vec());
            } else {
                // BERT-style: last_hidden_state → manual mean pooling + L2 normalization
                let (_shape, last_hidden) =
                    outputs["last_hidden_state"].try_extract_tensor::<f32>()?;
                let hidden_dim = self.dims;
                let mut pooled = vec![0.0f32; hidden_dim];
                let mut mask_sum = 0.0f32;

                for (t, &mask_raw) in attention_mask.iter().enumerate() {
                    let mask_val = mask_raw as f32;
                    if mask_val > 0.0 {
                        let offset = t * hidden_dim;
                        for (d, pooled_item) in pooled.iter_mut().enumerate().take(hidden_dim) {
                            *pooled_item += last_hidden[offset + d] * mask_val;
                        }
                        mask_sum += mask_val;
                    }
                }

                if mask_sum > 0.0 {
                    for pooled_item in &mut pooled {
                        *pooled_item /= mask_sum;
                    }
                }

                let norm = pooled.iter().map(|v| v * v).sum::<f32>().sqrt();
                if norm > 0.0 {
                    for pooled_item in &mut pooled {
                        *pooled_item /= norm;
                    }
                }

                results.push(pooled);
            }
        }

        Ok(results)
    }
}

// ── Factory ──────────────────────────────────────────────────

pub fn create_embedding_provider(
    provider: &str,
    api_key: Option<&str>,
    model: &str,
    dims: usize,
) -> Box<dyn EmbeddingProvider> {
    match provider {
        "openai" => {
            let key = api_key.unwrap_or("");
            Box::new(OpenAiEmbedding::new(
                "https://api.openai.com",
                key,
                model,
                dims,
            ))
        }
        "openrouter" => {
            let key = api_key.unwrap_or("");
            Box::new(OpenAiEmbedding::new(
                "https://openrouter.ai/api/v1",
                key,
                model,
                dims,
            ))
        }
        name if name.starts_with("custom:") => {
            let base_url = name.strip_prefix("custom:").unwrap_or("");
            let key = api_key.unwrap_or("");
            Box::new(OpenAiEmbedding::new(base_url, key, model, dims))
        }
        _ => Box::new(NoopEmbedding),
    }
}

/// Resolve an embedding provider from config.
///
/// Priority:
/// 1. `memory.embedding_provider = "local"` → LocalOnnxEmbedding (requires `local-embedding` feature)
/// 2. `memory.embedding_provider` set to remote provider → OpenAiEmbedding
/// 3. `providers.embedding_routes` non-empty → OpenAiEmbedding from first route
/// 4. None of the above → NoopEmbedding (backward compat)
pub async fn resolve_embedding_provider(
    config: &clawseed_config::schema::MemoryConfig,
    providers_config: &clawseed_config::schema::ProvidersConfig,
    #[cfg(feature = "local-embedding")] workspace_dir: &std::path::Path,
    #[cfg(not(feature = "local-embedding"))] _workspace_dir: &std::path::Path,
) -> anyhow::Result<std::sync::Arc<dyn EmbeddingProvider>> {
    let provider_name = config.embedding_provider.as_deref();

    match provider_name {
        Some("local") => {
            #[cfg(feature = "local-embedding")]
            {
                let model_name = config
                    .embedding_model
                    .as_deref()
                    .unwrap_or("gte-multilingual-base");
                let model_dir = workspace_dir.join("models").join(model_name);

                // Ensure model files are available (async download)
                let model_dir =
                    crate::model_cache::ensure_model_available(model_name, &model_dir).await?;

                // Ensure ONNX Runtime shared library is available.
                // On desktop: downloaded by model_cache to model_dir.
                // On Android: bundled in nativeLibraryDir, ORT_DYLIB_PATH env var set by gateway.
                #[cfg(not(target_os = "android"))]
                crate::model_cache::ensure_ort_lib_available(&model_dir).await?;

                // Preload the .so so ort's load-dynamic can find it via dlopen.
                // On Android, ORT_DYLIB_PATH is set externally — preload is skipped.
                let ort_lib_path = model_dir.join("libonnxruntime.so");
                if ort_lib_path.exists() {
                    ort::util::preload_dylib(&ort_lib_path)
                        .map_err(|e| anyhow::anyhow!("Failed to preload ONNX Runtime: {e}"))?;
                }

                let embedder = LocalOnnxEmbedding::new(&model_dir, config.embedding_dims)?;
                tracing::info!(
                    "Local embedding provider resolved: {} (dims={})",
                    embedder.name(),
                    embedder.dimensions()
                );
                Ok(std::sync::Arc::new(embedder))
            }

            #[cfg(not(feature = "local-embedding"))]
            {
                anyhow::bail!(
                    "Local embedding requested but the 'local-embedding' feature is not enabled. \
                     Rebuild with --features local-embedding or use a remote embedding provider."
                );
            }
        }
        Some(provider)
            if provider.starts_with("custom:")
                || provider == "openai"
                || provider == "openrouter" =>
        {
            let model = config
                .embedding_model
                .as_deref()
                .unwrap_or("text-embedding-3-small");
            let dims = config.embedding_dims.unwrap_or(1536);
            let embedder: Box<dyn EmbeddingProvider> = create_embedding_provider(
                provider,
                providers_config.default_api_key.as_deref(),
                model,
                dims,
            );
            // Convert Box<dyn> to Arc<dyn> via Arc from Box
            let embedder: Arc<dyn EmbeddingProvider> = Arc::from(embedder);
            tracing::info!(
                "Remote embedding provider resolved: {} model={} dims={}",
                embedder.name(),
                model,
                dims
            );
            Ok(embedder)
        }
        Some(other) => {
            tracing::warn!(
                "Unknown embedding provider '{}', falling back to NoopEmbedding",
                other
            );
            Ok(std::sync::Arc::new(NoopEmbedding))
        }
        None => {
            // Check if embedding_routes are configured (legacy path)
            if let Some(route) = providers_config.embedding_routes.first() {
                let dims = route.dimensions.unwrap_or(1536);
                let embedder: Box<dyn EmbeddingProvider> = create_embedding_provider(
                    &route.provider,
                    route.api_key.as_deref(),
                    &route.model,
                    dims,
                );
                let embedder: Arc<dyn EmbeddingProvider> = Arc::from(embedder);
                tracing::info!(
                    "Embedding provider resolved from route: {} model={} dims={}",
                    embedder.name(),
                    route.model,
                    dims
                );
                return Ok(embedder);
            }
            Ok(std::sync::Arc::new(NoopEmbedding))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noop_name() {
        let p = NoopEmbedding;
        assert_eq!(p.name(), "none");
        assert_eq!(p.dimensions(), 0);
    }

    #[tokio::test]
    async fn noop_embed_returns_empty() {
        let p = NoopEmbedding;
        let result = p.embed(&["hello"]).await.unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn factory_none() {
        let p = create_embedding_provider("none", None, "model", 1536);
        assert_eq!(p.name(), "none");
    }

    #[test]
    fn factory_openai() {
        let p = create_embedding_provider("openai", Some("key"), "text-embedding-3-small", 1536);
        assert_eq!(p.name(), "openai");
        assert_eq!(p.dimensions(), 1536);
    }

    #[test]
    fn factory_openrouter() {
        let p = create_embedding_provider(
            "openrouter",
            Some("sk-or-test"),
            "openai/text-embedding-3-small",
            1536,
        );
        assert_eq!(p.name(), "openai"); // uses OpenAiEmbedding internally
        assert_eq!(p.dimensions(), 1536);
    }

    #[test]
    fn factory_custom_url() {
        let p = create_embedding_provider("custom:http://localhost:1234", None, "model", 768);
        assert_eq!(p.name(), "openai"); // uses OpenAiEmbedding internally
        assert_eq!(p.dimensions(), 768);
    }

    // ── Edge cases ───────────────────────────────────────────────

    #[tokio::test]
    async fn noop_embed_one_returns_error() {
        let p = NoopEmbedding;
        // embed returns empty vec → pop() returns None → error
        let result = p.embed_one("hello").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn noop_embed_empty_batch() {
        let p = NoopEmbedding;
        let result = p.embed(&[]).await.unwrap();
        assert!(result.is_empty());
    }

    #[tokio::test]
    async fn noop_embed_multiple_texts() {
        let p = NoopEmbedding;
        let result = p.embed(&["a", "b", "c"]).await.unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn factory_empty_string_returns_noop() {
        let p = create_embedding_provider("", None, "model", 1536);
        assert_eq!(p.name(), "none");
    }

    #[test]
    fn factory_unknown_provider_returns_noop() {
        let p = create_embedding_provider("cohere", None, "model", 1536);
        assert_eq!(p.name(), "none");
    }

    #[test]
    fn factory_custom_empty_url() {
        // "custom:" with no URL — should still construct without panic
        let p = create_embedding_provider("custom:", None, "model", 768);
        assert_eq!(p.name(), "openai");
    }

    #[test]
    fn factory_openai_no_api_key() {
        let p = create_embedding_provider("openai", None, "text-embedding-3-small", 1536);
        assert_eq!(p.name(), "openai");
        assert_eq!(p.dimensions(), 1536);
    }

    #[test]
    fn openai_trailing_slash_stripped() {
        let p = OpenAiEmbedding::new("https://api.openai.com/", "key", "model", 1536);
        assert_eq!(p.base_url, "https://api.openai.com");
    }

    #[test]
    fn openai_dimensions_custom() {
        let p = OpenAiEmbedding::new("http://localhost", "k", "m", 384);
        assert_eq!(p.dimensions(), 384);
    }

    #[test]
    fn embeddings_url_openrouter() {
        let p = OpenAiEmbedding::new(
            "https://openrouter.ai/api/v1",
            "key",
            "openai/text-embedding-3-small",
            1536,
        );
        assert_eq!(
            p.embeddings_url(),
            "https://openrouter.ai/api/v1/embeddings"
        );
    }

    #[test]
    fn embeddings_url_standard_openai() {
        let p = OpenAiEmbedding::new("https://api.openai.com", "key", "model", 1536);
        assert_eq!(p.embeddings_url(), "https://api.openai.com/v1/embeddings");
    }

    #[test]
    fn embeddings_url_base_with_v1_no_duplicate() {
        let p = OpenAiEmbedding::new("https://api.example.com/v1", "key", "model", 1536);
        assert_eq!(p.embeddings_url(), "https://api.example.com/v1/embeddings");
    }

    #[test]
    fn embeddings_url_non_v1_api_path_uses_raw_suffix() {
        let p = OpenAiEmbedding::new(
            "https://api.example.com/api/coding/v3",
            "key",
            "model",
            1536,
        );
        assert_eq!(
            p.embeddings_url(),
            "https://api.example.com/api/coding/v3/embeddings"
        );
    }

    #[test]
    fn embeddings_url_custom_full_endpoint() {
        let p = OpenAiEmbedding::new(
            "https://my-api.example.com/api/v2/embeddings",
            "key",
            "model",
            1536,
        );
        assert_eq!(
            p.embeddings_url(),
            "https://my-api.example.com/api/v2/embeddings"
        );
    }

    #[tokio::test]
    async fn resolve_noop_when_no_provider_configured() {
        let config = clawseed_config::schema::MemoryConfig::default();
        let providers = clawseed_config::schema::ProvidersConfig::default();
        let dir = std::path::PathBuf::from("/tmp/test");
        let provider = resolve_embedding_provider(&config, &providers, &dir)
            .await
            .unwrap();
        assert_eq!(provider.name(), "none");
        assert_eq!(provider.dimensions(), 0);
    }

    #[tokio::test]
    async fn resolve_from_embedding_route() {
        let config = clawseed_config::schema::MemoryConfig::default();
        let mut providers = clawseed_config::schema::ProvidersConfig::default();
        providers
            .embedding_routes
            .push(clawseed_config::schema::EmbeddingRouteConfig {
                hint: "default".into(),
                provider: "openai".into(),
                model: "text-embedding-3-small".into(),
                dimensions: Some(1536),
                api_key: Some("sk-test".into()),
            });
        let dir = std::path::PathBuf::from("/tmp/test");
        let provider = resolve_embedding_provider(&config, &providers, &dir)
            .await
            .unwrap();
        assert_eq!(provider.name(), "openai");
        assert_eq!(provider.dimensions(), 1536);
    }
}
