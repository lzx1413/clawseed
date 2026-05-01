//! WebAuthn API handlers stub.
//!
//! Gated behind the `webauthn` feature flag.

use std::sync::Arc;
use parking_lot::Mutex;
use std::collections::HashMap;

/// WebAuthn state for hardware key authentication.
pub struct WebAuthnState {
    /// Placeholder for WebAuthn manager.
    pub pending_registrations: Mutex<HashMap<String, String>>,
    pub pending_authentications: Mutex<HashMap<String, String>>,
}

/// POST /api/webauthn/register/start
pub async fn handle_register_start() -> impl axum::response::IntoResponse {
    (
        axum::http::StatusCode::NOT_IMPLEMENTED,
        axum::Json(serde_json::json!({"error": "WebAuthn not yet implemented"})),
    )
}

/// POST /api/webauthn/register/finish
pub async fn handle_register_finish() -> impl axum::response::IntoResponse {
    (
        axum::http::StatusCode::NOT_IMPLEMENTED,
        axum::Json(serde_json::json!({"error": "WebAuthn not yet implemented"})),
    )
}

/// POST /api/webauthn/auth/start
pub async fn handle_auth_start() -> impl axum::response::IntoResponse {
    (
        axum::http::StatusCode::NOT_IMPLEMENTED,
        axum::Json(serde_json::json!({"error": "WebAuthn not yet implemented"})),
    )
}

/// POST /api/webauthn/auth/finish
pub async fn handle_auth_finish() -> impl axum::response::IntoResponse {
    (
        axum::http::StatusCode::NOT_IMPLEMENTED,
        axum::Json(serde_json::json!({"error": "WebAuthn not yet implemented"})),
    )
}

/// GET /api/webauthn/credentials
pub async fn handle_list_credentials() -> impl axum::response::IntoResponse {
    axum::Json(serde_json::json!({"credentials": []}))
}

/// DELETE /api/webauthn/credentials/{id}
pub async fn handle_delete_credential() -> impl axum::response::IntoResponse {
    (
        axum::http::StatusCode::NOT_IMPLEMENTED,
        axum::Json(serde_json::json!({"error": "WebAuthn not yet implemented"})),
    )
}
