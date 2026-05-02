//! AgentToolContext — the capability bag for tool execution.
//!
//! Extensions define their own types and register them via ContextProvider.
//! Tools query for capabilities with `ctx.get::<T>()` — if the type isn't
//! available, the tool simply skips that check.

use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Trait for objects that can provide a typed capability to the tool context.
///
/// Extensions implement this to inject capabilities (e.g. SecurityPolicy,
/// Provider handle) without the core agent knowing about them.
pub trait ContextProvider: Send + Sync {
    /// The concrete type this provider supplies.
    fn provided_type_id(&self) -> TypeId;

    /// Return the capability as a boxed Any for storage.
    fn into_any_arc(self: Box<Self>) -> Arc<dyn Any + Send + Sync>;
}

/// Implementation of ContextProvider for any `Send + Sync + 'static` type.
pub struct TypedProvider<T: Send + Sync + 'static> {
    value: Arc<T>,
}

impl<T: Send + Sync + 'static> TypedProvider<T> {
    pub fn new(value: Arc<T>) -> Self {
        Self { value }
    }
}

impl<T: Send + Sync + 'static> ContextProvider for TypedProvider<T> {
    fn provided_type_id(&self) -> TypeId {
        TypeId::of::<T>()
    }

    fn into_any_arc(self: Box<Self>) -> Arc<dyn Any + Send + Sync> {
        self.value as Arc<dyn Any + Send + Sync>
    }
}

/// The real tool execution context — holds workspace dir and registered providers.
pub struct AgentToolContext {
    workspace_dir: PathBuf,
    /// TypeId → Arc<dyn Any> for O(1) capability lookup.
    capabilities: HashMap<TypeId, Arc<dyn Any + Send + Sync>>,
}

impl AgentToolContext {
    pub fn new(workspace_dir: PathBuf) -> Self {
        Self {
            workspace_dir,
            capabilities: HashMap::new(),
        }
    }

    /// Convenience: add a typed capability directly.
    pub fn with_capability<T: Send + Sync + 'static>(mut self, value: Arc<T>) -> Self {
        self.capabilities.insert(TypeId::of::<T>(), value);
        self
    }

    /// Convenience: add a typed capability directly (mutable).
    pub fn add_capability<T: Send + Sync + 'static>(&mut self, value: Arc<T>) {
        self.capabilities.insert(TypeId::of::<T>(), value);
    }

    /// Add a provider from a boxed ContextProvider.
    pub fn add_boxed_provider(&mut self, provider: Box<dyn ContextProvider>) {
        let type_id = provider.provided_type_id();
        let arc = provider.into_any_arc();
        self.capabilities.insert(type_id, arc);
    }

    /// Add a pre-built Arc capability by TypeId (used by Agent to share capabilities).
    pub fn add_arc(&mut self, type_id: TypeId, arc: Arc<dyn Any + Send + Sync>) {
        self.capabilities.insert(type_id, arc);
    }
}

impl clawseed_api::tool_context::ToolContext for AgentToolContext {
    fn workspace_dir(&self) -> &Path {
        &self.workspace_dir
    }

    fn get_any(&self, type_id: TypeId) -> Option<&(dyn Any + Send + Sync)> {
        self.capabilities
            .get(&type_id)
            .map(|arc| arc.as_ref() as &(dyn Any + Send + Sync))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clawseed_api::tool_context::ToolContextExt;

    struct FakePolicy {
        name: String,
    }

    #[test]
    fn get_returns_registered_capability() {
        let policy = Arc::new(FakePolicy {
            name: "test".into(),
        });
        let ctx = AgentToolContext::new(PathBuf::from("/tmp")).with_capability(policy);

        // Use the dyn ToolContext to access get() via the extension trait
        let ctx_ref: &dyn clawseed_api::tool_context::ToolContext = &ctx;
        let found: &FakePolicy = ctx_ref.get::<FakePolicy>().expect("should find capability");
        assert_eq!(found.name, "test");
    }

    #[test]
    fn get_returns_none_for_unregistered_type() {
        let ctx = AgentToolContext::new(PathBuf::from("/tmp"));
        let ctx_ref: &dyn clawseed_api::tool_context::ToolContext = &ctx;
        assert!(ctx_ref.get::<FakePolicy>().is_none());
    }

    #[test]
    fn workspace_dir_returns_correct_path() {
        let ctx = AgentToolContext::new(PathBuf::from("/workspace"));
        use clawseed_api::tool_context::ToolContext;
        assert_eq!(ctx.workspace_dir(), Path::new("/workspace"));
    }
}
