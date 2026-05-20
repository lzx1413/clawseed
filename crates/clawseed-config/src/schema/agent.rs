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

    /// When true, automatically continue generation when the LLM response is
    /// truncated due to max_tokens. A continuation message is appended and
    /// the provider is called again.
    #[serde(default = "default_auto_continue_on_truncation")]
    pub auto_continue_on_truncation: bool,

    /// Maximum consecutive auto-continuation rounds (safety limit).
    #[serde(default = "default_max_auto_continue")]
    pub max_auto_continue: usize,

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

fn default_auto_continue_on_truncation() -> bool {
    true
}

fn default_max_auto_continue() -> usize {
    10
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            max_tool_iterations: default_max_tool_iterations(),
            temperature: None,
            max_tokens: None,
            auto_continue_on_truncation: true,
            max_auto_continue: default_max_auto_continue(),
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
