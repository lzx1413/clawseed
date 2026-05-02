//! Minimal agent_loop shim — provides the symbols the gateway depends on.
//!
//! The original 4300-line monolithic agent loop was replaced by the registry-based
//! `Agent` in `agent.rs`. This module provides backwards-compatible shims for
//! gateway integration.

use std::future::Future;

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

/// Scope a session key around an async operation.
///
/// In the registry-based agent, session scoping is handled by the
/// `memory_session_id` field on the Agent struct. This shim exists
/// solely for gateway compatibility — it simply runs the future as-is.
pub async fn scope_session_key<F, T>(_key: Option<String>, fut: F) -> T
where
    F: Future<Output = T>,
{
    fut.await
}
