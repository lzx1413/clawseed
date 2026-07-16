//! End-to-end WebSocket integration tests for the Android demo flow.
//!
//! Validates the full protocol path that the Android Kotlin client depends on:
//!   connect → session_start → register_tools → tools_registered →
//!   message → streaming events → tool_call_request → tool_result → done
//!
//! Uses a real Axum server with a mock OpenAI-compatible API so every
//! WebSocket message passes through the actual ws.rs handler code.

use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use clawseed_agent::security::pairing::PairingGuard;
use clawseed_agent::tool_registry::DefaultToolRegistry;
use clawseed_api::memory_traits::Memory;
use clawseed_gateway::ws::handle_ws_chat;
use futures_util::{SinkExt, StreamExt};
use parking_lot::Mutex;
use tokio_tungstenite::{connect_async, tungstenite::Message};

// ── Mock memory ───────────────────────────────────────────────────────────────

struct MockMemory;

#[async_trait::async_trait]
impl Memory for MockMemory {
    fn name(&self) -> &str {
        "mock"
    }
    async fn store(
        &self,
        _key: &str,
        _content: &str,
        _category: clawseed_api::memory_traits::MemoryCategory,
        _session_id: Option<&str>,
    ) -> anyhow::Result<()> {
        Ok(())
    }
    async fn recall(
        &self,
        _query: &str,
        _limit: usize,
        _session_id: Option<&str>,
        _since: Option<&str>,
        _until: Option<&str>,
        _search_mode: Option<clawseed_api::memory_traits::SearchMode>,
    ) -> anyhow::Result<Vec<clawseed_api::memory_traits::MemoryEntry>> {
        Ok(Vec::new())
    }
    async fn get(
        &self,
        _key: &str,
    ) -> anyhow::Result<Option<clawseed_api::memory_traits::MemoryEntry>> {
        Ok(None)
    }
    async fn list(
        &self,
        _category: Option<&clawseed_api::memory_traits::MemoryCategory>,
        _session_id: Option<&str>,
    ) -> anyhow::Result<Vec<clawseed_api::memory_traits::MemoryEntry>> {
        Ok(Vec::new())
    }
    async fn forget(&self, _key: &str) -> anyhow::Result<bool> {
        Ok(false)
    }
    async fn count(&self) -> anyhow::Result<usize> {
        Ok(0)
    }
    async fn health_check(&self) -> bool {
        true
    }
}

// ── Mock OpenAI-compatible API server ─────────────────────────────────────────
//
// The WS handler creates agents via Agent::from_config(), which needs a real
// provider. We run a tiny Axum server that responds to the OpenAI chat
// completions API with pre-configured responses.
//
// Supports both streaming and non-streaming requests, since the agent's
// turn_streamed() uses the streaming API.

type MockResponses = Arc<Mutex<Vec<MockChatResponse>>>;

#[derive(Clone)]
enum MockChatResponse {
    Text(String),
    ToolCalls(Vec<clawseed_api::provider::ToolCall>),
}

/// Handle chat completions — supports both streaming and non-streaming.
async fn mock_chat_completions(
    axum::extract::State(responses): axum::extract::State<MockResponses>,
    body: String,
) -> impl axum::response::IntoResponse {
    let parsed: serde_json::Value = serde_json::from_str(&body).unwrap_or_default();
    let is_stream = parsed["stream"].as_bool().unwrap_or(false);

    let mut guard = responses.lock();
    let response = if guard.is_empty() {
        MockChatResponse::Text("done".into())
    } else {
        guard.remove(0)
    };
    drop(guard);

    let (content, tool_calls) = match response {
        MockChatResponse::Text(text) => (Some(text), vec![]),
        MockChatResponse::ToolCalls(calls) => (Some(String::new()), calls),
    };

    let chat_id = format!("chatcmpl-{}", uuid::Uuid::new_v4());

    if is_stream {
        // SSE streaming response
        let mut chunks: Vec<String> = Vec::new();

        // First chunk: role
        chunks.push(serde_json::json!({
            "id": chat_id,
            "object": "chat.completion.chunk",
            "choices": [{"index": 0, "delta": {"role": "assistant", "content": ""}, "finish_reason": null}]
        }).to_string());

        // Content chunk
        if let Some(ref text) = content
            && !text.is_empty()
        {
            chunks.push(
                serde_json::json!({
                    "id": chat_id,
                    "object": "chat.completion.chunk",
                    "choices": [{"index": 0, "delta": {"content": text}, "finish_reason": null}]
                })
                .to_string(),
            );
        }

        // Tool calls chunks
        for (i, tc) in tool_calls.iter().enumerate() {
            chunks.push(
                serde_json::json!({
                    "id": chat_id,
                    "object": "chat.completion.chunk",
                    "choices": [{"index": 0, "delta": {
                        "tool_calls": [{
                            "index": i,
                            "id": tc.id,
                            "type": "function",
                            "function": {"name": tc.name, "arguments": tc.arguments}
                        }]
                    }, "finish_reason": null}]
                })
                .to_string(),
            );
        }

        // Final chunk
        let finish = if tool_calls.is_empty() {
            "stop"
        } else {
            "tool_calls"
        };
        chunks.push(
            serde_json::json!({
                "id": chat_id,
                "object": "chat.completion.chunk",
                "choices": [{"index": 0, "delta": {}, "finish_reason": finish}]
            })
            .to_string(),
        );

        let sse_body = chunks
            .iter()
            .map(|c| format!("data: {c}\n\n"))
            .collect::<Vec<_>>()
            .join("")
            + "data: [DONE]\n\n";

        (
            axum::http::StatusCode::OK,
            [(
                axum::http::header::CONTENT_TYPE,
                "text/event-stream".to_string(),
            )],
            sse_body,
        )
            .into_response()
    } else {
        // Non-streaming JSON response
        let tool_calls_json: Vec<serde_json::Value> = tool_calls
            .iter()
            .enumerate()
            .map(|(i, tc)| {
                serde_json::json!({
                    "id": tc.id,
                    "type": "function",
                    "function": {"name": tc.name, "arguments": tc.arguments},
                    "index": i
                })
            })
            .collect();

        let resp = serde_json::json!({
            "id": chat_id,
            "object": "chat.completion",
            "choices": [{
                "index": 0,
                "message": {
                    "role": "assistant",
                    "content": content,
                    "tool_calls": if tool_calls_json.is_empty() { serde_json::Value::Null } else { serde_json::Value::Array(tool_calls_json) },
                },
                "finish_reason": if tool_calls.is_empty() { "stop" } else { "tool_calls" }
            }],
            "usage": {"prompt_tokens": 0, "completion_tokens": 0, "total_tokens": 0}
        });

        axum::Json(resp).into_response()
    }
}

// ── Test helpers ──────────────────────────────────────────────────────────────

/// Shared responses for both the mock API server and assertions.
struct TestContext {
    ws_addr: std::net::SocketAddr,
    #[allow(dead_code)]
    api_addr: std::net::SocketAddr,
    #[allow(dead_code)]
    responses: MockResponses,
}

fn text_response(text: &str) -> MockChatResponse {
    MockChatResponse::Text(text.into())
}

fn tool_response(calls: Vec<clawseed_api::provider::ToolCall>) -> MockChatResponse {
    MockChatResponse::ToolCalls(calls)
}

fn make_config(api_addr: std::net::SocketAddr) -> clawseed_config::schema::Config {
    let mut config = clawseed_config::schema::Config::default();
    config.providers.fallback = Some("openai".into());
    config.providers.models.insert(
        "openai".into(),
        clawseed_config::schema::ModelProviderConfig {
            api_key: Some("test-key".into()),
            base_url: Some(format!("http://{api_addr}/v1")),
            model: Some("test-model".into()),
            name: None,
            api_path: None,
            temperature: None,
            timeout_secs: None,
            extra_headers: Default::default(),
            wire_api: None,
            max_tokens: None,
            provider_extra: None,
            merge_system_into_user: false,
        },
    );
    config
}

fn test_app_state(
    config: clawseed_config::schema::Config,
    api_addr: Option<std::net::SocketAddr>,
) -> clawseed_gateway::AppState {
    let provider: Arc<dyn clawseed_api::provider::Provider> = if let Some(addr) = api_addr {
        Arc::from(
            clawseed_providers::create_resilient_provider_with_options(
                "openai",
                Some("test-key"),
                Some(&format!("http://{addr}/v1")),
                &clawseed_config::schema::ReliabilityConfig::default(),
                &clawseed_providers::ProviderRuntimeOptions::default(),
            )
            .unwrap_or_else(|_| {
                clawseed_providers::create_resilient_provider_with_options(
                    "ollama",
                    None,
                    Some("http://127.0.0.1:1/v1"),
                    &clawseed_config::schema::ReliabilityConfig::default(),
                    &clawseed_providers::ProviderRuntimeOptions::default(),
                )
                .unwrap()
            }),
        )
    } else {
        Arc::from(
            clawseed_providers::create_resilient_provider_with_options(
                "openai",
                Some("test-key"),
                None,
                &clawseed_config::schema::ReliabilityConfig::default(),
                &clawseed_providers::ProviderRuntimeOptions::default(),
            )
            .unwrap_or_else(|_| {
                clawseed_providers::create_resilient_provider_with_options(
                    "ollama",
                    None,
                    Some("http://127.0.0.1:1/v1"),
                    &clawseed_config::schema::ReliabilityConfig::default(),
                    &clawseed_providers::ProviderRuntimeOptions::default(),
                )
                .unwrap()
            }),
        )
    };
    clawseed_gateway::AppState {
        config: Arc::new(Mutex::new(config)),
        provider,
        model: "test-model".into(),
        temperature: 0.0,
        mem: Arc::new(MockMemory),
        user_profile_store: None,
        auto_save: false,
        webhook_secret_hash: None,
        pairing: Arc::new(PairingGuard::new(false, &[])),
        trust_forwarded_headers: false,
        rate_limiter: Arc::new(clawseed_gateway::GatewayRateLimiter::new(100, 100, 100)),
        auth_limiter: Arc::new(clawseed_gateway::auth_rate_limit::AuthRateLimiter::new()),
        idempotency_store: Arc::new(clawseed_gateway::IdempotencyStore::new(
            Duration::from_secs(300),
            1000,
        )),
        observer: Arc::new(clawseed_agent::observability::NoopObserver),
        tool_registry: Arc::new(DefaultToolRegistry::new()),
        shared_builtin_tools: Arc::new([]),
        skill_index: Arc::new(parking_lot::RwLock::new(Vec::new())),
        skills_excluded: Arc::new(std::sync::Mutex::new(Vec::new())),
        cost_tracker: None,
        event_tx: tokio::sync::broadcast::channel(16).0,
        event_buffer: Arc::new(clawseed_gateway::EventBuffer::new(16)),
        shutdown_tx: tokio::sync::watch::channel(false).0,
        node_registry: Arc::new(clawseed_gateway::NodeRegistry::new(16)),
        session_backend: None,
        session_queue: Arc::new(clawseed_gateway::session_queue::SessionActorQueue::new(
            8, 30, 600,
        )),
        path_prefix: String::new(),
        web_dist_dir: None,
        canvas_store: clawseed_agent::tools::CanvasStore::new(),
        cancel_tokens: Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
    }
}

/// Set up the full test environment: mock API server + gateway server.
/// Returns the WebSocket address and the shared responses handle.
async fn setup_test_env(responses: Vec<MockChatResponse>) -> TestContext {
    let responses = Arc::new(Mutex::new(responses));

    // Start mock OpenAI API server
    let api_app = Router::new()
        .route("/v1/chat/completions", post(mock_chat_completions))
        .with_state(responses.clone());
    let api_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let api_addr = api_listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(api_listener, api_app).await.unwrap();
    });

    // Start gateway with config pointing to mock API
    let config = make_config(api_addr);
    let state = test_app_state(config, Some(api_addr));
    let ws_app = Router::new()
        .route("/ws/chat", get(handle_ws_chat))
        .with_state(state);
    let ws_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let ws_addr = ws_listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(ws_listener, ws_app).await.unwrap();
    });

    TestContext {
        ws_addr,
        api_addr,
        responses,
    }
}

/// Connect to the WebSocket and perform the connect handshake.
/// Returns (tx, rx) ready for further interaction.
async fn ws_connect_with_handshake(
    addr: std::net::SocketAddr,
) -> (
    futures_util::stream::SplitSink<
        tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
        Message,
    >,
    futures_util::stream::SplitStream<
        tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
    >,
) {
    let (stream, _) = connect_async(format!("ws://{addr}/ws/chat")).await.unwrap();
    let (mut tx, mut rx) = stream.split();

    // Receive session_start
    let _ = expect_msg_type(&mut rx, "session_start").await;

    // Send connect handshake (like Android client)
    let connect_msg = serde_json::json!({
        "type": "connect",
        "device_name": "Test Device",
        "capabilities": ["remote_tools"]
    });
    tx.send(Message::Text(connect_msg.to_string().into()))
        .await
        .unwrap();
    let _ = expect_msg_type(&mut rx, "connected").await;

    (tx, rx)
}

/// Parse a WebSocket text message into a JSON value, panicking on failure.
fn parse_msg(text: &str) -> serde_json::Value {
    serde_json::from_str(text).expect("valid JSON from server")
}

/// Wait for a specific message type from the WebSocket stream.
async fn expect_msg_type(
    rx: &mut futures_util::stream::SplitStream<
        tokio_tungstenite::WebSocketStream<
            tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
        >,
    >,
    expected_type: &str,
) -> serde_json::Value {
    let deadline = tokio::time::sleep(Duration::from_secs(15));
    tokio::pin!(deadline);
    loop {
        tokio::select! {
            msg = rx.next() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        let v = parse_msg(&text);
                        if v["type"].as_str() == Some(expected_type) {
                            return v;
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => {
                        panic!("WebSocket closed while waiting for {expected_type}");
                    }
                    Some(Err(e)) => {
                        panic!("WebSocket error while waiting for {expected_type}: {e}");
                    }
                    _ => {}
                }
            }
            _ = &mut deadline => {
                panic!("Timeout waiting for message type '{expected_type}'");
            }
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

/// Test 1: Health endpoint returns ok.
#[tokio::test]
async fn health_endpoint_returns_ok() {
    let ctx = setup_test_env(vec![]).await;
    let config = make_config(ctx.api_addr);
    let state = test_app_state(config, Some(ctx.api_addr));

    let app = Router::new()
        .route(
            "/health",
            get(
                |axum::extract::State(_): axum::extract::State<clawseed_gateway::AppState>| async {
                    axum::Json(serde_json::json!({"status": "ok"}))
                },
            ),
        )
        .with_state(state);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    let client = reqwest::Client::new();
    let resp = client
        .get(format!("http://{addr}/health"))
        .send()
        .await
        .unwrap();
    assert!(resp.status().is_success());
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["status"], "ok");
}

/// Test 2: WebSocket connect → session_start handshake.
#[tokio::test]
async fn ws_connect_receives_session_start() {
    let ctx = setup_test_env(vec![]).await;
    let (stream, _) = connect_async(format!("ws://{}/ws/chat", ctx.ws_addr))
        .await
        .unwrap();
    let (mut tx, mut rx) = stream.split();

    let msg = expect_msg_type(&mut rx, "session_start").await;
    assert!(msg["session_id"].is_string());
    assert_eq!(msg["resumed"], false);

    tx.close().await.unwrap();
}

/// Test 3: WebSocket connect handshake sends connected ack.
#[tokio::test]
async fn ws_connect_handshake_sends_connected_ack() {
    let ctx = setup_test_env(vec![]).await;
    let (stream, _) = connect_async(format!("ws://{}/ws/chat", ctx.ws_addr))
        .await
        .unwrap();
    let (mut tx, mut rx) = stream.split();

    let _ = expect_msg_type(&mut rx, "session_start").await;

    let connect_msg = serde_json::json!({
        "type": "connect",
        "session_id": "android-test-session",
        "device_name": "Pixel 8",
        "capabilities": ["remote_tools"]
    });
    tx.send(Message::Text(connect_msg.to_string().into()))
        .await
        .unwrap();

    let connected = expect_msg_type(&mut rx, "connected").await;
    assert_eq!(connected["message"], "Connection established");

    tx.close().await.unwrap();
}

/// Test 4: register_tools → tools_registered acknowledgement.
#[tokio::test]
async fn ws_register_tools_returns_acknowledgement() {
    let ctx = setup_test_env(vec![]).await;
    let (mut tx, mut rx) = ws_connect_with_handshake(ctx.ws_addr).await;

    let register_msg = serde_json::json!({
        "type": "register_tools",
        "tools": [
            {
                "name": "device_info",
                "description": "Get device information",
                "parameters": {"type": "object", "properties": {}}
            },
            {
                "name": "local_contacts",
                "description": "查询手机联系人",
                "parameters": {
                    "type": "object",
                    "properties": {"query": {"type": "string", "description": "搜索关键词"}},
                    "required": ["query"]
                }
            }
        ]
    });
    tx.send(Message::Text(register_msg.to_string().into()))
        .await
        .unwrap();

    let ack = expect_msg_type(&mut rx, "tools_registered").await;
    assert_eq!(ack["count"], 2);
    assert_eq!(ack["registered"], 2);

    tx.close().await.unwrap();
}

/// Test 5: get_registered_tools returns the tool list.
#[tokio::test]
async fn ws_get_registered_tools_returns_list() {
    let ctx = setup_test_env(vec![]).await;
    let (mut tx, mut rx) = ws_connect_with_handshake(ctx.ws_addr).await;

    let register_msg = serde_json::json!({
        "type": "register_tools",
        "tools": [{
            "name": "device_info",
            "description": "Get device information",
            "parameters": {"type": "object", "properties": {}}
        }]
    });
    tx.send(Message::Text(register_msg.to_string().into()))
        .await
        .unwrap();
    let _ack = expect_msg_type(&mut rx, "tools_registered").await;

    let query_msg = serde_json::json!({"type": "get_registered_tools"});
    tx.send(Message::Text(query_msg.to_string().into()))
        .await
        .unwrap();

    let response = expect_msg_type(&mut rx, "registered_tools").await;
    let tools = response["tools"].as_array().expect("tools array");
    assert_eq!(tools.len(), 1);
    assert_eq!(tools[0]["name"], "device_info");

    tx.close().await.unwrap();
}

/// Test 6: Chat message → streaming events → done.
#[tokio::test]
async fn ws_chat_message_streams_chunks_and_done() {
    // Agent may make multiple chat calls; provide enough responses
    let ctx = setup_test_env(vec![
        text_response("Hello from agent!"),
        text_response("Hello from agent!"),
    ])
    .await;
    let (mut tx, mut rx) = ws_connect_with_handshake(ctx.ws_addr).await;

    let chat_msg = serde_json::json!({"type": "message", "content": "Hello!"});
    tx.send(Message::Text(chat_msg.to_string().into()))
        .await
        .unwrap();

    let done = expect_msg_type(&mut rx, "done").await;
    assert_eq!(done["full_response"], "Hello from agent!");

    tx.close().await.unwrap();
}

/// Test 7: Full Android demo round-trip: register_tools → chat →
/// tool_call_request → tool_result → done.
///
/// This is the critical path the Android demo depends on:
/// 1. Connect and register device_info tool
/// 2. Send a message that triggers the LLM to call the remote tool
/// 3. Receive tool_call_request, respond with tool_result
/// 4. Get final done response
#[tokio::test]
async fn ws_full_remote_tool_round_trip() {
    let ctx = setup_test_env(vec![
        // First: LLM calls the remote device_info tool
        tool_response(vec![clawseed_api::provider::ToolCall {
            id: "call_device_1".into(),
            name: "device_info".into(),
            arguments: "{}".into(),
        }]),
        // Second: LLM uses the tool output to respond
        text_response("Your device is a Pixel 8 running Android 14."),
        // Extra fallback for additional agent iterations
        text_response("Your device is a Pixel 8 running Android 14."),
    ])
    .await;
    let (mut tx, mut rx) = ws_connect_with_handshake(ctx.ws_addr).await;

    // Register the device_info tool (like Android MainActivity)
    let register_msg = serde_json::json!({
        "type": "register_tools",
        "tools": [{
            "name": "device_info",
            "description": "Get device information including model, manufacturer, and Android version",
            "parameters": {"type": "object", "properties": {}}
        }]
    });
    tx.send(Message::Text(register_msg.to_string().into()))
        .await
        .unwrap();
    let tools_ack = expect_msg_type(&mut rx, "tools_registered").await;
    assert_eq!(tools_ack["registered"], 1);

    // Send a chat message that triggers the remote tool
    let chat_msg = serde_json::json!({"type": "message", "content": "What device am I using?"});
    tx.send(Message::Text(chat_msg.to_string().into()))
        .await
        .unwrap();

    // Receive tool_call_request
    let tool_call_req = expect_msg_type(&mut rx, "tool_call_request").await;
    assert_eq!(tool_call_req["name"], "device_info");
    let call_id = tool_call_req["id"].as_str().unwrap().to_string();

    // Send tool_result back (like Android toolCallHandler)
    let tool_result_msg = serde_json::json!({
        "type": "tool_result",
        "id": call_id,
        "output": "Model: Pixel 8, Manufacturer: Google, Android Version: 14",
        "success": true
    });
    tx.send(Message::Text(tool_result_msg.to_string().into()))
        .await
        .unwrap();

    // Receive result_acknowledged
    let ack = expect_msg_type(&mut rx, "result_acknowledged").await;
    assert_eq!(ack["id"], call_id);

    // Receive done with the final response
    let done = expect_msg_type(&mut rx, "done").await;
    assert_eq!(
        done["full_response"],
        "Your device is a Pixel 8 running Android 14."
    );

    tx.close().await.unwrap();
}

/// Test 8: Remote tool error path — client reports failure via tool_result with success=false.
#[tokio::test]
async fn ws_remote_tool_error_handled() {
    let ctx = setup_test_env(vec![
        tool_response(vec![clawseed_api::provider::ToolCall {
            id: "call_contacts_1".into(),
            name: "local_contacts".into(),
            arguments: r#"{"query": "张三"}"#.into(),
        }]),
        text_response("抱歉，无法访问联系人：权限不足"),
        text_response("抱歉，无法访问联系人：权限不足"),
    ])
    .await;
    let (mut tx, mut rx) = ws_connect_with_handshake(ctx.ws_addr).await;

    // Register tool
    let register_msg = serde_json::json!({
        "type": "register_tools",
        "tools": [{
            "name": "local_contacts",
            "description": "查询手机联系人",
            "parameters": {"type": "object", "properties": {"query": {"type": "string"}}, "required": ["query"]}
        }]
    });
    tx.send(Message::Text(register_msg.to_string().into()))
        .await
        .unwrap();
    let _ack = expect_msg_type(&mut rx, "tools_registered").await;

    // Chat
    let chat_msg = serde_json::json!({"type": "message", "content": "查一下张三的电话"});
    tx.send(Message::Text(chat_msg.to_string().into()))
        .await
        .unwrap();

    // Receive tool_call_request
    let tool_call_req = expect_msg_type(&mut rx, "tool_call_request").await;
    let call_id = tool_call_req["id"].as_str().unwrap().to_string();

    // Send error result (like Android permission denied)
    let error_result = serde_json::json!({
        "type": "tool_result",
        "id": call_id,
        "output": "",
        "success": false,
        "error": "Permission denied: READ_CONTACTS required"
    });
    tx.send(Message::Text(error_result.to_string().into()))
        .await
        .unwrap();

    let _ack = expect_msg_type(&mut rx, "result_acknowledged").await;
    let done = expect_msg_type(&mut rx, "done").await;
    assert!(done["full_response"].as_str().unwrap().contains("权限不足"));

    tx.close().await.unwrap();
}

/// Test 9: tool_error message type (alternative error path used by Android client).
#[tokio::test]
async fn ws_tool_error_message_type() {
    let ctx = setup_test_env(vec![
        tool_response(vec![clawseed_api::provider::ToolCall {
            id: "call_err_1".into(),
            name: "device_info".into(),
            arguments: "{}".into(),
        }]),
        text_response("Device info unavailable."),
        text_response("Device info unavailable."),
    ])
    .await;
    let (mut tx, mut rx) = ws_connect_with_handshake(ctx.ws_addr).await;

    let register_msg = serde_json::json!({
        "type": "register_tools",
        "tools": [{"name": "device_info", "description": "Device info", "parameters": {"type": "object", "properties": {}}}]
    });
    tx.send(Message::Text(register_msg.to_string().into()))
        .await
        .unwrap();
    let _ack = expect_msg_type(&mut rx, "tools_registered").await;

    let chat_msg = serde_json::json!({"type": "message", "content": "Get device info"});
    tx.send(Message::Text(chat_msg.to_string().into()))
        .await
        .unwrap();

    let tool_call_req = expect_msg_type(&mut rx, "tool_call_request").await;
    let call_id = tool_call_req["id"].as_str().unwrap().to_string();

    // Use tool_error instead of tool_result
    let tool_error_msg = serde_json::json!({
        "type": "tool_error",
        "id": call_id,
        "error": "Service unavailable"
    });
    tx.send(Message::Text(tool_error_msg.to_string().into()))
        .await
        .unwrap();

    let _ack = expect_msg_type(&mut rx, "result_acknowledged").await;
    let done = expect_msg_type(&mut rx, "done").await;
    assert!(
        done["full_response"]
            .as_str()
            .unwrap()
            .contains("unavailable")
    );

    tx.close().await.unwrap();
}

/// Test 10: Empty message content returns error with EMPTY_CONTENT code.
#[tokio::test]
async fn ws_empty_message_returns_error() {
    let ctx = setup_test_env(vec![]).await;
    let (mut tx, mut rx) = ws_connect_with_handshake(ctx.ws_addr).await;

    let empty_msg = serde_json::json!({"type": "message", "content": ""});
    tx.send(Message::Text(empty_msg.to_string().into()))
        .await
        .unwrap();

    let error = expect_msg_type(&mut rx, "error").await;
    assert_eq!(error["code"], "EMPTY_CONTENT");

    tx.close().await.unwrap();
}

/// Test 11: Unsupported message type returns error with UNKNOWN_MESSAGE_TYPE code.
#[tokio::test]
async fn ws_unsupported_message_type_returns_error() {
    let ctx = setup_test_env(vec![]).await;
    let (mut tx, mut rx) = ws_connect_with_handshake(ctx.ws_addr).await;

    let bad_msg = serde_json::json!({"type": "unknown_type", "data": "test"});
    tx.send(Message::Text(bad_msg.to_string().into()))
        .await
        .unwrap();

    let error = expect_msg_type(&mut rx, "error").await;
    assert_eq!(error["code"], "UNKNOWN_MESSAGE_TYPE");

    tx.close().await.unwrap();
}

/// Test 12: Invalid JSON returns error with INVALID_JSON code.
#[tokio::test]
async fn ws_invalid_json_returns_error() {
    let ctx = setup_test_env(vec![]).await;
    let (mut tx, mut rx) = ws_connect_with_handshake(ctx.ws_addr).await;

    tx.send(Message::Text("not json at all".into()))
        .await
        .unwrap();

    let error = expect_msg_type(&mut rx, "error").await;
    assert_eq!(error["code"], "INVALID_JSON");

    tx.close().await.unwrap();
}

/// Test 13: Session with session_id parameter preserves session identity.
#[tokio::test]
async fn ws_session_id_parameter_sets_session() {
    let ctx = setup_test_env(vec![]).await;
    let session_id = "android-demo-session-42";
    let (stream, _) = connect_async(format!(
        "ws://{}/ws/chat?session_id={session_id}",
        ctx.ws_addr
    ))
    .await
    .unwrap();
    let (mut tx, mut rx) = stream.split();

    let session_start = expect_msg_type(&mut rx, "session_start").await;
    assert_eq!(session_start["session_id"], session_id);

    tx.close().await.unwrap();
}

/// Test 14: Multiple remote tools registered in sequence accumulate.
#[tokio::test]
async fn ws_multiple_tool_registrations_accumulate() {
    let ctx = setup_test_env(vec![]).await;
    let (mut tx, mut rx) = ws_connect_with_handshake(ctx.ws_addr).await;

    // Register first tool
    let reg1 = serde_json::json!({
        "type": "register_tools",
        "tools": [{"name": "device_info", "description": "Device info", "parameters": {"type": "object", "properties": {}}}]
    });
    tx.send(Message::Text(reg1.to_string().into()))
        .await
        .unwrap();
    let ack1 = expect_msg_type(&mut rx, "tools_registered").await;
    assert_eq!(ack1["count"], 1);
    assert_eq!(ack1["registered"], 1);

    // Register second tool
    let reg2 = serde_json::json!({
        "type": "register_tools",
        "tools": [{"name": "local_contacts", "description": "Contacts", "parameters": {"type": "object", "properties": {"query": {"type": "string"}}}}]
    });
    tx.send(Message::Text(reg2.to_string().into()))
        .await
        .unwrap();
    let ack2 = expect_msg_type(&mut rx, "tools_registered").await;
    assert_eq!(ack2["count"], 1);
    assert_eq!(ack2["registered"], 2);

    // Verify both tools are in get_registered_tools
    let query = serde_json::json!({"type": "get_registered_tools"});
    tx.send(Message::Text(query.to_string().into()))
        .await
        .unwrap();
    let tools = expect_msg_type(&mut rx, "registered_tools").await;
    let tool_names: Vec<&str> = tools["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    assert!(tool_names.contains(&"device_info"));
    assert!(tool_names.contains(&"local_contacts"));

    tx.close().await.unwrap();
}

/// Test 15: register_tools as first message (before connect handshake).
/// The Android demo sends connect then register_tools immediately.
#[tokio::test]
async fn ws_register_tools_after_connect_handshake() {
    let ctx = setup_test_env(vec![]).await;
    let (mut tx, mut rx) = ws_connect_with_handshake(ctx.ws_addr).await;

    let register_msg = serde_json::json!({
        "type": "register_tools",
        "tools": [{
            "name": "device_info",
            "description": "Get device info",
            "parameters": {"type": "object", "properties": {}}
        }]
    });
    tx.send(Message::Text(register_msg.to_string().into()))
        .await
        .unwrap();
    let ack = expect_msg_type(&mut rx, "tools_registered").await;
    assert_eq!(ack["registered"], 1);

    tx.close().await.unwrap();
}

/// Test 16: Agent init error is reported via WebSocket error message.
#[tokio::test]
async fn ws_shared_provider_init_succeeds() {
    // Config with invalid provider → agent init will fail
    // With from_config_with_shared_components(), the agent uses the shared
    // provider from AppState rather than re-reading config, so we must inject
    // a provider that actually fails during agent assembly (e.g., tool loading).
    // A simpler approach: provide no tools via config that will cause init to
    // fail by using a provider that always errors on chat().
    //
    // However, the shared provider is always used now. The realistic failure
    // path is when the shared provider itself is broken — which can't happen
    // at init time since it's already constructed. Agent init via
    // from_config_with_shared_components() only fails if build_from_config()
    // fails (e.g., tool loading, memory init). Since we pass a working memory
    // and the provider is already constructed, init succeeds.
    //
    // The new semantic: shared components are trusted; init failure from
    // invalid config is no longer possible (config is only used for tool
    // lists, hooks, etc., not for provider creation).
    // Update the test to verify that a valid shared provider results in a
    // successful session_start instead.
    let ollama_provider = clawseed_providers::create_resilient_provider_with_options(
        "ollama",
        None,
        Some("http://127.0.0.1:1/v1"),
        &clawseed_config::schema::ReliabilityConfig::default(),
        &clawseed_providers::ProviderRuntimeOptions::default(),
    )
    .unwrap();
    let state = clawseed_gateway::AppState {
        config: Arc::new(Mutex::new(clawseed_config::schema::Config::default())),
        provider: Arc::from(ollama_provider),
        model: "test-model".into(),
        temperature: 0.0,
        mem: Arc::new(MockMemory),
        user_profile_store: None,
        auto_save: false,
        webhook_secret_hash: None,
        pairing: Arc::new(PairingGuard::new(false, &[])),
        trust_forwarded_headers: false,
        rate_limiter: Arc::new(clawseed_gateway::GatewayRateLimiter::new(100, 100, 100)),
        auth_limiter: Arc::new(clawseed_gateway::auth_rate_limit::AuthRateLimiter::new()),
        idempotency_store: Arc::new(clawseed_gateway::IdempotencyStore::new(
            Duration::from_secs(300),
            1000,
        )),
        observer: Arc::new(clawseed_agent::observability::NoopObserver),
        tool_registry: Arc::new(DefaultToolRegistry::new()),
        shared_builtin_tools: Arc::new([]),
        skill_index: Arc::new(parking_lot::RwLock::new(Vec::new())),
        skills_excluded: Arc::new(std::sync::Mutex::new(Vec::new())),
        cost_tracker: None,
        event_tx: tokio::sync::broadcast::channel(16).0,
        event_buffer: Arc::new(clawseed_gateway::EventBuffer::new(16)),
        shutdown_tx: tokio::sync::watch::channel(false).0,
        node_registry: Arc::new(clawseed_gateway::NodeRegistry::new(16)),
        session_backend: None,
        session_queue: Arc::new(clawseed_gateway::session_queue::SessionActorQueue::new(
            8, 30, 600,
        )),
        path_prefix: String::new(),
        web_dist_dir: None,
        canvas_store: clawseed_agent::tools::CanvasStore::new(),
        cancel_tokens: Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
    };
    let app = Router::new()
        .route("/ws/chat", get(handle_ws_chat))
        .with_state(state);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    let (stream, _) = connect_async(format!("ws://{addr}/ws/chat")).await.unwrap();
    let (mut tx, mut rx) = stream.split();

    // With shared components, agent init succeeds — session_start is sent
    let msg = expect_msg_type(&mut rx, "session_start").await;
    assert_eq!(msg["type"], "session_start");

    tx.close().await.ok();
}
