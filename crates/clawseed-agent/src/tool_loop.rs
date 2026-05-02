//! Tool loop execution for the agent.
//!
//! Manages the iterative cycle of: provider response -> tool calls ->
//! tool results -> next provider response, until the provider returns
//! a final text response or the loop is cancelled.

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
