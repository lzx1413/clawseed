//! ContextProvider — extension mechanism for ToolContext.
//!
//! Extensions register ContextProviders to add capabilities that tools
//! can discover via `ctx.get::<T>()`.

use std::any::Any;

/// ContextProvider trait — extensions implement this to inject capabilities
/// into the ToolContext capability bag.
pub trait ContextProvider: Send + Sync {
    /// Provide a capability object. `AgentToolContext::get::<T>()` queries this.
    fn as_any(&self) -> &dyn Any;
}
