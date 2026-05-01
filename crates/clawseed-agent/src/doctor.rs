//! Diagnostics module stub.

use clawseed_config::schema::Config;
use serde::Serialize;

/// Diagnostic severity level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Ok,
    Warn,
    Error,
}

/// A single diagnostic result.
#[derive(Debug, Clone, Serialize)]
pub struct DiagnosticResult {
    pub name: String,
    pub severity: Severity,
    pub message: String,
}

/// Run diagnostics against the given configuration.
pub fn diagnose(_config: &Config) -> Vec<DiagnosticResult> {
    Vec::new()
}
