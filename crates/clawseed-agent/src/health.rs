//! Health monitoring stubs for the minimal agent crate.

/// Mark a component as healthy (no-op in minimal agent).
pub fn mark_component_ok(_component: &str) {
    // No-op in minimal agent crate
}

/// Mark a component as unhealthy (no-op in minimal agent).
pub fn mark_component_error(_component: &str, _error: impl Into<String>) {
    // No-op in minimal agent crate
}

/// Get a JSON snapshot of component health (returns empty object in minimal agent).
pub fn snapshot_json() -> serde_json::Value {
    serde_json::json!({"components": {}})
}

/// Get a JSON snapshot of component health.
/// Alias for `snapshot_json` used by the gateway.
pub fn snapshot() -> serde_json::Value {
    snapshot_json()
}
