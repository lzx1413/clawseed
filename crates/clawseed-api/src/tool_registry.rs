//! Tool registry trait and types.
//!
//! Provides a unified interface for registering and looking up tools
//! regardless of their source (built-in, MCP, remote).

use std::sync::Arc;

use crate::tool::{Tool, ToolSpec};

/// Provenance of a registered tool.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ToolSource {
    /// Built-in tool shipped with ClawSeed.
    BuiltIn,
    /// Tool provided by an MCP server.
    Mcp { server: String },
    /// Tool registered by a remote client (e.g., Android via WebSocket).
    Remote { session: String },
}

/// Metadata about a registered tool entry.
#[derive(Debug, Clone)]
pub struct ToolEntry {
    pub source: ToolSource,
}

/// Core registry trait — all tool sources register through this interface.
///
/// Implementations must be safe for concurrent access from async contexts.
/// Tool lookup (`get_tool`) returns `Arc<dyn Tool>` to avoid lifetime issues
/// with interior mutability across await points.
pub trait ToolRegistry: Send + Sync {
    /// Register a tool. Returns false if a tool with the same name already exists.
    fn register(&self, tool: Box<dyn Tool>, source: ToolSource) -> bool;

    /// Remove a tool by name. Returns true if a tool was removed.
    fn unregister(&self, name: &str) -> bool;

    /// Look up a tool by name for execution.
    fn get_tool(&self, name: &str) -> Option<Arc<dyn Tool>>;

    /// Look up a tool by name regardless of filtering.
    fn get_tool_unfiltered(&self, name: &str) -> Option<Arc<dyn Tool>>;

    /// Get all tool specs for LLM registration.
    fn tool_specs(&self) -> Vec<ToolSpec>;

    /// Get tool entry metadata (source, etc.).
    fn get_entry(&self, name: &str) -> Option<ToolEntry>;

    /// List all registered tool names.
    fn tool_names(&self) -> Vec<String>;

    /// List all registered tool names regardless of filtering.
    fn all_tool_names(&self) -> Vec<String>;

    /// Check if a tool name passes the current filter rules.
    fn is_tool_enabled(&self, name: &str) -> bool;

    /// Register a tool, replacing any existing tool with the same name.
    /// Returns the previous entry if one was replaced.
    fn register_or_replace(&self, tool: Box<dyn Tool>, source: ToolSource) -> Option<ToolEntry>;

    /// Remove all tools matching a given source. Returns the number of tools removed.
    fn unregister_by_source(&self, source: &ToolSource) -> usize;

    /// Number of registered tools.
    fn len(&self) -> usize;

    /// Whether the registry is empty.
    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Downcast support for runtime type inspection.
    fn as_any(&self) -> &dyn std::any::Any;
}
