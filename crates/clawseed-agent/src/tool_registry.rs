//! Default implementation of the ToolRegistry trait.
//!
//! Uses DashMap for lock-free concurrent access, safe in async contexts.
//! Caches ToolSpecs and invalidates on mutation.

use std::sync::Arc;

use dashmap::DashMap;
use parking_lot::RwLock;

use clawseed_api::tool::{Tool, ToolSpec};
use clawseed_api::tool_registry::{ToolEntry, ToolRegistry, ToolSource};

/// Default ToolRegistry implementation using DashMap for concurrent access.
pub struct DefaultToolRegistry {
    tools: DashMap<String, (Arc<dyn Tool>, ToolEntry)>,
    cached_specs: RwLock<Option<Vec<ToolSpec>>>,
    allowed_patterns: std::sync::RwLock<Vec<String>>,
    denied_patterns: std::sync::RwLock<Vec<String>>,
    mcp_tool_filters: std::collections::HashMap<String, Vec<String>>,
}

impl Default for DefaultToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl DefaultToolRegistry {
    pub fn new() -> Self {
        Self {
            tools: DashMap::new(),
            cached_specs: RwLock::new(None),
            allowed_patterns: std::sync::RwLock::new(Vec::new()),
            denied_patterns: std::sync::RwLock::new(Vec::new()),
            mcp_tool_filters: std::collections::HashMap::new(),
        }
    }

    /// Create a registry with filtering configuration.
    pub fn with_filters(
        allowed_tools: Vec<String>,
        denied_tools: Vec<String>,
        mcp_tool_filters: std::collections::HashMap<String, Vec<String>>,
    ) -> Self {
        Self {
            tools: DashMap::new(),
            cached_specs: RwLock::new(None),
            allowed_patterns: std::sync::RwLock::new(allowed_tools),
            denied_patterns: std::sync::RwLock::new(denied_tools),
            mcp_tool_filters,
        }
    }

    /// Update filter patterns at runtime (e.g. on config change).
    /// Invalidates cached specs so the next query reflects the new filters.
    pub fn update_filters(
        &self,
        allowed_tools: Vec<String>,
        denied_tools: Vec<String>,
    ) {
        *self.allowed_patterns.write().unwrap() = allowed_tools;
        *self.denied_patterns.write().unwrap() = denied_tools;
        *self.cached_specs.write() = None;
    }

    /// Check if a tool name is allowed based on config patterns.
    fn is_tool_allowed(&self, name: &str, source: &ToolSource) -> bool {
        let allowed = self.allowed_patterns.read().unwrap();
        let denied = self.denied_patterns.read().unwrap();
        // Denied takes precedence
        if denied.iter().any(|p| glob_match(name, p)) {
            return false;
        }
        // If allowed_patterns is empty, allow all (except MCP filters)
        if allowed.is_empty() {
            if let ToolSource::Mcp { server } = source
                && let Some(filters) = self.mcp_tool_filters.get(server)
            {
                return filters.iter().any(|p| glob_match(name, p));
            }
            return true;
        }
        // Check allowed patterns
        allowed.iter().any(|p| glob_match(name, p))
    }

    /// Register a tool, replacing any existing tool with the same name.
    /// Returns the previous entry if one was replaced.
    pub fn register_or_replace(
        &self,
        tool: Box<dyn Tool>,
        source: ToolSource,
    ) -> Option<ToolEntry> {
        let name = tool.name().to_string();
        let arc: Arc<dyn Tool> = Arc::from(tool);
        let entry = ToolEntry { source };

        let old = self.tools.insert(name, (arc, entry.clone()));

        // Invalidate cached specs
        *self.cached_specs.write() = None;

        old.map(|(_, e)| e)
    }

    /// Bulk-register tools from a Vec, all with the same source.
    /// Returns the number of tools successfully registered.
    pub fn register_all(&self, tools: Vec<Box<dyn Tool>>, source: ToolSource) -> usize {
        let mut count = 0;
        for tool in tools {
            let name = tool.name().to_string();
            use dashmap::mapref::entry::Entry;
            match self.tools.entry(name) {
                Entry::Occupied(_) => continue,
                Entry::Vacant(entry) => {
                    let arc: Arc<dyn Tool> = Arc::from(tool);
                    let entry_meta = ToolEntry {
                        source: source.clone(),
                    };
                    entry.insert((arc, entry_meta));
                    count += 1;
                }
            }
        }
        if count > 0 {
            *self.cached_specs.write() = None;
        }
        count
    }

    /// Register a pre-Arced tool. Returns false if a tool with the same name already exists.
    pub fn register_arc(&self, tool: Arc<dyn Tool>, source: ToolSource) -> bool {
        let name = tool.name().to_string();
        use dashmap::mapref::entry::Entry;
        match self.tools.entry(name) {
            Entry::Occupied(_) => false,
            Entry::Vacant(entry) => {
                let entry_meta = ToolEntry { source };
                entry.insert((tool, entry_meta));
                *self.cached_specs.write() = None;
                true
            }
        }
    }

    /// Bulk-register pre-Arced tools, all with the same source.
    /// Returns the number of tools successfully registered.
    pub fn register_all_arc(&self, tools: Vec<Arc<dyn Tool>>, source: ToolSource) -> usize {
        let mut count = 0;
        for tool in tools {
            let name = tool.name().to_string();
            use dashmap::mapref::entry::Entry;
            match self.tools.entry(name) {
                Entry::Occupied(_) => continue,
                Entry::Vacant(entry) => {
                    let entry_meta = ToolEntry {
                        source: source.clone(),
                    };
                    entry.insert((tool, entry_meta));
                    count += 1;
                }
            }
        }
        if count > 0 {
            *self.cached_specs.write() = None;
        }
        count
    }

    /// Remove all tools matching a given source.
    /// Returns the number of tools removed.
    pub fn unregister_by_source(&self, source: &ToolSource) -> usize {
        let keys_to_remove: Vec<String> = self
            .tools
            .iter()
            .filter(|entry| &entry.value().1.source == source)
            .map(|entry| entry.key().clone())
            .collect();

        let count = keys_to_remove.len();
        for key in keys_to_remove {
            self.tools.remove(&key);
        }
        if count > 0 {
            *self.cached_specs.write() = None;
        }
        count
    }

    fn rebuild_specs(&self) -> Vec<ToolSpec> {
        self.tools
            .iter()
            .filter(|entry| self.is_tool_allowed(entry.key(), &entry.value().1.source))
            .map(|entry| entry.value().0.spec())
            .collect()
    }
}

/// Check if a tool name matches a glob pattern.
/// Supports `*` (any sequence) and `?` (single char).
fn glob_match(name: &str, pattern: &str) -> bool {
    glob_match::glob_match(pattern, name)
}

impl ToolRegistry for DefaultToolRegistry {
    fn register(&self, tool: Box<dyn Tool>, source: ToolSource) -> bool {
        let name = tool.name().to_string();
        use dashmap::mapref::entry::Entry;
        match self.tools.entry(name) {
            Entry::Occupied(_) => false,
            Entry::Vacant(entry) => {
                let arc: Arc<dyn Tool> = Arc::from(tool);
                let entry_meta = ToolEntry { source };
                entry.insert((arc, entry_meta));
                *self.cached_specs.write() = None;
                true
            }
        }
    }

    fn register_or_replace(&self, tool: Box<dyn Tool>, source: ToolSource) -> Option<ToolEntry> {
        self.register_or_replace(tool, source)
    }

    fn unregister_by_source(&self, source: &ToolSource) -> usize {
        self.unregister_by_source(source)
    }

    fn unregister(&self, name: &str) -> bool {
        if self.tools.remove(name).is_some() {
            *self.cached_specs.write() = None;
            true
        } else {
            false
        }
    }

    fn get_tool(&self, name: &str) -> Option<Arc<dyn Tool>> {
        self.tools.get(name).and_then(|entry| {
            if self.is_tool_allowed(entry.key(), &entry.value().1.source) {
                Some(Arc::clone(&entry.value().0))
            } else {
                None
            }
        })
    }

    fn get_tool_unfiltered(&self, name: &str) -> Option<Arc<dyn Tool>> {
        self.tools
            .get(name)
            .map(|entry| Arc::clone(&entry.value().0))
    }

    fn tool_specs(&self) -> Vec<ToolSpec> {
        // Fast path: return cached specs if available
        if let Some(specs) = self.cached_specs.read().as_ref() {
            return specs.clone();
        }
        // Slow path: rebuild under write lock to prevent stale-cache race
        let mut cache = self.cached_specs.write();
        // Double-check after acquiring write lock
        if let Some(ref specs) = *cache {
            return specs.clone();
        }
        let specs = self.rebuild_specs();
        *cache = Some(specs.clone());
        specs
    }

    fn get_entry(&self, name: &str) -> Option<ToolEntry> {
        self.tools.get(name).map(|entry| entry.value().1.clone())
    }

    fn tool_names(&self) -> Vec<String> {
        self.tools
            .iter()
            .filter(|entry| self.is_tool_allowed(entry.key(), &entry.value().1.source))
            .map(|entry| entry.key().clone())
            .collect()
    }

    fn all_tool_names(&self) -> Vec<String> {
        self.tools.iter().map(|entry| entry.key().clone()).collect()
    }

    fn is_tool_enabled(&self, name: &str) -> bool {
        self.tools
            .get(name)
            .map(|entry| self.is_tool_allowed(entry.key(), &entry.value().1.source))
            .unwrap_or(false)
    }

    fn len(&self) -> usize {
        self.tools.len()
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clawseed_api::tool::{Tool, ToolResult};
    use clawseed_api::tool_context::ToolContext;
    use serde_json::Value;

    struct MockTool {
        name: String,
    }

    impl MockTool {
        fn new(name: &str) -> Self {
            Self {
                name: name.to_string(),
            }
        }
    }

    #[async_trait::async_trait]
    impl Tool for MockTool {
        fn name(&self) -> &str {
            &self.name
        }
        fn description(&self) -> &str {
            "mock"
        }
        fn parameters_schema(&self) -> Value {
            serde_json::json!({"type": "object"})
        }
        async fn execute(
            &self,
            _args: Value,
            _ctx: &dyn ToolContext,
        ) -> anyhow::Result<ToolResult> {
            Ok(ToolResult {
                success: true,
                output: "ok".into(),
                error: None,
            })
        }
    }

    #[test]
    fn test_register_and_lookup() {
        let registry = DefaultToolRegistry::new();
        assert!(registry.register(Box::new(MockTool::new("foo")), ToolSource::BuiltIn));
        assert!(!registry.register(Box::new(MockTool::new("foo")), ToolSource::BuiltIn)); // duplicate

        assert!(registry.get_tool("foo").is_some());
        assert!(registry.get_tool("bar").is_none());
        assert_eq!(registry.len(), 1);
    }

    #[test]
    fn test_unregister() {
        let registry = DefaultToolRegistry::new();
        registry.register(Box::new(MockTool::new("foo")), ToolSource::BuiltIn);
        assert!(registry.unregister("foo"));
        assert!(!registry.unregister("foo")); // already removed
        assert_eq!(registry.len(), 0);
    }

    #[test]
    fn test_register_or_replace() {
        let registry = DefaultToolRegistry::new();
        registry.register(Box::new(MockTool::new("foo")), ToolSource::BuiltIn);
        let old = registry.register_or_replace(
            Box::new(MockTool::new("foo")),
            ToolSource::Remote {
                session: "s1".into(),
            },
        );
        assert!(old.is_some());
        assert_eq!(registry.len(), 1);
        assert_eq!(
            registry.get_entry("foo").unwrap().source,
            ToolSource::Remote {
                session: "s1".into()
            }
        );
    }

    #[test]
    fn test_unregister_by_source() {
        let registry = DefaultToolRegistry::new();
        registry.register(Box::new(MockTool::new("a")), ToolSource::BuiltIn);
        registry.register(
            Box::new(MockTool::new("b")),
            ToolSource::Mcp {
                server: "svc".into(),
            },
        );
        registry.register(
            Box::new(MockTool::new("c")),
            ToolSource::Mcp {
                server: "svc".into(),
            },
        );
        registry.register(
            Box::new(MockTool::new("d")),
            ToolSource::Mcp {
                server: "other".into(),
            },
        );

        let removed = registry.unregister_by_source(&ToolSource::Mcp {
            server: "svc".into(),
        });
        assert_eq!(removed, 2);
        assert_eq!(registry.len(), 2);
        assert!(registry.get_tool("a").is_some());
        assert!(registry.get_tool("d").is_some());
    }

    #[test]
    fn test_tool_specs_cached() {
        let registry = DefaultToolRegistry::new();
        registry.register(Box::new(MockTool::new("foo")), ToolSource::BuiltIn);

        let specs1 = registry.tool_specs();
        let specs2 = registry.tool_specs();
        assert_eq!(specs1.len(), 1);
        assert_eq!(specs1[0].name, "foo");
        assert_eq!(specs1.len(), specs2.len());
    }

    #[test]
    fn test_register_all() {
        let registry = DefaultToolRegistry::new();
        let tools: Vec<Box<dyn Tool>> =
            vec![Box::new(MockTool::new("a")), Box::new(MockTool::new("b"))];
        let count = registry.register_all(tools, ToolSource::BuiltIn);
        assert_eq!(count, 2);
        assert_eq!(registry.len(), 2);
    }

    #[test]
    fn test_tool_names() {
        let registry = DefaultToolRegistry::new();
        registry.register(Box::new(MockTool::new("foo")), ToolSource::BuiltIn);
        registry.register(Box::new(MockTool::new("bar")), ToolSource::BuiltIn);

        let mut names = registry.tool_names();
        names.sort();
        assert_eq!(names, vec!["bar", "foo"]);
    }

    #[test]
    fn test_denied_tool_not_returned_by_get_tool() {
        let registry = DefaultToolRegistry::with_filters(
            vec![],
            vec!["shell".to_string()],
            std::collections::HashMap::new(),
        );
        registry.register(Box::new(MockTool::new("calculator")), ToolSource::BuiltIn);
        registry.register(Box::new(MockTool::new("shell")), ToolSource::BuiltIn);

        assert!(registry.get_tool("calculator").is_some());
        assert!(registry.get_tool("shell").is_none(), "denied tool should not be returned by get_tool");
    }

    #[test]
    fn test_denied_tool_excluded_from_tool_names() {
        let registry = DefaultToolRegistry::with_filters(
            vec![],
            vec!["shell".to_string()],
            std::collections::HashMap::new(),
        );
        registry.register(Box::new(MockTool::new("calculator")), ToolSource::BuiltIn);
        registry.register(Box::new(MockTool::new("shell")), ToolSource::BuiltIn);

        let names = registry.tool_names();
        assert_eq!(names, vec!["calculator"]);
    }

    #[test]
    fn test_allowed_tool_filter() {
        let registry = DefaultToolRegistry::with_filters(
            vec!["file_*".to_string()],
            vec![],
            std::collections::HashMap::new(),
        );
        registry.register(Box::new(MockTool::new("file_read")), ToolSource::BuiltIn);
        registry.register(Box::new(MockTool::new("shell")), ToolSource::BuiltIn);

        assert!(registry.get_tool("file_read").is_some());
        assert!(registry.get_tool("shell").is_none(), "non-allowed tool should not be returned");
        assert_eq!(registry.tool_names(), vec!["file_read"]);
    }
}
