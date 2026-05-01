//! Agent loop utilities needed by the gateway.
//!
//! Re-exports and stubs for functions referenced by the gateway.

use clawseed_api::TOOL_LOOP_SESSION_KEY;

/// Set the session key for the duration of the given future.
pub async fn scope_session_key<F>(key: Option<String>, fut: F) -> F::Output
where
    F: std::future::Future,
{
    match key {
        Some(k) => TOOL_LOOP_SESSION_KEY.scope(Some(k), fut).await,
        None => fut.await,
    }
}

/// Check if an error represents a cancelled tool loop.
pub fn is_tool_loop_cancelled(err: &anyhow::Error) -> bool {
    err.to_string().contains("ToolLoopCancelled")
        || err.to_string().contains("cancelled")
        || err.chain().any(|e| {
            e.to_string().contains("ToolLoopCancelled")
                || e.to_string().contains("cancelled")
        })
}

/// Process a message through the agent loop (stub).
pub async fn process_message(
    _config: clawseed_config::schema::Config,
    _message: &str,
    _session_id: Option<&str>,
) -> anyhow::Result<String> {
    Ok(String::new())
}
