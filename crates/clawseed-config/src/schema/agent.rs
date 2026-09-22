//! Agent configuration.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// Agent configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    /// Maximum number of tool call iterations per turn.
    #[serde(default = "default_max_tool_iterations")]
    pub max_tool_iterations: usize,

    /// Sampling temperature.
    #[serde(default)]
    pub temperature: Option<f64>,

    /// Maximum output tokens.
    #[serde(default)]
    pub max_tokens: Option<u32>,

    /// Enable token-aware automatic conversation compaction.
    ///
    /// When disabled, the legacy message-count history guard remains in use.
    #[serde(default = "default_context_compaction_enabled")]
    pub context_compaction_enabled: bool,

    /// Model context window in tokens used by automatic compaction.
    ///
    /// Providers do not expose a portable context-window capability through the
    /// `Provider` trait, so deployments can set this explicitly. When omitted,
    /// `context_compaction_trigger_tokens` is used instead.
    #[serde(default = "default_context_window_tokens")]
    pub context_window_tokens: Option<usize>,

    /// Explicit token threshold that triggers automatic compaction.
    /// Takes precedence over `context_window_tokens` and its percentage.
    #[serde(default)]
    pub context_compaction_trigger_tokens: Option<usize>,

    /// Percentage of `context_window_tokens` at which compaction starts.
    #[serde(default = "default_context_compaction_threshold_percent")]
    pub context_compaction_threshold_percent: u8,

    /// Target size for the generated conversation summary.
    #[serde(default = "default_context_compaction_target_tokens")]
    pub context_compaction_target_tokens: usize,

    /// Number of recent non-system messages to retain verbatim after compaction.
    #[serde(
        default = "default_context_compaction_keep_recent_messages",
        skip_serializing
    )]
    pub context_compaction_keep_recent_messages: usize,

    /// Number of recent conversation turns to retain verbatim after compaction.
    ///
    /// This supersedes `context_compaction_keep_recent_messages`; the legacy
    /// field remains deserializable so existing configurations continue to
    /// work.
    #[serde(default)]
    pub context_compaction_keep_recent_turns: Option<usize>,

    /// Enable web search tool.
    #[serde(default)]
    pub web_search_enabled: bool,

    /// Web search provider (e.g. "brave", "searxng").
    #[serde(default)]
    pub web_search_provider: Option<String>,

    /// System prompt override.
    #[serde(default)]
    pub system_prompt: Option<String>,

    /// Memory namespace for isolation.
    #[serde(default)]
    pub memory_namespace: Option<String>,

    /// Daily cost budget in USD (0 = unlimited).
    #[serde(default)]
    pub daily_budget_usd: Option<f64>,

    /// Per-turn cost budget in USD (0 = unlimited).
    #[serde(default)]
    pub turn_budget_usd: Option<f64>,

    /// Glob patterns for tool names that are allowed.
    /// Supports wildcards like "file_*", "memory_*".
    /// If empty, all tools are allowed.
    #[serde(default)]
    pub allowed_tools: Vec<String>,

    /// Glob patterns for tool names that are denied (takes precedence over allowed).
    #[serde(default)]
    pub denied_tools: Vec<String>,

    /// MCP server-level tool filtering.
    /// Map from MCP server name to list of allowed tool name globs.
    /// If a server is not in this map, all its tools are allowed.
    #[serde(default)]
    pub mcp_tool_filters: HashMap<String, Vec<String>>,
}

fn default_max_tool_iterations() -> usize {
    25
}

fn default_context_compaction_threshold_percent() -> u8 {
    80
}

fn default_context_compaction_enabled() -> bool {
    true
}

fn default_context_window_tokens() -> Option<usize> {
    Some(512_000)
}

fn default_context_compaction_target_tokens() -> usize {
    2_048
}

fn default_context_compaction_keep_recent_messages() -> usize {
    1
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            max_tool_iterations: default_max_tool_iterations(),
            temperature: None,
            max_tokens: None,
            context_compaction_enabled: default_context_compaction_enabled(),
            context_window_tokens: default_context_window_tokens(),
            context_compaction_trigger_tokens: None,
            context_compaction_threshold_percent: default_context_compaction_threshold_percent(),
            context_compaction_target_tokens: default_context_compaction_target_tokens(),
            context_compaction_keep_recent_messages:
                default_context_compaction_keep_recent_messages(),
            context_compaction_keep_recent_turns: None,
            web_search_enabled: false,
            web_search_provider: None,
            system_prompt: None,
            memory_namespace: None,
            daily_budget_usd: None,
            turn_budget_usd: None,
            allowed_tools: Vec::new(),
            denied_tools: Vec::new(),
            mcp_tool_filters: HashMap::new(),
        }
    }
}
