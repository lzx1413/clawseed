//! Cost tracking types.
//!
//! Minimal types for cost tracking.

use std::sync::atomic::{AtomicU64, Ordering};

/// Budget check result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BudgetCheck {
    Allowed,
    OverDaily,
    OverTurn,
}

/// Token usage record for cost tracking.
#[derive(Debug, Clone)]
pub struct TokenUsage {
    pub model: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub input_price: f64,
    pub output_price: f64,
    pub total_tokens: u64,
    pub cost_usd: f64,
}

impl TokenUsage {
    pub fn new(
        model: &str,
        input_tokens: u64,
        output_tokens: u64,
        input_price: f64,
        output_price: f64,
    ) -> Self {
        let total_tokens = input_tokens.saturating_add(output_tokens);
        let cost_usd = (input_tokens as f64 * input_price + output_tokens as f64 * output_price)
            / 1_000_000.0;
        Self {
            model: model.to_string(),
            input_tokens,
            output_tokens,
            input_price,
            output_price,
            total_tokens,
            cost_usd,
        }
    }
}

/// Simple cost tracker.
pub struct CostTracker {
    total_tokens: AtomicU64,
    total_cost_usd: AtomicU64, // stored as cents * 10^6 to avoid f64 atomics
}

impl CostTracker {
    pub fn new() -> Self {
        Self {
            total_tokens: AtomicU64::new(0),
            total_cost_usd: AtomicU64::new(0),
        }
    }

    pub fn record_usage(&self, usage: TokenUsage) -> anyhow::Result<()> {
        self.total_tokens.fetch_add(usage.total_tokens, Ordering::Relaxed);
        // Store cost as micro-cents (multiply by 1e8 to get integer representation)
        let cost_microcents = (usage.cost_usd * 1e8) as u64;
        self.total_cost_usd.fetch_add(cost_microcents, Ordering::Relaxed);
        Ok(())
    }

    pub fn check_budget(&self, _additional_cost_usd: f64) -> anyhow::Result<BudgetCheck> {
        Ok(BudgetCheck::Allowed)
    }

    pub fn total_cost_usd(&self) -> f64 {
        self.total_cost_usd.load(Ordering::Relaxed) as f64 / 1e8
    }

    pub fn total_tokens(&self) -> u64 {
        self.total_tokens.load(Ordering::Relaxed)
    }

    /// Get or initialize a global cost tracker (stub).
    pub fn get_or_init_global(_cost_config: clawseed_config::schema::CostConfig, _workspace_dir: &std::path::Path) -> std::sync::Arc<Self> {
        std::sync::Arc::new(Self::new())
    }

    /// Get a summary of cost usage (stub).
    pub fn get_summary(&self) -> serde_json::Value {
        serde_json::json!({
            "total_tokens": self.total_tokens(),
            "total_cost_usd": self.total_cost_usd(),
        })
    }
}

impl Default for CostTracker {
    fn default() -> Self {
        Self::new()
    }
}
