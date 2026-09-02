//! Backwards-compatible tool-loop cancellation symbols.
//!
//! Active turn and tool execution live in `agent::turn` and
//! `agent::tool_execution`. This module remains for callers that still use the
//! legacy cancellation helper path.

/// Error type for cancelled tool loops.
#[derive(Debug)]
pub struct ToolLoopCancelled;

impl std::fmt::Display for ToolLoopCancelled {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("tool loop cancelled")
    }
}

impl std::error::Error for ToolLoopCancelled {}

/// Check if an error was caused by tool-loop cancellation.
pub fn is_tool_loop_cancelled(err: &anyhow::Error) -> bool {
    err.chain().any(|source| source.is::<ToolLoopCancelled>())
}
