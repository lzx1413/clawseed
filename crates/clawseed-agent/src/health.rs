//! Health monitoring — no-op stubs.
//!
//! Real health monitoring is an extension concern. The core agent provides
//! these stubs so that cron and other subsystems can call them without
//! conditional logic.

/// Mark a component as healthy (no-op).
pub fn mark_component_ok(_component: &str) {}

/// Mark a component as errored (no-op).
pub fn mark_component_error(_component: &str, _error: String) {}

/// Return a JSON snapshot of component health (empty object).
pub fn snapshot_json() -> serde_json::Value {
    serde_json::json!({})
}
