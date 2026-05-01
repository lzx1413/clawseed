//! Pairing API handlers and device management stubs.

use axum::{
    extract::Path,
    http::StatusCode,
    response::{IntoResponse, Json},
};
use serde::{Deserialize, Serialize};

/// Registered device information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceInfo {
    pub id: String,
    pub name: String,
    pub created_at: String,
}

/// Device registry for managing paired devices.
pub struct DeviceRegistry {
    _workspace_dir: std::path::PathBuf,
}

impl DeviceRegistry {
    pub fn new(workspace_dir: &std::path::Path) -> Self {
        Self {
            _workspace_dir: workspace_dir.to_path_buf(),
        }
    }
}

/// Pending pairing request store.
pub struct PairingStore {
    _max_pending: usize,
}

impl PairingStore {
    pub fn new(max_pending: usize) -> Self {
        Self { _max_pending: max_pending }
    }
}

/// POST /api/pairing/initiate — initiate a pairing request
pub async fn initiate_pairing() -> impl IntoResponse {
    (
        StatusCode::NOT_IMPLEMENTED,
        Json(serde_json::json!({"error": "Pairing API not yet implemented"})),
    )
}

/// POST /api/pair — enhanced pairing endpoint
pub async fn submit_pairing_enhanced() -> impl IntoResponse {
    (
        StatusCode::NOT_IMPLEMENTED,
        Json(serde_json::json!({"error": "Pairing API not yet implemented"})),
    )
}

/// GET /api/devices — list paired devices
pub async fn list_devices() -> impl IntoResponse {
    Json(serde_json::json!({"devices": []}))
}

/// DELETE /api/devices/{id} — revoke a device
pub async fn revoke_device(Path(_id): Path<String>) -> impl IntoResponse {
    (
        StatusCode::NOT_IMPLEMENTED,
        Json(serde_json::json!({"error": "Device revocation not yet implemented"})),
    )
}

/// POST /api/devices/{id}/token/rotate — rotate device token
pub async fn rotate_token(Path(_id): Path<String>) -> impl IntoResponse {
    (
        StatusCode::NOT_IMPLEMENTED,
        Json(serde_json::json!({"error": "Token rotation not yet implemented"})),
    )
}
