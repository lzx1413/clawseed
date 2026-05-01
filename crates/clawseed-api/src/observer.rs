//! Observer trait for event emission.

use std::time::Duration;

/// Discrete events emitted by the agent runtime.
#[derive(Debug, Clone)]
pub enum ObserverEvent {
    AgentStart { provider: String, model: String },
    LlmRequest { provider: String, model: String, messages_count: usize },
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
    ToolCallStart { tool: String, arguments: Option<String> },
    ToolCall { tool: String, duration: Duration, success: bool },
    TurnComplete,
    HeartbeatTick,
    CacheHit { cache_type: String, tokens_saved: u64 },
    CacheMiss { cache_type: String },
    Error { component: String, message: String },
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

#[cfg(test)]
mod tests {
    use super::*;
    use parking_lot::Mutex;

    #[derive(Default)]
    struct DummyObserver {
        events: Mutex<u64>,
        metrics: Mutex<u64>,
    }

    impl Observer for DummyObserver {
        fn record_event(&self, _event: &ObserverEvent) {
            *self.events.lock() += 1;
        }
        fn record_metric(&self, _metric: &ObserverMetric) {
            *self.metrics.lock() += 1;
        }
        fn name(&self) -> &str { "dummy" }
        fn as_any(&self) -> &dyn std::any::Any { self }
    }

    #[test]
    fn observer_records_events_and_metrics() {
        let observer = DummyObserver::default();
        observer.record_event(&ObserverEvent::HeartbeatTick);
        observer.record_event(&ObserverEvent::Error {
            component: "test".into(),
            message: "boom".into(),
        });
        observer.record_metric(&ObserverMetric::TokensUsed(42));
        assert_eq!(*observer.events.lock(), 2);
        assert_eq!(*observer.metrics.lock(), 1);
    }

    #[test]
    fn observer_default_flush_and_as_any_work() {
        let observer = DummyObserver::default();
        observer.flush();
        assert_eq!(observer.name(), "dummy");
        assert!(observer.as_any().downcast_ref::<DummyObserver>().is_some());
    }
}
