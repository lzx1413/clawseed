//! ToolContext — the capability bag.
//!
//! Extensions define their own types and register them via ContextProvider.
//! Tools query for capabilities with `ctx.get::<T>()` — if the type isn't
//! available, the tool simply skips that check.
//!
//! # Design
//!
//! `ToolContext` uses a non-generic `get_any` method for dyn compatibility.
//! The generic `get::<T>()` is provided as an extension method.
//!
//! ```ignore
//! fn execute(&self, args: Value, ctx: &dyn ToolContext) -> Result<ToolResult> {
//!     if let Some(mem) = ctx.get::<Arc<dyn Memory>>() {
//!         mem.store("key", "value").await?;
//!     }
//!     // execute...
//! }
//! ```

use std::any::{Any, TypeId};
use std::path::Path;

/// Core trait for tool execution context.
pub trait ToolContext: Send + Sync {
    /// The workspace directory for file operations.
    fn workspace_dir(&self) -> &Path;

    /// Query a capability by TypeId. Implementations typically iterate
    /// over registered ContextProviders and match by type.
    fn get_any(&self, type_id: TypeId) -> Option<&(dyn Any + Send + Sync)>;
}

/// Extension trait for type-safe capability lookup on `dyn ToolContext`.
pub trait ToolContextExt {
    /// Query a capability by type. Extensions define their own types;
    /// the core never changes. Returns `None` if the capability isn't available.
    fn get<T: 'static>(&self) -> Option<&T>;
}

impl ToolContextExt for dyn ToolContext {
    fn get<T: 'static>(&self) -> Option<&T> {
        self.get_any(TypeId::of::<T>())
            .and_then(|any| any.downcast_ref::<T>())
    }
}

impl ToolContextExt for dyn ToolContext + Send {
    fn get<T: 'static>(&self) -> Option<&T> {
        self.get_any(TypeId::of::<T>())
            .and_then(|any| any.downcast_ref::<T>())
    }
}

impl ToolContextExt for dyn ToolContext + Send + Sync {
    fn get<T: 'static>(&self) -> Option<&T> {
        self.get_any(TypeId::of::<T>())
            .and_then(|any| any.downcast_ref::<T>())
    }
}
