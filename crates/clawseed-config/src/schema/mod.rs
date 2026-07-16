//! Configuration schema — the main `Config` struct and sub-configs.

mod agent;
mod gateway;
mod providers;
mod storage;

pub use agent::*;
pub use gateway::*;
pub use providers::*;
pub use storage::*;

use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

pub use clawseed_api::memory_traits::{ConflictMode, MergeStrategy, SearchMode};

/// Multimodal (image) configuration for provider requests.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultimodalConfig {
    /// Maximum number of images allowed per conversation.
    #[serde(default = "default_max_images")]
    pub max_images: usize,

    /// Maximum image file size in megabytes.
    #[serde(default = "default_max_image_size_mb")]
    pub max_image_size_mb: usize,

    /// Whether remote image URLs may be fetched.
    #[serde(default)]
    pub allow_remote_fetch: bool,
}

fn default_max_images() -> usize {
    4
}
fn default_max_image_size_mb() -> usize {
    5
}

impl Default for MultimodalConfig {
    fn default() -> Self {
        Self {
            max_images: default_max_images(),
            max_image_size_mb: default_max_image_size_mb(),
            allow_remote_fetch: false,
        }
    }
}

impl MultimodalConfig {
    /// Return the effective `(max_images, max_image_size_mb)` limits.
    ///
    /// Guarantees at least 1 image and 1 MB so that zero-config
    /// doesn't silently disable multimodal entirely.
    pub fn effective_limits(&self) -> (usize, usize) {
        (self.max_images.max(1), self.max_image_size_mb.max(1))
    }
}

/// Build a `reqwest::Client` for a given label with sensible default timeouts.
///
/// Convenience wrapper around [`build_runtime_proxy_client_with_timeouts`]
/// using 30s request timeout and 10s connect timeout.
pub fn build_runtime_proxy_client(label: &str) -> reqwest::Client {
    build_runtime_proxy_client_with_timeouts(label, 30, 10)
}

/// Build a `reqwest::Client` with the given timeout settings.
///
/// For now this is a simple stub that creates a standard client with the
/// specified connect and request timeouts. Proxy support and other
/// advanced configuration will be added later.
pub fn build_runtime_proxy_client_with_timeouts(
    _label: &str,
    timeout_secs: u64,
    connect_timeout_secs: u64,
) -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(timeout_secs))
        .connect_timeout(std::time::Duration::from_secs(connect_timeout_secs))
        .build()
        .unwrap_or_else(|_| reqwest::Client::new())
}

/// Apply runtime proxy settings to a reqwest client builder.
///
/// For now this is a no-op stub that returns the builder unchanged.
/// Proxy support will be added later.
pub fn apply_runtime_proxy_to_builder(
    builder: reqwest::ClientBuilder,
    _label: &str,
) -> reqwest::ClientBuilder {
    builder
}

/// Top-level configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Path this config was loaded from (not serialized; set after loading).
    #[serde(skip)]
    pub config_path: PathBuf,

    #[serde(default)]
    pub workspace_dir: PathBuf,

    #[serde(default)]
    pub providers: ProvidersConfig,

    #[serde(default)]
    pub agent: AgentConfig,

    #[serde(default)]
    pub gateway: GatewayConfig,

    #[serde(default)]
    pub storage: StorageConfig,

    #[serde(default)]
    pub reliability: ReliabilityConfig,

    #[serde(default)]
    pub secrets: SecretsConfig,

    #[serde(default)]
    pub runtime: RuntimeConfig,

    #[serde(default)]
    pub memory: MemoryConfig,

    /// Structured user modeling configuration.
    #[serde(default)]
    pub user_model: UserModelConfig,

    #[serde(default)]
    pub autonomy: AutonomyConfig,

    #[serde(default)]
    pub cron: CronConfig,

    #[serde(default)]
    pub scheduler: SchedulerConfig,

    #[serde(default)]
    pub backup: BackupConfig,

    /// Composio integration configuration.
    #[serde(default)]
    pub composio: ComposioConfig,

    /// MCP (Model Context Protocol) configuration.
    #[serde(default)]
    pub mcp: McpConfig,

    /// Hooks configuration.
    #[serde(default)]
    pub hooks: HooksConfig,

    /// Tunnel configuration.
    #[serde(default)]
    pub tunnel: TunnelConfig,

    /// Nodes configuration.
    #[serde(default)]
    pub nodes: NodesConfig,

    /// Cost tracking configuration.
    #[serde(default)]
    pub cost: CostConfig,

    /// Channel configuration.
    #[serde(default)]
    pub channels: ChannelsConfig,

    /// Browser configuration.
    #[serde(default)]
    pub browser: BrowserConfig,

    /// HTTP request configuration.
    #[serde(default)]
    pub http_request: HttpRequestConfig,

    /// Web fetch configuration.
    #[serde(default)]
    pub web_fetch: WebFetchConfig,

    /// Transcription configuration.
    #[serde(default)]
    pub transcription: TranscriptionConfig,

    /// Web search configuration.
    #[serde(default)]
    pub web_search: WebSearchConfig,

    /// Agents map (named agent configs).
    #[serde(default)]
    pub agents: std::collections::HashMap<String, AgentEntryConfig>,

    /// Locale override (e.g. "en-US").
    #[serde(default)]
    pub locale: Option<String>,

    /// Identity / persona configuration.
    #[serde(default)]
    pub identity: IdentityConfig,

    /// Skill system configuration.
    #[serde(default)]
    pub skills: SkillsConfig,
}

/// Secrets / encryption configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecretsConfig {
    /// Whether to encrypt stored secrets at rest.
    #[serde(default = "default_true")]
    pub encrypt: bool,
}

fn default_true() -> bool {
    true
}

/// Memory configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryConfig {
    /// Backend type: "sqlite", "none", etc.
    #[serde(default = "default_memory_backend")]
    pub backend: String,
    /// Auto-save user messages to memory.
    #[serde(default)]
    pub auto_save: bool,
    /// Minimum relevance score for recall.
    #[serde(default = "default_min_relevance")]
    pub min_relevance_score: f64,
    /// Response cache enabled.
    #[serde(default)]
    pub response_cache_enabled: bool,
    /// Response cache TTL in minutes.
    #[serde(default = "default_cache_ttl")]
    pub response_cache_ttl_minutes: u64,
    /// Response cache max entries.
    #[serde(default = "default_cache_max")]
    pub response_cache_max_entries: usize,
    /// Response cache hot entries.
    #[serde(default = "default_cache_hot")]
    pub response_cache_hot_entries: usize,
    /// Memory namespace for isolation.
    #[serde(default)]
    pub namespace: Option<String>,
    /// Qdrant vector database configuration.
    #[serde(default)]
    pub qdrant: QdrantConfig,
    /// Run periodic hygiene pass to prune stale Conversation/Daily entries.
    #[serde(default = "default_hygiene_enabled")]
    pub hygiene_enabled: bool,
    /// Drop Conversation rows older than this many days. Core memories are never pruned.
    #[serde(default = "default_conversation_retention_days")]
    pub conversation_retention_days: u32,
    /// Enable periodic export of Core memories to MEMORY_SNAPSHOT.md.
    #[serde(default)]
    pub snapshot_enabled: bool,
    /// Auto-hydrate from MEMORY_SNAPSHOT.md when brain.db is missing.
    #[serde(default = "default_true_val")]
    pub auto_hydrate: bool,
    /// Jaccard similarity threshold for conflict detection (0.0–1.0).
    #[serde(default = "default_conflict_threshold")]
    pub conflict_threshold: f64,
    /// Conflict detection mode: Jaccard (word overlap only) or
    /// Combined (weighted Jaccard + cosine + BM25 overlap).
    /// None defaults to Combined { jaccard_w: 0.4, cosine_w: 0.4, bm25_w: 0.2 }.
    #[serde(default)]
    pub conflict_mode: Option<ConflictMode>,
    /// Auto-recall relevant memories at the start of each turn.
    #[serde(default = "default_true_val")]
    pub auto_recall: bool,
    /// Number of memories to auto-recall per turn.
    #[serde(default = "default_auto_recall_limit")]
    pub auto_recall_limit: usize,
    /// Embedding provider: "local" for on-device ONNX, "openai"/"openrouter"/"custom:URL" for remote API.
    /// None means no embedding (NoopEmbedding, BM25+LIKE only).
    #[serde(default)]
    pub embedding_provider: Option<String>,
    /// Embedding model name (e.g. "gte-multilingual-base" for local, "text-embedding-3-small" for OpenAI).
    #[serde(default)]
    pub embedding_model: Option<String>,
    /// Embedding dimensions override. None = auto-detect from model (768 for gte-multilingual).
    #[serde(default)]
    pub embedding_dims: Option<usize>,
    /// Search mode: "hybrid" (vector+keyword), "embedding" (vector only), or "bm25" (keyword only).
    /// None defaults to Hybrid.
    #[serde(default)]
    pub search_mode: Option<SearchMode>,
    /// Weight for vector similarity in hybrid search (0.0–1.0). None defaults to 0.7.
    #[serde(default)]
    pub vector_weight: Option<f32>,
    /// Weight for BM25 keyword search in hybrid search (0.0–1.0). None defaults to 0.3.
    #[serde(default)]
    pub keyword_weight: Option<f32>,
    /// Merge strategy for combining vector and keyword search results.
    /// None defaults to RRF when neither `vector_weight` nor `keyword_weight`
    /// is explicitly set; otherwise defaults to Weighted (preserving user intent).
    #[serde(default)]
    pub merge_strategy: Option<MergeStrategy>,
    /// Maximum entries in the embedding cache. None defaults to 10_000.
    #[serde(default)]
    pub embedding_cache_max: Option<usize>,
    /// Batch-embed all memories with NULL embeddings at startup.
    #[serde(default)]
    pub backfill_on_startup: bool,
    /// Defer embedding computation to a background task after writing the row.
    /// None defaults to true when an embedding_provider is configured, false otherwise.
    #[serde(default)]
    pub defer_embedding: Option<bool>,
    /// Inject Core memories into the system prompt (cacheable) instead of
    /// the user message (dynamic). None defaults to true when auto_recall=true.
    #[serde(default)]
    pub stable_memory_in_system_prompt: Option<bool>,
    /// Global minimum retention floor for hygiene pruning.
    /// Any category is guaranteed to retain at least this many entries.
    /// None = no global floor (per-category floors may still apply).
    #[serde(default)]
    pub min_retention_floor: Option<usize>,
    /// Minimum retention floor specifically for Daily memories.
    /// Overrides `min_retention_floor` if set. None = use global floor.
    #[serde(default)]
    pub daily_retention_floor: Option<usize>,
    /// Minimum retention floor specifically for Conversation memories.
    /// Overrides `min_retention_floor` if set. None = use global floor.
    #[serde(default)]
    pub conversation_retention_floor: Option<usize>,
}

/// Structured user profile configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserModelConfig {
    /// Enable profile persistence and prompt injection.
    #[serde(default = "default_true_val")]
    pub enabled: bool,
    /// Maximum active profile items rendered into the system prompt.
    #[serde(default = "default_user_model_prompt_items")]
    pub max_prompt_items: usize,
    /// Infer durable profile items from completed conversations.
    #[serde(default)]
    pub auto_infer: bool,
    /// Minimum model confidence accepted for an inferred item.
    #[serde(default = "default_user_model_inference_confidence")]
    pub inference_min_confidence: f64,
    /// Maximum inferred items persisted from one completed turn.
    #[serde(default = "default_user_model_inference_items")]
    pub max_inferred_items_per_turn: usize,
}

fn default_user_model_prompt_items() -> usize {
    20
}

fn default_user_model_inference_confidence() -> f64 {
    0.8
}

fn default_user_model_inference_items() -> usize {
    3
}

impl Default for UserModelConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_prompt_items: default_user_model_prompt_items(),
            auto_infer: false,
            inference_min_confidence: default_user_model_inference_confidence(),
            max_inferred_items_per_turn: default_user_model_inference_items(),
        }
    }
}

fn default_memory_backend() -> String {
    "sqlite".into()
}
fn default_min_relevance() -> f64 {
    0.3
}
fn default_cache_ttl() -> u64 {
    60
}
fn default_cache_max() -> usize {
    1000
}
fn default_cache_hot() -> usize {
    100
}
fn default_hygiene_enabled() -> bool {
    true
}
fn default_conversation_retention_days() -> u32 {
    30
}
fn default_true_val() -> bool {
    true
}
fn default_conflict_threshold() -> f64 {
    0.6
}
fn default_auto_recall_limit() -> usize {
    3
}

impl Default for MemoryConfig {
    fn default() -> Self {
        Self {
            backend: default_memory_backend(),
            auto_save: false,
            min_relevance_score: default_min_relevance(),
            response_cache_enabled: false,
            response_cache_ttl_minutes: default_cache_ttl(),
            response_cache_max_entries: default_cache_max(),
            response_cache_hot_entries: default_cache_hot(),
            namespace: None,
            qdrant: QdrantConfig::default(),
            hygiene_enabled: default_hygiene_enabled(),
            conversation_retention_days: default_conversation_retention_days(),
            snapshot_enabled: false,
            auto_hydrate: default_true_val(),
            conflict_threshold: default_conflict_threshold(),
            conflict_mode: None,
            auto_recall: default_true_val(),
            auto_recall_limit: default_auto_recall_limit(),
            embedding_provider: None,
            embedding_model: None,
            embedding_dims: None,
            search_mode: None,
            vector_weight: None,
            keyword_weight: None,
            merge_strategy: None,
            embedding_cache_max: None,
            backfill_on_startup: false,
            defer_embedding: None,
            stable_memory_in_system_prompt: None,
            min_retention_floor: None,
            daily_retention_floor: None,
            conversation_retention_floor: None,
        }
    }
}

impl MemoryConfig {
    /// Resolve the effective search mode (Hybrid when None).
    pub fn effective_search_mode(&self) -> SearchMode {
        self.search_mode.unwrap_or_default()
    }

    /// Resolve the effective vector weight (0.7 when None).
    pub fn effective_vector_weight(&self) -> f32 {
        self.vector_weight.unwrap_or(0.7)
    }

    /// Resolve the effective keyword weight (0.3 when None).
    pub fn effective_keyword_weight(&self) -> f32 {
        self.keyword_weight.unwrap_or(0.3)
    }

    /// Resolve the effective embedding cache max (10_000 when None).
    pub fn effective_embedding_cache_max(&self) -> usize {
        self.embedding_cache_max.unwrap_or(10_000)
    }

    /// Resolve the effective merge strategy for hybrid search.
    ///
    /// - If `merge_strategy` is explicitly set → use it directly.
    /// - If `vector_weight` or `keyword_weight` is explicitly set → `Weighted`
    ///   (preserving user intent from old config).
    /// - Otherwise → `Rrf { k: 60 }` (new default, eliminates BM25 scale mismatch).
    pub fn effective_merge_strategy(&self) -> MergeStrategy {
        if let Some(ref strategy) = self.merge_strategy {
            return strategy.clone();
        }
        // If user explicitly set any weight → they intended Weighted mode.
        if self.vector_weight.is_some() || self.keyword_weight.is_some() {
            return MergeStrategy::Weighted {
                vector_weight: self.effective_vector_weight(),
                keyword_weight: self.effective_keyword_weight(),
            };
        }
        MergeStrategy::default() // Rrf { k: 60 }
    }

    /// Whether embedding is enabled (provider is set).
    pub fn embedding_enabled(&self) -> bool {
        self.embedding_provider.is_some()
    }

    /// Resolve whether embedding should be deferred.
    ///
    /// - Explicit `defer_embedding` → use it directly.
    /// - None → true when an embedding_provider is configured, false otherwise.
    pub fn effective_defer_embedding(&self) -> bool {
        self.defer_embedding.unwrap_or(self.embedding_enabled())
    }

    /// Resolve whether stable Core memories should be injected into
    /// the system prompt (cacheable) instead of the user message (dynamic).
    ///
    /// - Explicit `stable_memory_in_system_prompt` → use it directly.
    /// - None → true when `auto_recall` is enabled, false otherwise.
    pub fn effective_stable_memory_in_system_prompt(&self) -> bool {
        self.stable_memory_in_system_prompt
            .unwrap_or(self.auto_recall)
    }

    /// Resolve the effective conflict detection mode.
    ///
    /// - Explicit `conflict_mode` → use it directly.
    /// - None → Combined { jaccard_w: 0.4, cosine_w: 0.4, bm25_w: 0.2 }.
    pub fn effective_conflict_mode(&self) -> ConflictMode {
        self.conflict_mode.clone().unwrap_or_default()
    }

    /// Resolve the effective retention floor for a given category.
    ///
    /// - Per-category floor (e.g. `daily_retention_floor`) overrides global if set.
    /// - `min_retention_floor` serves as the global fallback.
    /// - None on both = no floor (0 = unlimited pruning).
    pub fn effective_retention_floor(&self, category: &str) -> usize {
        match category {
            "daily" => self
                .daily_retention_floor
                .or(self.min_retention_floor)
                .unwrap_or(0),
            "conversation" => self
                .conversation_retention_floor
                .or(self.min_retention_floor)
                .unwrap_or(0),
            _ => self.min_retention_floor.unwrap_or(0),
        }
    }
}

/// Qdrant vector database configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct QdrantConfig {
    /// Qdrant API key.
    #[serde(default)]
    pub api_key: Option<String>,
}

/// Autonomy configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutonomyConfig {
    /// The autonomy level: "full", "supervised", or "readonly".
    #[serde(default)]
    pub level: AutonomyLevel,
    /// Tools that never need approval.
    #[serde(default)]
    pub auto_approve: Vec<String>,
    /// Tools that always need approval.
    #[serde(default)]
    pub always_ask: Vec<String>,
    /// Allowed shell commands.
    #[serde(default)]
    pub allowed_commands: Vec<String>,
    /// Tools excluded from non-CLI channels.
    /// Deprecated: use `[agent].denied_tools` instead.
    #[serde(default)]
    pub non_cli_excluded_tools: Vec<String>,
    /// Maximum number of actions per hour (0 = rate-limited / no budget).
    #[serde(default = "default_max_actions_per_hour")]
    pub max_actions_per_hour: u32,
}

/// Autonomy level.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AutonomyLevel {
    ReadOnly,
    #[default]
    Supervised,
    Full,
}

impl Default for AutonomyConfig {
    fn default() -> Self {
        Self {
            level: AutonomyLevel::default(),
            auto_approve: vec![
                "file_read".into(),
                "memory_recall".into(),
                "web_search".into(),
                "web_fetch".into(),
                "weather".into(),
            ],
            always_ask: Vec::new(),
            allowed_commands: Vec::new(),
            non_cli_excluded_tools: Vec::new(),
            max_actions_per_hour: default_max_actions_per_hour(),
        }
    }
}

/// Cron configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CronConfig {
    /// Enable the cron subsystem.
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Run all overdue jobs at startup.
    #[serde(default = "default_true")]
    pub catch_up_on_startup: bool,
    /// Maximum number of historical cron run records.
    #[serde(default = "default_max_run_history")]
    pub max_run_history: u32,
    /// Declarative cron job definitions.
    #[serde(default)]
    pub jobs: Vec<CronJobDecl>,
}

fn default_max_run_history() -> u32 {
    50
}

impl Default for CronConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            catch_up_on_startup: true,
            max_run_history: default_max_run_history(),
            jobs: Vec::new(),
        }
    }
}

/// A declarative cron job definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CronJobDecl {
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default = "default_job_type_decl")]
    pub job_type: String,
    pub schedule: CronScheduleDecl,
    #[serde(default)]
    pub command: Option<String>,
    #[serde(default)]
    pub prompt: Option<String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub allowed_tools: Option<Vec<String>>,
    #[serde(default = "default_true")]
    pub uses_memory: bool,
    #[serde(default)]
    pub session_target: Option<String>,
    #[serde(default)]
    pub delivery: Option<DeliveryConfigDecl>,
}

fn default_job_type_decl() -> String {
    "shell".into()
}

/// Schedule variant for declarative cron jobs.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum CronScheduleDecl {
    Cron {
        expr: String,
        #[serde(default)]
        tz: Option<String>,
    },
    Every {
        every_ms: u64,
    },
    At {
        at: String,
    },
}

/// Delivery configuration for declarative cron jobs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeliveryConfigDecl {
    #[serde(default = "default_delivery_mode")]
    pub mode: String,
    #[serde(default)]
    pub channel: Option<String>,
    #[serde(default)]
    pub to: Option<String>,
    #[serde(default = "default_true")]
    pub best_effort: bool,
}

fn default_delivery_mode() -> String {
    "none".into()
}

impl Default for DeliveryConfigDecl {
    fn default() -> Self {
        Self {
            mode: default_delivery_mode(),
            channel: None,
            to: None,
            best_effort: true,
        }
    }
}

impl Default for SecretsConfig {
    fn default() -> Self {
        Self { encrypt: true }
    }
}

/// Runtime behavior configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RuntimeConfig {
    /// Enable reasoning/thinking mode.
    #[serde(default)]
    pub reasoning_enabled: Option<bool>,

    /// Reasoning effort level (low/medium/high).
    #[serde(default)]
    pub reasoning_effort: Option<String>,
}

/// Scheduler configuration for cron engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchedulerConfig {
    /// Maximum number of cron jobs to execute concurrently.
    #[serde(default = "default_max_concurrent")]
    pub max_concurrent: usize,
    /// Maximum number of due jobs to fetch per poll cycle.
    #[serde(default = "default_max_tasks")]
    pub max_tasks: usize,
}

fn default_max_concurrent() -> usize {
    4
}
fn default_max_tasks() -> usize {
    10
}

impl Default for SchedulerConfig {
    fn default() -> Self {
        Self {
            max_concurrent: default_max_concurrent(),
            max_tasks: default_max_tasks(),
        }
    }
}

/// Backup configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BackupConfig {
    /// Cron expression for scheduled backups (e.g. "0 2 * * *").
    #[serde(default)]
    pub schedule_cron: Option<String>,
    /// Timezone for the backup schedule.
    #[serde(default)]
    pub schedule_timezone: Option<String>,
}

/// Reliability / retry configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReliabilityConfig {
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,

    #[serde(default = "default_retry_delay_secs")]
    pub retry_delay_secs: u64,

    #[serde(default)]
    pub scheduler_poll_secs: u64,
    #[serde(default)]
    pub enable_fallback: bool,
    /// Number of retries for cron job execution.
    #[serde(default = "default_scheduler_retries")]
    pub scheduler_retries: u32,
    /// Initial backoff in milliseconds for provider retries.
    #[serde(default = "default_provider_backoff_ms")]
    pub provider_backoff_ms: u64,
    /// Additional API keys for round-robin rotation.
    #[serde(default)]
    pub api_keys: Vec<String>,
}

fn default_scheduler_retries() -> u32 {
    2
}
fn default_provider_backoff_ms() -> u64 {
    500
}
fn default_max_actions_per_hour() -> u32 {
    1000
}

fn default_scheduler_poll_secs() -> u64 {
    30
}
fn default_max_retries() -> u32 {
    3
}
fn default_retry_delay_secs() -> u64 {
    2
}

impl Default for ReliabilityConfig {
    fn default() -> Self {
        Self {
            max_retries: default_max_retries(),
            retry_delay_secs: default_retry_delay_secs(),
            scheduler_poll_secs: default_scheduler_poll_secs(),
            enable_fallback: false,
            scheduler_retries: default_scheduler_retries(),
            provider_backoff_ms: default_provider_backoff_ms(),
            api_keys: Vec::new(),
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            config_path: PathBuf::new(),
            workspace_dir: PathBuf::new(),
            providers: ProvidersConfig::default(),
            agent: AgentConfig::default(),
            gateway: GatewayConfig::default(),
            storage: StorageConfig::default(),
            reliability: ReliabilityConfig::default(),
            secrets: SecretsConfig::default(),
            runtime: RuntimeConfig::default(),
            memory: MemoryConfig::default(),
            user_model: UserModelConfig::default(),
            autonomy: AutonomyConfig::default(),
            cron: CronConfig::default(),
            scheduler: SchedulerConfig::default(),
            backup: BackupConfig::default(),
            composio: ComposioConfig::default(),
            mcp: McpConfig::default(),
            hooks: HooksConfig::default(),
            tunnel: TunnelConfig::default(),
            nodes: NodesConfig::default(),
            cost: CostConfig::default(),
            channels: ChannelsConfig::default(),
            browser: BrowserConfig::default(),
            http_request: HttpRequestConfig::default(),
            web_fetch: WebFetchConfig::default(),
            transcription: TranscriptionConfig::default(),
            web_search: WebSearchConfig::default(),
            agents: std::collections::HashMap::new(),
            locale: None,
            identity: IdentityConfig::default(),
            skills: SkillsConfig::default(),
        }
    }
}

impl Config {
    /// Load from a TOML file, then apply environment variable overrides.
    pub fn from_file(path: &std::path::Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read config file: {}", path.display()))?;
        let mut config: Config = toml::from_str(&content)
            .with_context(|| format!("Failed to parse config file: {}", path.display()))?;
        config.config_path = path.to_path_buf();
        Ok(Self::with_env_overrides(config))
    }

    /// Return the default TOML config string written on first run.
    pub fn default_toml() -> String {
        r#"# ClawSeed default configuration
# Auto-generated on first run. Edit as needed.

[gateway]
host = "127.0.0.1"
port = 42617
require_pairing = false
session_persistence = true

[providers]
fallback = "custom:https://api.deepseek.com"

[providers.models."custom:https://api.deepseek.com"]
model = "deepseek-chat"
base_url = "https://api.deepseek.com"

[providers.models."custom:https://coding.dashscope.aliyuncs.com/apps/anthropic"]
base_url = "https://coding.dashscope.aliyuncs.com/apps/anthropic"
model = "glm-5"
name = "claude-sonnet-4-5"

[agent]
max_tool_iterations = 10

[autonomy]
level = "supervised"
auto_approve = ["file_read", "memory_recall", "web_search", "web_fetch", "calculator", "glob_search", "content_search"]

[memory]
backend = "sqlite"
auto_save = true
auto_recall_limit = 3

[user_model]
enabled = true
max_prompt_items = 20
auto_infer = false
inference_min_confidence = 0.8
max_inferred_items_per_turn = 3

[reliability]
max_retries = 2
provider_backoff_ms = 500

[secrets]
encrypt = true
"#.to_string()
    }

    /// Validate the configuration (stub — always succeeds).
    pub fn validate(&self) -> Result<()> {
        Ok(())
    }

    /// Save configuration back to the file it was loaded from.
    pub fn save(&self) -> Result<()> {
        if self.config_path.as_os_str().is_empty() {
            return Ok(());
        }
        let toml_str =
            toml::to_string_pretty(self).context("Failed to serialize config to TOML")?;
        std::fs::write(&self.config_path, &toml_str)
            .with_context(|| format!("Failed to write config: {}", self.config_path.display()))?;
        Ok(())
    }

    /// Apply `CLAWSEED_*` environment variable overrides.
    pub fn with_env_overrides(mut self) -> Self {
        if let Ok(v) = std::env::var("CLAWSEED_PROVIDER") {
            self.providers.default = Some(v);
        }
        if let Ok(v) = std::env::var("CLAWSEED_MODEL") {
            self.providers.default_model = Some(v);
        }
        if let Ok(v) = std::env::var("CLAWSEED_API_KEY") {
            self.providers.default_api_key = Some(v);
        }
        if let Ok(v) = std::env::var("CLAWSEED_PROVIDER_URL") {
            self.providers.default_base_url = Some(v);
        }
        if let Ok(v) = std::env::var("CLAWSEED_PROVIDER_TIMEOUT_SECS")
            && let Ok(secs) = v.parse::<u64>()
        {
            self.providers.default_timeout_secs = Some(secs);
        }
        if let Ok(v) = std::env::var("CLAWSEED_GATEWAY_HOST") {
            self.gateway.host = v;
        }
        if let Ok(v) = std::env::var("CLAWSEED_GATEWAY_PORT")
            && let Ok(port) = v.parse::<u16>()
        {
            self.gateway.port = port;
        }
        if let Ok(v) = std::env::var("CLAWSEED_WORKSPACE") {
            self.workspace_dir = PathBuf::from(v);
        }
        if let Ok(v) = std::env::var("CLAWSEED_EXTRA_HEADERS") {
            self.providers.extra_headers = parse_extra_headers(&v);
        }
        if let Ok(v) = std::env::var("CLAWSEED_TEMPERATURE")
            && let Ok(t) = v.parse::<f64>()
        {
            self.agent.temperature = Some(t);
        }
        if let Ok(v) = std::env::var("CLAWSEED_STORAGE_DB_URL") {
            self.storage.db_url = Some(v);
        }
        if let Ok(v) = std::env::var("CLAWSEED_WEB_SEARCH_ENABLED") {
            let enabled = v.parse().unwrap_or(false);
            self.agent.web_search_enabled = enabled;
            self.web_search.enabled = enabled;
        }
        if let Ok(v) = std::env::var("CLAWSEED_WEB_SEARCH_PROVIDER") {
            self.agent.web_search_provider = Some(v.clone());
            self.web_search.provider = Some(v);
        }
        if let Ok(v) = std::env::var("CLAWSEED_WEB_SEARCH_TAVILY_API_KEY") {
            self.web_search.tavily_api_key = Some(v);
        }
        if let Ok(v) = std::env::var("CLAWSEED_EMBEDDING_PROVIDER") {
            self.memory.embedding_provider = Some(v);
        }
        if let Ok(v) = std::env::var("CLAWSEED_EMBEDDING_MODEL") {
            self.memory.embedding_model = Some(v);
        }
        if let Ok(v) = std::env::var("CLAWSEED_EMBEDDING_API_KEY")
            && let Some(route) = self.providers.embedding_routes.first_mut()
        {
            route.api_key = Some(v);
        }
        if let Ok(v) = std::env::var("CLAWSEED_EMBEDDING_DIMENSIONS")
            && let Ok(dims) = v.parse::<usize>()
        {
            self.memory.embedding_dims = Some(dims);
        }
        self
    }

    /// Resolve the active provider name.
    pub fn resolve_provider(&self) -> Option<&str> {
        self.providers.default.as_deref()
    }

    /// Resolve the active model name.
    pub fn resolve_model(&self) -> Option<&str> {
        self.providers.default_model.as_deref()
    }
}

/// Parse comma-separated `key=value` headers.
fn parse_extra_headers(input: &str) -> std::collections::HashMap<String, String> {
    let mut map = std::collections::HashMap::new();
    for pair in input.split(',') {
        if let Some((k, v)) = pair.trim().split_once('=') {
            map.insert(k.trim().to_string(), v.trim().to_string());
        }
    }
    map
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_roundtrip() {
        let config = Config::default();
        let toml_str = toml::to_string(&config).unwrap();
        let parsed: Config = toml::from_str(&toml_str).unwrap();
        assert_eq!(parsed.gateway.port, config.gateway.port);
        assert_eq!(
            parsed.agent.max_tool_iterations,
            config.agent.max_tool_iterations
        );
    }

    #[test]
    fn env_overrides_provider() {
        // SAFETY: test-only, single-threaded test
        unsafe { std::env::set_var("CLAWSEED_PROVIDER", "anthropic") };
        let config = Config::default().with_env_overrides();
        assert_eq!(config.providers.default.as_deref(), Some("anthropic"));
        unsafe { std::env::remove_var("CLAWSEED_PROVIDER") };
    }

    #[test]
    fn env_overrides_gateway_port() {
        unsafe { std::env::set_var("CLAWSEED_GATEWAY_PORT", "9999") };
        let config = Config::default().with_env_overrides();
        assert_eq!(config.gateway.port, 9999);
        unsafe { std::env::remove_var("CLAWSEED_GATEWAY_PORT") };
    }

    #[test]
    fn env_overrides_temperature() {
        unsafe { std::env::set_var("CLAWSEED_TEMPERATURE", "0.5") };
        let config = Config::default().with_env_overrides();
        assert_eq!(config.agent.temperature, Some(0.5));
        unsafe { std::env::remove_var("CLAWSEED_TEMPERATURE") };
    }

    #[test]
    fn default_toml_parses_successfully() {
        let toml_str = Config::default_toml();
        let config: Config = toml::from_str(&toml_str).expect("default_toml should parse");
        assert_eq!(config.gateway.port, 42617);
        assert_eq!(
            config.providers.fallback.as_deref(),
            Some("custom:https://api.deepseek.com")
        );
        assert_eq!(config.agent.max_tool_iterations, 10);
        assert!(config.memory.auto_save);
        assert_eq!(config.memory.backend, "sqlite");
    }

    #[test]
    fn default_port_is_42617() {
        assert_eq!(GatewayConfig::default().port, 42617);
    }

    #[test]
    fn user_model_config_defaults_and_overrides() {
        let defaults: Config = toml::from_str("").unwrap();
        assert!(defaults.user_model.enabled);
        assert_eq!(defaults.user_model.max_prompt_items, 20);
        assert!(!defaults.user_model.auto_infer);
        assert_eq!(defaults.user_model.inference_min_confidence, 0.8);
        assert_eq!(defaults.user_model.max_inferred_items_per_turn, 3);

        let configured: Config = toml::from_str(
            "[user_model]\nenabled = false\nmax_prompt_items = 7\nauto_infer = true\ninference_min_confidence = 0.9\nmax_inferred_items_per_turn = 2\n",
        )
        .unwrap();
        assert!(!configured.user_model.enabled);
        assert_eq!(configured.user_model.max_prompt_items, 7);
        assert!(configured.user_model.auto_infer);
        assert_eq!(configured.user_model.inference_min_confidence, 0.9);
        assert_eq!(configured.user_model.max_inferred_items_per_turn, 2);
    }

    #[test]
    fn parse_extra_headers_basic() {
        let map = parse_extra_headers("X-Api-Key=abc, X-Request-Id=123");
        assert_eq!(map.get("X-Api-Key").unwrap(), "abc");
        assert_eq!(map.get("X-Request-Id").unwrap(), "123");
    }

    #[test]
    fn parse_extra_headers_empty() {
        let map = parse_extra_headers("");
        assert!(map.is_empty());
    }

    #[test]
    fn from_toml_string() {
        let toml_str = r#"
[providers]
default = "openai"
default_model = "gpt-4"

[agent]
max_tool_iterations = 20

[gateway]
port = 8080
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.providers.default.as_deref(), Some("openai"));
        assert_eq!(config.providers.default_model.as_deref(), Some("gpt-4"));
        assert_eq!(config.agent.max_tool_iterations, 20);
        assert_eq!(config.gateway.port, 8080);
    }
}

// ── Stub config types for gateway compatibility ──────────────────

/// Composio integration configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComposioConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub api_key: Option<String>,
    #[serde(default = "default_entity_id")]
    pub entity_id: String,
}

fn default_entity_id() -> String {
    "default".into()
}

impl Default for ComposioConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            api_key: None,
            entity_id: default_entity_id(),
        }
    }
}

/// MCP (Model Context Protocol) configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct McpConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub servers: Vec<Config>,
    #[serde(default)]
    pub deferred_loading: bool,
}

/// Hooks configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HooksConfig {
    #[serde(default)]
    pub enabled: bool,
    /// Ordered list of hook declarations. Hooks are run in declaration order.
    #[serde(default)]
    pub chain: Vec<HookDecl>,
}

/// A declarative hook entry in config.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookDecl {
    /// Hook type identifier. Built-in types: "security_policy", "audit_log".
    #[serde(rename = "type")]
    pub hook_type: String,
    /// Hook-specific configuration.
    #[serde(default)]
    pub config: serde_json::Value,
}

/// Tunnel configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TunnelConfig {
    #[serde(default = "default_tunnel_provider")]
    pub provider: String,
    /// Ngrok tunnel configuration.
    #[serde(default)]
    pub ngrok: Option<NgrokTunnelConfig>,
    /// Cloudflare tunnel configuration.
    #[serde(default)]
    pub cloudflare: Option<CloudflareTunnelConfig>,
}

/// Ngrok tunnel configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NgrokTunnelConfig {
    #[serde(default)]
    pub auth_token: String,
}

/// Cloudflare tunnel configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CloudflareTunnelConfig {
    #[serde(default)]
    pub token: String,
}

fn default_tunnel_provider() -> String {
    "none".into()
}

/// Nodes configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NodesConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_max_nodes")]
    pub max_nodes: usize,
}

fn default_max_nodes() -> usize {
    16
}

/// Cost tracking configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CostConfig {}

/// Channels configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ChannelsConfig {
    #[serde(default)]
    pub webhook: Option<WebhookChannelConfig>,
    #[serde(default)]
    pub telegram: Option<TelegramChannelConfig>,
    #[serde(default)]
    pub discord: Option<DiscordChannelConfig>,
    #[serde(default)]
    pub slack: Option<SlackChannelConfig>,
    #[serde(default)]
    pub mattermost: Option<MattermostChannelConfig>,
    #[serde(default)]
    pub matrix: Option<MatrixChannelConfig>,
    #[serde(default)]
    pub whatsapp: Option<WhatsAppChannelConfig>,
    #[serde(default)]
    pub linq: Option<LinqChannelConfig>,
    #[serde(default)]
    pub nextcloud_talk: Option<NextcloudTalkChannelConfig>,
    #[serde(default)]
    pub wati: Option<WatiChannelConfig>,
    #[serde(default)]
    pub irc: Option<IrcChannelConfig>,
    #[serde(default)]
    pub lark: Option<LarkChannelConfig>,
    #[serde(default)]
    pub feishu: Option<FeishuChannelConfig>,
    #[serde(default)]
    pub dingtalk: Option<DingtalkChannelConfig>,
    #[serde(default)]
    pub qq: Option<QqChannelConfig>,
    #[serde(default)]
    pub nostr: Option<NostrChannelConfig>,
    #[serde(default)]
    pub clawdtalk: Option<ClawdTalkChannelConfig>,
    #[serde(default)]
    pub email: Option<EmailChannelConfig>,
    #[serde(default)]
    pub voice_duplex: Option<serde_json::Value>,
}

/// Telegram channel configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelegramChannelConfig {
    #[serde(default)]
    pub bot_token: String,
}

/// Discord channel configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscordChannelConfig {
    #[serde(default)]
    pub bot_token: String,
}

/// Slack channel configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlackChannelConfig {
    #[serde(default)]
    pub bot_token: String,
    #[serde(default)]
    pub app_token: Option<String>,
}

/// Mattermost channel configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MattermostChannelConfig {
    #[serde(default)]
    pub bot_token: String,
}

/// Matrix channel configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatrixChannelConfig {
    #[serde(default)]
    pub access_token: String,
}

/// WhatsApp channel configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WhatsAppChannelConfig {
    #[serde(default)]
    pub access_token: Option<String>,
    #[serde(default)]
    pub app_secret: Option<String>,
    #[serde(default)]
    pub verify_token: Option<String>,
}

/// Linq channel configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinqChannelConfig {
    #[serde(default)]
    pub api_token: String,
    #[serde(default)]
    pub signing_secret: Option<String>,
}

/// Nextcloud Talk channel configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NextcloudTalkChannelConfig {
    #[serde(default)]
    pub app_token: String,
    #[serde(default)]
    pub webhook_secret: Option<String>,
}

/// Wati channel configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatiChannelConfig {
    #[serde(default)]
    pub api_token: String,
}

/// IRC channel configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IrcChannelConfig {
    #[serde(default)]
    pub server_password: Option<String>,
    #[serde(default)]
    pub nickserv_password: Option<String>,
    #[serde(default)]
    pub sasl_password: Option<String>,
}

/// Lark channel configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LarkChannelConfig {
    #[serde(default)]
    pub app_secret: String,
    #[serde(default)]
    pub encrypt_key: Option<String>,
    #[serde(default)]
    pub verification_token: Option<String>,
}

/// Feishu channel configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeishuChannelConfig {
    #[serde(default)]
    pub app_secret: String,
    #[serde(default)]
    pub encrypt_key: Option<String>,
    #[serde(default)]
    pub verification_token: Option<String>,
}

/// Dingtalk channel configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DingtalkChannelConfig {
    #[serde(default)]
    pub client_secret: String,
}

/// QQ channel configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QqChannelConfig {
    #[serde(default)]
    pub app_secret: String,
}

/// Nostr channel configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NostrChannelConfig {
    #[serde(default)]
    pub private_key: String,
}

/// ClawdTalk channel configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClawdTalkChannelConfig {
    #[serde(default)]
    pub api_key: String,
    #[serde(default)]
    pub webhook_secret: Option<String>,
}

/// Email channel configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmailChannelConfig {
    #[serde(default)]
    pub password: String,
}

impl ChannelsConfig {
    /// Iterate over configured channels with their presence status.
    pub fn channels(&self) -> Vec<(ChannelInfo, bool)> {
        let mut result = Vec::new();
        if self.webhook.is_some() {
            result.push((ChannelInfo { name: "webhook" }, true));
        }
        if self.telegram.is_some() {
            result.push((ChannelInfo { name: "telegram" }, true));
        }
        if self.discord.is_some() {
            result.push((ChannelInfo { name: "discord" }, true));
        }
        if self.slack.is_some() {
            result.push((ChannelInfo { name: "slack" }, true));
        }
        if self.mattermost.is_some() {
            result.push((ChannelInfo { name: "mattermost" }, true));
        }
        if self.matrix.is_some() {
            result.push((ChannelInfo { name: "matrix" }, true));
        }
        if self.whatsapp.is_some() {
            result.push((ChannelInfo { name: "whatsapp" }, true));
        }
        if self.linq.is_some() {
            result.push((ChannelInfo { name: "linq" }, true));
        }
        if self.nextcloud_talk.is_some() {
            result.push((
                ChannelInfo {
                    name: "nextcloud_talk",
                },
                true,
            ));
        }
        if self.wati.is_some() {
            result.push((ChannelInfo { name: "wati" }, true));
        }
        if self.irc.is_some() {
            result.push((ChannelInfo { name: "irc" }, true));
        }
        if self.lark.is_some() {
            result.push((ChannelInfo { name: "lark" }, true));
        }
        if self.feishu.is_some() {
            result.push((ChannelInfo { name: "feishu" }, true));
        }
        if self.dingtalk.is_some() {
            result.push((ChannelInfo { name: "dingtalk" }, true));
        }
        if self.qq.is_some() {
            result.push((ChannelInfo { name: "qq" }, true));
        }
        if self.clawdtalk.is_some() {
            result.push((ChannelInfo { name: "clawdtalk" }, true));
        }
        if self.email.is_some() {
            result.push((ChannelInfo { name: "email" }, true));
        }
        result
    }
}

/// Channel info (minimal).
#[derive(Debug, Clone)]
pub struct ChannelInfo {
    name: &'static str,
}

impl ChannelInfo {
    pub fn name(&self) -> &str {
        self.name
    }
}

/// Webhook channel configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookChannelConfig {
    #[serde(default)]
    pub secret: Option<String>,
}

/// Browser configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BrowserConfig {
    #[serde(default)]
    pub computer_use: ComputerUseConfig,
}

/// Computer use configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ComputerUseConfig {
    #[serde(default)]
    pub api_key: Option<String>,
}

/// HTTP request configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HttpRequestConfig {
    /// Whether the HTTP request tool is enabled.
    #[serde(default)]
    pub enabled: bool,
    /// Allowed domains (empty = allow all when enabled).
    #[serde(default)]
    pub allowed_domains: Vec<String>,
}

/// Web fetch configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WebFetchConfig {
    /// Whether the web fetch tool is enabled.
    #[serde(default)]
    pub enabled: bool,
    /// Allowed domains (empty = allow all when enabled).
    #[serde(default)]
    pub allowed_domains: Vec<String>,
}

/// Transcription configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TranscriptionConfig {
    #[serde(default)]
    pub api_key: Option<String>,
}

/// Web search configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WebSearchConfig {
    /// Whether the web search tool is enabled.
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub provider: Option<String>,
    #[serde(default)]
    pub brave_api_key: Option<String>,
    #[serde(default)]
    pub searxng_instance_url: Option<String>,
    #[serde(default)]
    pub tavily_api_key: Option<String>,
}

/// Agent entry configuration (for named agents in the config).
///
/// Historically this only carried a named `api_key`. It now also serves as a
/// **persona** definition: a named bundle of soul (identity / system prompt),
/// memory namespace, and tool-filter overrides that can be selected per
/// connection. Fields default to empty/None, so legacy configs that only set
/// `api_key` keep working unchanged.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AgentEntryConfig {
    #[serde(default)]
    pub api_key: Option<String>,

    /// Soul override: an `IdentityConfig` (openclaw directory or AIEOS
    /// inline/path). When set, this persona uses its own identity instead of
    /// the global `[identity]` block.
    #[serde(default)]
    pub identity: Option<IdentityConfig>,

    /// Memory isolation namespace. When set, the persona's memory operations
    /// are scoped to this namespace within the shared memory backend (see
    /// `NamespacedMemory`). `None` = share the global memory space.
    #[serde(default)]
    pub memory_namespace: Option<String>,

    /// Glob patterns for tool names this persona is allowed to use. Overrides
    /// the global `[agent].allowed_tools` when non-empty.
    #[serde(default)]
    pub allowed_tools: Vec<String>,

    /// Glob patterns for tool names this persona is denied (takes precedence
    /// over allowed). Overrides the global `[agent].denied_tools` when non-empty.
    #[serde(default)]
    pub denied_tools: Vec<String>,

    /// Skill names disabled for this persona. Merged into `[skills].excluded`
    /// when the persona is resolved.
    #[serde(default)]
    pub denied_skills: Vec<String>,

    /// Optional model override for this persona. When set, it replaces the
    /// current fallback provider's model while keeping provider credentials
    /// and transport settings from the global provider profile.
    #[serde(default)]
    pub model: Option<String>,

    /// Optional thinking/reasoning toggle for this persona. `None` inherits the
    /// global provider profile; `Some(true/false)` writes
    /// `provider_extra.thinking.type = enabled/disabled` for this persona's
    /// resolved config.
    #[serde(default)]
    pub thinking_enabled: Option<bool>,

    /// Optional short avatar label for UI clients, typically an emoji or one
    /// to two display characters. Runtime resolution ignores this field.
    #[serde(default)]
    pub avatar: Option<String>,

    /// Optional UI accent color in `#RRGGBB` format. Runtime resolution ignores
    /// this field.
    #[serde(default)]
    pub color: Option<String>,

    /// Direct system-prompt override — the simplest soul form, bypassing both
    /// AIEOS and workspace personality files. When set, takes precedence over
    /// the global `[identity]` for this persona.
    #[serde(default)]
    pub system_prompt: Option<String>,
}

impl AgentEntryConfig {
    /// True if any persona-specific override is set (i.e. this entry is more
    /// than just a named API key). Used to decide whether resolving a persona
    /// should produce overrides at all.
    pub fn has_persona_overrides(&self) -> bool {
        self.identity.is_some()
            || self.memory_namespace.is_some()
            || !self.allowed_tools.is_empty()
            || !self.denied_tools.is_empty()
            || !self.denied_skills.is_empty()
            || self.model.is_some()
            || self.thinking_enabled.is_some()
            || self.avatar.is_some()
            || self.color.is_some()
            || self.system_prompt.is_some()
    }
}

/// Skill system configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillsConfig {
    /// Enable the skill system.
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// Additional skill root directories (appended after defaults).
    #[serde(default)]
    pub extra_roots: Vec<String>,

    /// Maximum number of active skills simultaneously.
    #[serde(default = "default_max_active_skills")]
    pub max_active: usize,

    /// Skill names to exclude from the index.
    #[serde(default)]
    pub excluded: Vec<String>,
}

fn default_max_active_skills() -> usize {
    5
}

impl Default for SkillsConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            extra_roots: Vec::new(),
            max_active: default_max_active_skills(),
            excluded: Vec::new(),
        }
    }
}

/// Identity / persona configuration.
///
/// Supports two formats:
/// - `"openclaw"` (default): loads markdown files (SOUL.md, IDENTITY.md, etc.)
///   from the workspace directory.
/// - `"aieos"`: loads an AIEOS v1.1 JSON identity from a file or inline string.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdentityConfig {
    /// Identity format: `"openclaw"` (default) or `"aieos"`.
    #[serde(default = "default_identity_format")]
    pub format: String,
    /// Path to AIEOS JSON file (relative to workspace directory).
    #[serde(default)]
    pub aieos_path: Option<String>,
    /// Inline AIEOS JSON (alternative to file path).
    #[serde(default)]
    pub aieos_inline: Option<String>,
    /// Directory (relative to workspace_dir) from which to load openclaw
    /// personality files (SOUL.md, IDENTITY.md, ...). `None` = load from the
    /// workspace root. Only consulted when `format == "openclaw"`.
    #[serde(default)]
    pub personality_dir: Option<String>,
}

fn default_identity_format() -> String {
    "openclaw".into()
}

impl Default for IdentityConfig {
    fn default() -> Self {
        Self {
            format: default_identity_format(),
            aieos_path: None,
            aieos_inline: None,
            personality_dir: None,
        }
    }
}
