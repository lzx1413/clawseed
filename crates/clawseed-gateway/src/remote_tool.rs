//! Remote tool implementation for Android integration PoC.
//!
//! Wraps a remote tool registered via WebSocket as a clawseed [`Tool`],
//! delegating execution to the WebSocket client (e.g., Android Kotlin UI).
//!
//! Pattern follows `node_tool.rs`: uses oneshot channel for async request/response.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use tokio::sync::{mpsc, oneshot};
use clawseed_api::tool::{Tool, ToolResult};

/// Default timeout for remote tool invocations (30 seconds).
const REMOTE_TOOL_TIMEOUT_SECS: u64 = 30;

/// Specification for a remote tool registered by WebSocket client.
/// Re-exported from ws.rs for convenience.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct RemoteToolSpec {
    /// Tool name (must match agent's tool invocation)
    pub name: String,
    /// Human-readable description for the LLM
    pub description: String,
    /// JSON schema for parameters
    pub parameters: serde_json::Value,
}

/// Request from RemoteTool to ws.rs to invoke a remote tool on the WebSocket client.
pub struct RemoteToolRequest {
    /// Unique call ID for correlation
    pub call_id: String,
    /// Tool name to invoke
    pub tool_name: String,
    /// Arguments for the tool
    pub args: serde_json::Value,
    /// Channel to send the result back to the tool execution
    pub response_tx: oneshot::Sender<RemoteToolResult>,
}

/// Result returned from WebSocket client after executing a remote tool.
#[derive(Debug)]
pub struct RemoteToolResult {
    /// Whether execution succeeded
    pub success: bool,
    /// Output from the tool
    pub output: String,
    /// Optional error message
    pub error: Option<String>,
}

/// Handle shared between ws.rs and RemoteTool instances.
///
/// Contains the channel for sending tool invocation requests to the WebSocket
/// handler, and the registered tool specs for building Tool instances.
#[derive(Clone)]
pub struct RemoteToolRegistryHandle {
    /// Channel to send tool invocation requests to ws.rs
    pub request_tx: mpsc::Sender<RemoteToolRequest>,
    /// Registered tool specs, updated when client sends register_tools
    pub specs: HashMap<String, RemoteToolSpec>,
}

impl RemoteToolRegistryHandle {
    /// Create a new handle with the given request channel.
    pub fn new(request_tx: mpsc::Sender<RemoteToolRequest>) -> Self {
        Self {
            request_tx,
            specs: HashMap::new(),
        }
    }

    /// Register a tool spec.
    pub fn register(&mut self, spec: RemoteToolSpec) {
        self.specs.insert(spec.name.clone(), spec);
    }

    /// Get the number of registered tools.
    pub fn len(&self) -> usize {
        self.specs.len()
    }

    /// Check if no tools are registered.
    pub fn is_empty(&self) -> bool {
        self.specs.is_empty()
    }

    /// Build RemoteTool instances from registered specs.
    pub fn build_tools(self: &Arc<Self>) -> Vec<Box<dyn Tool>> {
        self.specs
            .values()
            .map(|spec| {
                Box::new(RemoteTool::new(
                    spec.name.clone(),
                    spec.description.clone(),
                    spec.parameters.clone(),
                    Arc::clone(self),
                )) as Box<dyn Tool>
            })
            .collect()
    }
}

/// A clawseed [`Tool`] backed by a WebSocket client.
///
/// When executed, sends a request to the WebSocket handler which forwards
/// `tool_call_request` to the client. Waits for `tool_result` response.
pub struct RemoteTool {
    /// Tool name
    name: String,
    /// Human-readable description
    description: String,
    /// JSON schema for parameters
    parameters: serde_json::Value,
    /// Handle for sending requests to ws.rs
    handle: Arc<RemoteToolRegistryHandle>,
}

impl RemoteTool {
    /// Create a new remote tool wrapper.
    pub fn new(
        name: String,
        description: String,
        parameters: serde_json::Value,
        handle: Arc<RemoteToolRegistryHandle>,
    ) -> Self {
        Self {
            name,
            description,
            parameters,
            handle,
        }
    }
}

#[async_trait]
impl Tool for RemoteTool {
    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn parameters_schema(&self) -> serde_json::Value {
        self.parameters.clone()
    }

    async fn execute(&self, args: serde_json::Value, _ctx: &dyn clawseed_api::tool_context::ToolContext) -> anyhow::Result<ToolResult> {
        let call_id = uuid::Uuid::new_v4().to_string();
        let (response_tx, response_rx) = oneshot::channel();

        let request = RemoteToolRequest {
            call_id,
            tool_name: self.name.clone(),
            args,
            response_tx,
        };

        // Send request to ws.rs handler
        if self.handle.request_tx.send(request).await.is_err() {
            return Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some("WebSocket handler not available".into()),
            });
        }

        // Wait for response with timeout
        match tokio::time::timeout(
            Duration::from_secs(REMOTE_TOOL_TIMEOUT_SECS),
            response_rx,
        )
            .await
        {
            Ok(Ok(result)) => Ok(ToolResult {
                success: result.success,
                output: result.output,
                error: result.error,
            }),
            Ok(Err(_)) => Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some("Response channel closed".into()),
            }),
            Err(_) => Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some(format!(
                    "Timeout waiting for remote tool response after {REMOTE_TOOL_TIMEOUT_SECS}s"
                )),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clawseed_api::tool_context::ToolContext;
    use std::path::PathBuf;

    /// Minimal ToolContext stub for tests.
    struct StubToolContext;

    impl ToolContext for StubToolContext {
        fn workspace_dir(&self) -> &std::path::Path {
            static EMPTY: std::path::PathBuf = std::path::PathBuf::new();
            &EMPTY
        }

        fn get_any(&self, _type_id: std::any::TypeId) -> Option<&(dyn std::any::Any + Send + Sync)> {
            None
        }
    }

    #[test]
    fn remote_tool_spec_deserialization() {
        let json = serde_json::json!({
            "name": "local_contacts",
            "description": "Query phone contacts",
            "parameters": {
                "type": "object",
                "properties": {
                    "query": {"type": "string", "description": "Search keyword"}
                }
            }
        });
        let spec: RemoteToolSpec = serde_json::from_value(json).unwrap();
        assert_eq!(spec.name, "local_contacts");
        assert_eq!(spec.description, "Query phone contacts");
    }

    #[test]
    fn remote_tool_registry_handle_register() {
        let (tx, _rx) = mpsc::channel(32);
        let mut handle = RemoteToolRegistryHandle::new(tx);
        assert!(handle.is_empty());

        handle.register(RemoteToolSpec {
            name: "test_tool".into(),
            description: "Test".into(),
            parameters: serde_json::json!({"type": "object"}),
        });
        assert_eq!(handle.len(), 1);
    }

    #[tokio::test]
    async fn remote_tool_execute_timeout() {
        let (tx, mut rx) = mpsc::channel(32);
        let handle = Arc::new(RemoteToolRegistryHandle::new(tx));

        let tool = RemoteTool::new(
            "test".into(),
            "Test".into(),
            serde_json::json!({"type": "object"}),
            Arc::clone(&handle),
        );

        // Spawn task that receives the request but drops the response channel
        // (simulating client disconnect without responding)
        tokio::spawn(async move {
            // Receive the request but intentionally drop response_tx without sending
            let _ = rx.recv().await; // Receives request, response_tx gets dropped here
        });

        // The tool.execute() sends request, then waits on response_rx.
        // When the spawned task drops response_tx (by exiting the closure),
        // the oneshot channel closes and execute() receives an Err.
        let result = tool.execute(serde_json::json!({}), &StubToolContext).await.unwrap();

        // When the spawned task drops response_tx, execute() gets Err from recv
        // and returns a ToolResult with error "Response channel closed"
        assert!(!result.success);
        assert!(result.error.unwrap().contains("Response channel closed"));
    }

    #[tokio::test]
    async fn remote_tool_execute_success() {
        let (tx, mut rx) = mpsc::channel(32);
        let handle = Arc::new(RemoteToolRegistryHandle::new(tx));

        let tool = RemoteTool::new(
            "test".into(),
            "Test".into(),
            serde_json::json!({"type": "object"}),
            Arc::clone(&handle),
        );

        // Spawn task that receives request and sends response
        tokio::spawn(async move {
            if let Some(req) = rx.recv().await {
                let _ = req.response_tx.send(RemoteToolResult {
                    success: true,
                    output: "success output".into(),
                    error: None,
                });
            }
        });

        let result = tool.execute(serde_json::json!({"arg": "value"}), &StubToolContext).await.unwrap();
        assert!(result.success);
        assert_eq!(result.output, "success output");
        assert!(result.error.is_none());
    }

    #[tokio::test]
    async fn remote_tool_execute_error_response() {
        let (tx, mut rx) = mpsc::channel(32);
        let handle = Arc::new(RemoteToolRegistryHandle::new(tx));

        let tool = RemoteTool::new(
            "test".into(),
            "Test".into(),
            serde_json::json!({"type": "object"}),
            Arc::clone(&handle),
        );

        // Spawn task that receives request and sends error
        tokio::spawn(async move {
            if let Some(req) = rx.recv().await {
                let _ = req.response_tx.send(RemoteToolResult {
                    success: false,
                    output: String::new(),
                    error: Some("Permission denied".into()),
                });
            }
        });

        let result = tool.execute(serde_json::json!({}), &StubToolContext).await.unwrap();
        assert!(!result.success);
        assert_eq!(result.error.unwrap(), "Permission denied");
    }

    #[tokio::test]
    async fn remote_tool_execute_channel_closed() {
        let (tx, rx) = mpsc::channel(32);
        let handle = Arc::new(RemoteToolRegistryHandle::new(tx));

        let tool = RemoteTool::new(
            "test".into(),
            "Test".into(),
            serde_json::json!({"type": "object"}),
            Arc::clone(&handle),
        );

        // Drop receiver to close the channel
        drop(rx);

        let result = tool.execute(serde_json::json!({}), &StubToolContext).await.unwrap();
        assert!(!result.success);
        assert!(result.error.unwrap().contains("not available"));
    }

    #[test]
    fn build_tools_from_handle() {
        let (tx, _rx) = mpsc::channel(32);
        let mut handle = RemoteToolRegistryHandle::new(tx);
        handle.register(RemoteToolSpec {
            name: "tool1".into(),
            description: "First tool".into(),
            parameters: serde_json::json!({"type": "object"}),
        });
        handle.register(RemoteToolSpec {
            name: "tool2".into(),
            description: "Second tool".into(),
            parameters: serde_json::json!({"type": "object"}),
        });

        let arc_handle = Arc::new(handle);
        let tools = arc_handle.build_tools();
        assert_eq!(tools.len(), 2);

        let names: Vec<&str> = tools.iter().map(|t| t.name()).collect();
        assert!(names.contains(&"tool1"));
        assert!(names.contains(&"tool2"));
    }
}