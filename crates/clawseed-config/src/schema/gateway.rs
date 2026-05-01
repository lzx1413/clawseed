//! Gateway configuration.

use serde::{Deserialize, Serialize};

/// Gateway configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatewayConfig {
    /// Bind host.
    #[serde(default = "default_host")]
    pub host: String,

    /// Bind port.
    #[serde(default = "default_port")]
    pub port: u16,

    /// Request timeout in seconds.
    #[serde(default = "default_timeout_secs")]
    pub timeout_secs: u64,

    /// Path prefix for all routes (e.g. "/api").
    #[serde(default)]
    pub path_prefix: Option<String>,

    /// Enable session persistence.
    #[serde(default)]
    pub session_persistence: bool,

    /// Session TTL in hours.
    #[serde(default = "default_session_ttl_hours")]
    pub session_ttl_hours: u32,

    /// TLS configuration.
    #[serde(default)]
    pub tls: Option<GatewayTlsConfig>,

    /// Enable CORS.
    #[serde(default = "default_true")]
    pub enable_cors: bool,

    /// Whether pairing is required for access.
    #[serde(default)]
    pub require_pairing: bool,

    /// Paired bearer tokens (encrypted at rest).
    #[serde(default)]
    pub paired_tokens: Vec<String>,

    /// Allow binding to public interfaces without a tunnel.
    #[serde(default)]
    pub allow_public_bind: bool,

    /// Rate limit for pairing endpoint (requests per minute per IP).
    #[serde(default = "default_pair_rate_limit")]
    pub pair_rate_limit_per_minute: u32,

    /// Rate limit for webhook endpoint (requests per minute per IP).
    #[serde(default = "default_webhook_rate_limit")]
    pub webhook_rate_limit_per_minute: u32,

    /// Maximum number of distinct IP keys tracked in rate limiter.
    #[serde(default)]
    pub rate_limit_max_keys: usize,

    /// TTL in seconds for idempotency keys.
    #[serde(default = "default_idempotency_ttl_secs")]
    pub idempotency_ttl_secs: u64,

    /// Maximum number of idempotency keys retained in memory.
    #[serde(default)]
    pub idempotency_max_keys: usize,

    /// Trust X-Forwarded-For / X-Real-IP headers for rate limiting.
    #[serde(default)]
    pub trust_forwarded_headers: bool,

    /// Filesystem path to web/dist/ for serving the dashboard.
    #[serde(default)]
    pub web_dist_dir: Option<String>,

    /// Pairing dashboard configuration.
    #[serde(default)]
    pub pairing_dashboard: PairingDashboardConfig,
}

/// Pairing dashboard configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PairingDashboardConfig {
    /// Maximum number of pending pairing codes.
    #[serde(default = "default_max_pending_codes")]
    pub max_pending_codes: usize,
}

fn default_max_pending_codes() -> usize { 10 }

fn default_pair_rate_limit() -> u32 { 10 }
fn default_webhook_rate_limit() -> u32 { 30 }
fn default_idempotency_ttl_secs() -> u64 { 300 }

/// TLS configuration for the gateway.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatewayTlsConfig {
    pub cert_path: String,
    pub key_path: String,
    /// Client certificate authentication configuration.
    #[serde(default)]
    pub client_auth: Option<GatewayClientAuthConfig>,
}

/// Client certificate authentication configuration for mTLS.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatewayClientAuthConfig {
    /// Whether client certificate authentication is enabled.
    #[serde(default)]
    pub enabled: bool,
    /// Path to the CA certificate for verifying client certificates.
    pub ca_cert_path: String,
    /// Whether to require a client certificate (vs. optional).
    #[serde(default = "default_true")]
    pub require_client_cert: bool,
    /// Pinned certificate fingerprints (SHA-256 hex).
    #[serde(default)]
    pub pinned_certs: Vec<String>,
}

fn default_host() -> String { "127.0.0.1".into() }
fn default_port() -> u16 { 3000 }
fn default_timeout_secs() -> u64 { 300 }
fn default_session_ttl_hours() -> u32 { 24 }
fn default_true() -> bool { true }

impl Default for GatewayConfig {
    fn default() -> Self {
        Self {
            host: default_host(),
            port: default_port(),
            timeout_secs: default_timeout_secs(),
            path_prefix: None,
            session_persistence: false,
            session_ttl_hours: default_session_ttl_hours(),
            tls: None,
            enable_cors: true,
            require_pairing: false,
            paired_tokens: Vec::new(),
            allow_public_bind: false,
            pair_rate_limit_per_minute: default_pair_rate_limit(),
            webhook_rate_limit_per_minute: default_webhook_rate_limit(),
            rate_limit_max_keys: 0,
            idempotency_ttl_secs: default_idempotency_ttl_secs(),
            idempotency_max_keys: 0,
            trust_forwarded_headers: false,
            web_dist_dir: None,
            pairing_dashboard: PairingDashboardConfig::default(),
        }
    }
}

impl Default for PairingDashboardConfig {
    fn default() -> Self {
        Self {
            max_pending_codes: default_max_pending_codes(),
        }
    }
}
