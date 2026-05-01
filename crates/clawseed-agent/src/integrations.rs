//! Integrations registry stub.

use clawseed_config::schema::Config;
use serde::Serialize;

/// Integration status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum IntegrationStatus {
    Active,
    Inactive,
    Misconfigured,
}

/// An integration registry entry.
pub struct IntegrationEntry {
    pub name: &'static str,
    pub description: &'static str,
    pub category: &'static str,
    pub status_fn: fn(&Config) -> IntegrationStatus,
}

/// List all known integrations.
pub fn all_integrations() -> Vec<IntegrationEntry> {
    Vec::new()
}

/// Registry sub-module (gateway references `integrations::registry`).
pub mod registry {
    pub use super::*;
}
