//! Platform abstraction stub.

use anyhow::Result;

/// Runtime adapter trait for platform-specific functionality.
pub trait RuntimeAdapter: Send + Sync + 'static {}

/// Create a runtime adapter from the configuration.
/// Returns a no-op adapter in the minimal agent crate.
pub fn create_runtime(_config: &clawseed_config::schema::Config) -> Result<Box<dyn RuntimeAdapter>> {
    // Return a no-op runtime adapter
    struct NoopRuntimeAdapter;
    impl RuntimeAdapter for NoopRuntimeAdapter {}
    Ok(Box::new(NoopRuntimeAdapter))
}
