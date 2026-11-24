//! ToolContext — the execution context for tool runs.
//!
//! Provides the workspace directory and other environment info
//! that tools may need during execution.

use std::path::Path;

use crate::user_profile::UserContext;

/// Core trait for tool execution context.
pub trait ToolContext: Send + Sync {
    /// The workspace directory for file operations.
    fn workspace_dir(&self) -> &Path;

    /// Authenticated user/session identity supplied by the transport.
    fn user_context(&self) -> Option<&UserContext> {
        None
    }

    /// Identifier of the agent turn currently executing this tool.
    fn turn_id(&self) -> Option<&str> {
        None
    }

    /// Provider-assigned identifier of the current tool call.
    fn tool_call_id(&self) -> Option<&str> {
        None
    }

    /// Images attached to the current user turn with request-local source paths.
    /// Extensions may use these paths only for the current tool invocation.
    fn image_attachments(&self) -> &[crate::provider::ImageAttachment] {
        &[]
    }
}
