//! End-to-end integration tests for the Android remote tool bridge.
//!
//! These tests validate the complete chain without a real LLM or Android device:
//!   real Agent (MockProvider) → RemoteTool dispatch → simulated client → result → final response

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use clawseed_api::provider::{
    ChatRequest, ChatResponse, Provider,
    ToolCall,
};
use clawseed_api::memory_traits::Memory;
use clawseed_agent::agent::{Agent, TurnEvent};
use clawseed_agent::dispatcher::NativeToolDispatcher;
use clawseed_agent::observer::NoopObserver;
use clawseed_gateway::remote_tool::{
    RemoteToolRegistryHandle, RemoteToolRequest, RemoteToolResult, RemoteToolSpec,
};
use parking_lot::Mutex;
use tokio::sync::mpsc;

// ── Shared mock provider ──────────────────────────────────────────────────────

struct MockProvider {
    responses: Mutex<Vec<ChatResponse>>,
}

impl MockProvider {
    fn new(responses: Vec<ChatResponse>) -> Self {
        Self {
            responses: Mutex::new(responses),
        }
    }
}

#[async_trait]
impl Provider for MockProvider {
    async fn chat_with_system(
        &self,
        _system_prompt: Option<&str>,
        _message: &str,
        _model: &str,
        _temperature: Option<f64>,
    ) -> anyhow::Result<String> {
        let mut guard = self.responses.lock();
        if guard.is_empty() {
            return Ok("fallback".into());
        }
        let resp = guard.remove(0);
        Ok(resp.text.unwrap_or_else(|| "fallback".into()))
    }

    async fn chat(
        &self,
        _request: ChatRequest<'_>,
        _model: &str,
        _temperature: Option<f64>,
    ) -> anyhow::Result<ChatResponse> {
        let mut guard = self.responses.lock();
        if guard.is_empty() {
            return Ok(ChatResponse {
                text: Some("done".into()),
                tool_calls: vec![],
                usage: None,
                reasoning_content: None,
            });
        }
        Ok(guard.remove(0))
    }
}

fn text_response(text: &str) -> ChatResponse {
    ChatResponse {
        text: Some(text.into()),
        tool_calls: vec![],
        usage: None,
        reasoning_content: None,
    }
}

fn tool_response(calls: Vec<ToolCall>) -> ChatResponse {
    ChatResponse {
        text: Some(String::new()),
        tool_calls: calls,
        usage: None,
        reasoning_content: None,
    }
}

// ── Shared setup ──────────────────────────────────────────────────────────────

fn contacts_spec() -> RemoteToolSpec {
    RemoteToolSpec {
        name: "local_contacts".to_string(),
        description: "查询手机联系人".to_string(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "query": {"type": "string", "description": "搜索关键词"}
            },
            "required": ["query"]
        }),
    }
}

fn make_memory() -> Arc<dyn Memory> {
    Arc::new(clawseed_memory::none::NoneMemory::new())
}

fn build_agent_with_remote_tool(
    provider: Box<MockProvider>,
    request_tx: mpsc::Sender<RemoteToolRequest>,
) -> Agent {
    let mut registry = RemoteToolRegistryHandle::new(request_tx);
    registry.register(contacts_spec());
    let observer: Arc<dyn clawseed_agent::observer::Observer> = Arc::new(NoopObserver);
    let mut agent = Agent::builder()
        .provider(provider)
        .tools(vec![])
        .memory(make_memory())
        .observer(observer as Arc<dyn clawseed_agent::observer::Observer>)
        .tool_dispatcher(Box::new(NativeToolDispatcher))
        .workspace_dir(std::env::temp_dir())
        .build()
        .unwrap();
    agent.add_remote_tools(Arc::new(registry).build_tools());
    agent
}

// ── Tests ─────────────────────────────────────────────────────────────────────

/// Happy path: agent calls remote tool, simulated Android client responds,
/// agent generates a final response that includes the tool output.
#[tokio::test]
async fn remote_tool_happy_path() {
    let (request_tx, mut request_rx) = mpsc::channel::<RemoteToolRequest>(32);

    let provider = Box::new(MockProvider::new(vec![
        tool_response(vec![ToolCall {
            id: "tc1".into(),
            name: "local_contacts".into(),
            arguments: r#"{"query": "张三"}"#.into(),
        }]),
        text_response("张三的电话是 13800138000"),
    ]));

    let mut agent = build_agent_with_remote_tool(provider, request_tx);

    // Agent turn runs in a spawned task; main task acts as the Android client.
    let agent_task = tokio::spawn(async move { agent.turn("帮我查一下张三的电话").await });

    // Simulate Android client: receive the tool_call_request and respond.
    let request = request_rx.recv().await.expect("expected tool request");
    assert_eq!(request.tool_name, "local_contacts");
    assert_eq!(request.args["query"].as_str().unwrap(), "张三");

    request
        .response_tx
        .send(RemoteToolResult {
            success: true,
            output: r#"[{"name":"张三","phone":"13800138000"}]"#.into(),
            error: None,
        })
        .unwrap();

    let response = agent_task.await.unwrap().unwrap();
    assert!(
        response.contains("13800138000"),
        "response should include the phone number, got: {response}"
    );
}

/// Error path: client returns a failure (permission denied).
/// The agent should still complete its turn and produce a non-empty response
/// acknowledging the failure — it must not hang.
#[tokio::test]
async fn remote_tool_error_handled_gracefully() {
    let (request_tx, mut request_rx) = mpsc::channel::<RemoteToolRequest>(32);

    let provider = Box::new(MockProvider::new(vec![
        tool_response(vec![ToolCall {
            id: "tc1".into(),
            name: "local_contacts".into(),
            arguments: r#"{"query": "张三"}"#.into(),
        }]),
        // LLM sees the error output and generates an apologetic response.
        text_response("抱歉，无法访问联系人：权限不足"),
    ]));

    let mut agent = build_agent_with_remote_tool(provider, request_tx);
    let agent_task = tokio::spawn(async move { agent.turn("帮我查一下张三的电话").await });

    let request = request_rx.recv().await.expect("expected tool request");

    // Client reports permission denied.
    request
        .response_tx
        .send(RemoteToolResult {
            success: false,
            output: String::new(),
            error: Some("Permission denied: READ_CONTACTS required".into()),
        })
        .unwrap();

    let response = agent_task.await.unwrap().unwrap();
    assert!(
        !response.is_empty(),
        "agent should produce a response even after tool error"
    );
}

/// Event stream: using turn_streamed, verify that ToolCall and ToolResult
/// TurnEvents are emitted for remote tool calls (Android client gets visibility).
#[tokio::test]
async fn remote_tool_turn_events_include_tool_call_and_result() {
    let (request_tx, mut request_rx) = mpsc::channel::<RemoteToolRequest>(32);
    let (event_tx, mut event_rx) = mpsc::channel::<TurnEvent>(64);

    let provider = Box::new(MockProvider::new(vec![
        tool_response(vec![ToolCall {
            id: "tc1".into(),
            name: "local_contacts".into(),
            arguments: r#"{"query": "张三"}"#.into(),
        }]),
        text_response("张三的电话是 13800138000"),
    ]));

    let mut agent = build_agent_with_remote_tool(provider, request_tx);

    // Collect events in a background task.
    let events_task = tokio::spawn(async move {
        let mut events = Vec::new();
        while let Some(event) = event_rx.recv().await {
            events.push(event);
        }
        events
    });

    // Drive the agent turn and the simulated client concurrently.
    let turn_task = tokio::spawn(async move {
        agent
            .turn_streamed("帮我查一下张三的电话", event_tx, None)
            .await
    });

    // Simulated client.
    let request = request_rx.recv().await.expect("expected tool request");
    request
        .response_tx
        .send(RemoteToolResult {
            success: true,
            output: r#"[{"name":"张三","phone":"13800138000"}]"#.into(),
            error: None,
        })
        .unwrap();

    turn_task.await.unwrap().unwrap();

    // event_tx was dropped when turn_task completed; events_task will now drain.
    let events = events_task.await.unwrap();

    let has_tool_call = events.iter().any(|e| {
        matches!(e, TurnEvent::ToolCall { name, .. } if name == "local_contacts")
    });
    let has_tool_result = events.iter().any(|e| {
        matches!(e, TurnEvent::ToolResult { name, .. } if name == "local_contacts")
    });

    assert!(has_tool_call, "expected ToolCall event for local_contacts");
    assert!(has_tool_result, "expected ToolResult event for local_contacts");
}

/// Timeout regression guard: the entire turn (including remote tool round-trip)
/// must complete well under the 30-second RemoteTool timeout. If the old deadlock
/// were to reappear, this test would time out at 5 seconds instead of silently
/// hanging for 30.
#[tokio::test]
async fn remote_tool_completes_well_under_timeout() {
    let (request_tx, mut request_rx) = mpsc::channel::<RemoteToolRequest>(32);

    let provider = Box::new(MockProvider::new(vec![
        tool_response(vec![ToolCall {
            id: "tc1".into(),
            name: "local_contacts".into(),
            arguments: r#"{"query": "张三"}"#.into(),
        }]),
        text_response("张三的电话是 13800138000"),
    ]));

    let mut agent = build_agent_with_remote_tool(provider, request_tx);

    tokio::time::timeout(Duration::from_secs(5), async move {
        let agent_task = tokio::spawn(async move { agent.turn("帮我查一下张三的电话").await });

        let request = request_rx.recv().await.expect("expected tool request");
        request
            .response_tx
            .send(RemoteToolResult {
                success: true,
                output: r#"[{"name":"张三","phone":"13800138000"}]"#.into(),
                error: None,
            })
            .unwrap();

        agent_task.await.unwrap().unwrap()
    })
    .await
    .expect("turn timed out after 5s — possible deadlock regression");
}
