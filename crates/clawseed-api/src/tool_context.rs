//! ToolContext — the execution context for tool runs.
//!
//! Provides the workspace directory and other environment info
//! that tools may need during execution.

use std::path::Path;

/// Core trait for tool execution context.
pub trait ToolContext: Send + Sync {
    /// The workspace directory for file operations.
    fn workspace_dir(&self) -> &Path;
}
