//! Observer trait for event emission.
//!
//! Minimal implementation: NoopObserver only.

use std::time::Duration;

/// Discrete events emitted by the agent runtime.
#[derive(Debug, Clone)]
pub enum ObserverEvent {
    AgentStart {
        provider: String,
        model: String,
    },
    LlmRequest {
        provider: String,
        model: String,
        messages_count: usize,
    },
    LlmResponse {
        provider: String,
        model: String,
        duration: Duration,
        success: bool,
        error_message: Option<String>,
        input_tokens: Option<u64>,
        output_tokens: Option<u64>,
    },
    AgentEnd {
        provider: String,
        model: String,
        duration: Duration,
        tokens_used: Option<u64>,
        cost_usd: Option<f64>,
    },
    ToolCallStart {
        tool: String,
        arguments: Option<String>,
    },
    ToolCall {
        tool: String,
        duration: Duration,
        success: bool,
    },
    TurnComplete,
    HeartbeatTick,
    CacheHit {
        cache_type: String,
        tokens_saved: u64,
    },
    CacheMiss {
        cache_type: String,
    },
    Error {
        component: String,
        message: String,
    },
}

/// Numeric metrics emitted by the agent runtime.
#[derive(Debug, Clone)]
pub enum ObserverMetric {
    RequestLatency(Duration),
    TokensUsed(u64),
    ActiveSessions(u64),
}

/// Core observability trait.
pub trait Observer: Send + Sync + 'static {
    fn record_event(&self, event: &ObserverEvent);
    fn record_metric(&self, metric: &ObserverMetric);
    fn flush(&self) {}
    fn name(&self) -> &str;
    fn as_any(&self) -> &dyn std::any::Any;
}

/// No-op observer used as the default when no observability backend is configured.
pub struct NoopObserver;

impl Observer for NoopObserver {
    fn record_event(&self, _event: &ObserverEvent) {}
    fn record_metric(&self, _metric: &ObserverMetric) {}
    fn name(&self) -> &str {
        "noop"
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

/// Create a default (noop) observer.
pub fn create_observer() -> Box<dyn Observer> {
    Box::new(NoopObserver)
}
