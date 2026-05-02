//! Channel trait stub for message channels.

use async_trait::async_trait;

/// Channel trait for receiving and sending messages.
#[async_trait]
pub trait Channel: Send + Sync + 'static {
    /// Get the channel name.
    fn name(&self) -> &str;

    /// Send a message through the channel.
    async fn send(&self, message: &str) -> anyhow::Result<()>;
}
