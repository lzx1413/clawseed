//! REST API handlers for the web dashboard.
//!
//! All `/api/*` routes require bearer token authentication (PairingGuard).

use super::AppState;
use axum::{
    http::{HeaderMap, StatusCode, header},
    response::Json,
};

mod attachments;
mod config;
mod config_secrets;
mod cron;
mod hooks;
mod identity;
mod integrations;
mod memory;
mod providers;
mod sessions;
mod skills;
mod status;
mod system;

pub use self::{
    attachments::*, config::*, cron::*, hooks::*, identity::*, integrations::*, memory::*,
    providers::*, sessions::*, skills::*, status::*, system::*,
};
use config_secrets::{hydrate_config_for_save, mask_sensitive_fields};

#[cfg(test)]
mod tests;

const MASKED_SECRET: &str = "***MASKED***";

// ── Bearer token auth extractor ─────────────────────────────────

/// Extract and validate bearer token from Authorization header.
fn extract_bearer_token(headers: &HeaderMap) -> Option<&str> {
    headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|auth| auth.strip_prefix("Bearer "))
}

/// Verify bearer token against PairingGuard. Returns error response if unauthorized.
pub(super) fn require_auth(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<(), (StatusCode, Json<serde_json::Value>)> {
    if !state.pairing.require_pairing() {
        return Ok(());
    }

    let token = extract_bearer_token(headers).unwrap_or("");
    if state.pairing.is_authenticated(token) {
        Ok(())
    } else {
        Err((
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({
                "error": "Unauthorized — pair first via POST /pair, then send Authorization: Bearer <token>"
            })),
        ))
    }
}

// Domain handlers are implemented in sibling modules and re-exported above so
// existing `api::handle_*` route paths remain stable.
