//! Observability module — re-exports from observer.rs and adds gateway-needed stubs.

pub use crate::observer::*;

/// Sub-module for trait-specific types.
pub mod traits {
    pub use crate::observer::ObserverMetric;
}

/// Prometheus observer stub. Only functional with the `observability-prometheus` feature.
pub struct PrometheusObserver;

impl crate::observer::Observer for PrometheusObserver {
    fn record_event(&self, _event: &crate::observer::ObserverEvent) {}
    fn record_metric(&self, _metric: &crate::observer::ObserverMetric) {}
    fn name(&self) -> &str { "prometheus" }
    fn as_any(&self) -> &dyn std::any::Any { self }
}

impl PrometheusObserver {
    pub fn new() -> Self { Self }
    pub fn encode(&self) -> String { String::new() }
}

impl Default for PrometheusObserver {
    fn default() -> Self {
        Self::new()
    }
}

/// Create an observer based on configuration.
/// Currently returns a NoopObserver regardless of config.
pub fn create_observer(_config: &clawseed_config::schema::Config) -> Box<dyn crate::observer::Observer> {
    Box::new(crate::observer::NoopObserver)
}
