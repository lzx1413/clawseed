//! Tunnel abstraction stub.

use anyhow::Result;

/// Tunnel trait for exposing the gateway publicly.
pub trait Tunnel: Send + Sync + 'static {
    fn name(&self) -> &str;
    fn start(&self, host: &str, port: u16) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<String>> + Send + '_>>;
}

/// Create a tunnel from the configuration.
/// Returns None if no tunnel is configured.
pub fn create_tunnel(_config: &clawseed_config::schema::Config) -> Result<Option<Box<dyn Tunnel>>> {
    Ok(None)
}
