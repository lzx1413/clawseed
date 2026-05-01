//! Server-Sent Events (SSE) for real-time event streaming.
//!
//! Stubs — full implementation deferred.

use axum::{
    extract::State,
    response::IntoResponse,
};
use parking_lot::Mutex;
use std::collections::VecDeque;
use std::sync::Arc;

use crate::AppState;
use clawseed_agent::observability::{Observer, ObserverEvent, ObserverMetric};

/// Ring buffer of recent events for history replay.
pub struct EventBuffer {
    events: Mutex<VecDeque<serde_json::Value>>,
    capacity: usize,
}

impl EventBuffer {
    pub fn new(capacity: usize) -> Self {
        Self {
            events: Mutex::new(VecDeque::with_capacity(capacity)),
            capacity,
        }
    }

    pub fn push(&self, event: serde_json::Value) {
        let mut events = self.events.lock();
        if events.len() >= self.capacity {
            events.pop_front();
        }
        events.push_back(event);
    }

    pub fn recent(&self, limit: usize) -> Vec<serde_json::Value> {
        let events = self.events.lock();
        events.iter().rev().take(limit).cloned().collect()
    }
}

/// Observer that broadcasts events to SSE and records them in an event buffer.
pub struct BroadcastObserver {
    inner: Box<dyn Observer>,
    event_tx: tokio::sync::broadcast::Sender<serde_json::Value>,
    event_buffer: Arc<EventBuffer>,
}

impl BroadcastObserver {
    pub fn new(
        inner: Box<dyn Observer>,
        event_tx: tokio::sync::broadcast::Sender<serde_json::Value>,
        event_buffer: Arc<EventBuffer>,
    ) -> Self {
        Self {
            inner,
            event_tx,
            event_buffer,
        }
    }

    pub fn inner(&self) -> &dyn Observer {
        self.inner.as_ref()
    }
}

impl Observer for BroadcastObserver {
    fn record_event(&self, event: &ObserverEvent) {
        // Broadcast event to SSE subscribers
        let json = serde_json::to_value(format!("{:?}", event)).unwrap_or_else(|_| serde_json::json!({"type": "unknown"}));
        let _ = self.event_tx.send(json.clone());
        self.event_buffer.push(json);
        self.inner.record_event(event);
    }

    fn record_metric(&self, metric: &ObserverMetric) {
        self.inner.record_metric(metric);
    }

    fn name(&self) -> &str {
        "broadcast"
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

/// GET /api/events — SSE event stream
pub async fn handle_sse_events(State(_state): State<AppState>) -> impl IntoResponse {
    (
        axum::http::StatusCode::NOT_IMPLEMENTED,
        "SSE event stream not yet implemented",
    )
}

/// GET /api/events/history — recent event history
pub async fn handle_events_history(State(state): State<AppState>) -> impl IntoResponse {
    let events = state.event_buffer.recent(100);
    axum::Json(serde_json::json!({"events": events}))
}
