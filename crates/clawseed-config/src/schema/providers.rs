//! Provider configuration.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// Provider configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProvidersConfig {
    /// Default provider name (e.g. "openai", "anthropic", "gemini", "bedrock").
    #[serde(default)]
    pub default: Option<String>,

    /// Default model name.
    #[serde(default)]
    pub default_model: Option<String>,

    /// Default API key.
    #[serde(default)]
    pub default_api_key: Option<String>,

    /// Default base URL for the provider.
    #[serde(default)]
    pub default_base_url: Option<String>,

    /// Default timeout in seconds.
    #[serde(default)]
    pub default_timeout_secs: Option<u64>,

    /// Extra HTTP headers to send with every request.
    #[serde(default)]
    pub extra_headers: HashMap<String, String>,

    /// Named provider configurations.
    #[serde(default)]
    pub models: HashMap<String, ModelProviderConfig>,

    /// Model routing rules.
    #[serde(default)]
    pub model_routes: Vec<ModelRouteConfig>,

    /// Embedding model routing.
    #[serde(default)]
    pub embedding_routes: Vec<EmbeddingRouteConfig>,

    /// Fallback provider when the primary fails.
    #[serde(default)]
    pub fallback: Option<String>,

    /// Reasoning effort level (low/medium/high).
    #[serde(default)]
    pub reasoning_effort: Option<String>,

    /// Enable reasoning/thinking mode.
    #[serde(default)]
    pub reasoning_enabled: Option<bool>,
}

/// Per-model provider configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelProviderConfig {
    #[serde(default)]
    pub api_key: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub base_url: Option<String>,
    #[serde(default)]
    pub api_path: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub temperature: Option<f64>,
    #[serde(default)]
    pub timeout_secs: Option<u64>,
    #[serde(default)]
    pub extra_headers: HashMap<String, String>,
    #[serde(default)]
    pub wire_api: Option<String>,
    #[serde(default)]
    pub max_tokens: Option<u32>,
    #[serde(default)]
    pub provider_extra: Option<serde_json::Value>,
    /// When true, system messages are merged into the first user message.
    #[serde(default)]
    pub merge_system_into_user: bool,
}

/// A model routing rule.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelRouteConfig {
    pub hint: String,
    pub provider: String,
    pub model: String,
    #[serde(default)]
    pub api_key: Option<String>,
}

/// An embedding model routing rule.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingRouteConfig {
    pub hint: String,
    pub provider: String,
    pub model: String,
    #[serde(default)]
    pub dimensions: Option<usize>,
    #[serde(default)]
    pub api_key: Option<String>,
}

impl ProvidersConfig {
    /// Return the fallback provider configuration, if any is defined.
    ///
    /// If a `fallback` provider name is set and a matching entry exists in
    /// `models`, returns a reference to that entry. Otherwise returns `None`.
    pub fn fallback_provider(&self) -> Option<&ModelProviderConfig> {
        self.fallback
            .as_deref()
            .and_then(|name| self.models.get(name))
    }
}
