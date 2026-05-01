//! Agent configuration.

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
}

fn default_max_tool_iterations() -> usize { 25 }

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            max_tool_iterations: default_max_tool_iterations(),
            temperature: None,
            max_tokens: None,
            web_search_enabled: false,
            web_search_provider: None,
            system_prompt: None,
            memory_namespace: None,
            daily_budget_usd: None,
            turn_budget_usd: None,
        }
    }
}
