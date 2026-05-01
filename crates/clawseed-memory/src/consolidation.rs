//! Memory consolidation stub.

use clawseed_api::memory_traits::Memory;
use clawseed_api::provider::Provider;

/// Consolidate a conversation turn into memory (stub).
pub async fn consolidate_turn(
    _provider: &dyn Provider,
    _model: &str,
    _memory: &dyn Memory,
    _user_msg: &str,
    _assistant_resp: &str,
) -> anyhow::Result<()> {
    // No-op in minimal crate
    Ok(())
}
