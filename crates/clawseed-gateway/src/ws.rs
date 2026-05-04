//! WebSocket agent chat handler.
//!
//! Connect: `ws://host:port/ws/chat?session_id=ID&name=My+Session`
//!
//! Protocol (Server → Client):
//! ```text
//! {"type":"session_start","session_id":"...","name":"...","resumed":true,"message_count":42}
//! {"type":"chunk","content":"Hi! "}
//! {"type":"thinking","content":"..."}
//! {"type":"tool_call","id":"...","name":"shell","args":{...}}
//! {"type":"tool_result","id":"...","name":"shell","output":"..."}
//! {"type":"tool_call_request","id":"...","name":"local_contacts","args":{...}}  ← Android bridge
//! {"type":"result_acknowledged","id":"..."}                                      ← Android bridge
//! {"type":"tools_registered","count":N,"registered":N}                          ← Android bridge
//! {"type":"registered_tools","tools":[...]}                                     ← Android bridge
//! {"type":"done","full_response":"..."}
//! {"type":"title_updated","title":"..."}
//! {"type":"aborted"}
//! {"type":"error","message":"...","code":"..."}
//! ```
//!
//! Protocol (Client → Server):
//! ```text
//! {"type":"connect","session_id":"...","device_name":"...","capabilities":[...]}
//! {"type":"message","content":"Hello"}
//! {"type":"register_tools","tools":[{"name":"...","description":"...","parameters":{...}},...]}
//! {"type":"tool_result","id":"...","output":"...","success":true}                ← Android bridge
//! {"type":"tool_result","id":"...","output":"...","success":false,"error":"..."}← Android bridge
//! {"type":"tool_error","id":"...","error":"Permission denied"}                   ← Android bridge
//! {"type":"get_registered_tools"}                                                ← Android bridge
//! ```
//!
//! Query params:
//! - `session_id` — resume or create a session (default: new UUID)
//! - `name` — optional human-readable label for the session
//! - `token` — bearer auth token (alternative to Authorization header)

use super::AppState;
use super::session_backend::SessionBackend;
use axum::{
    extract::{
        Query, State, WebSocketUpgrade,
        ws::{Message, WebSocket},
    },
    http::{HeaderMap, header},
    response::IntoResponse,
};
use clawseed_api::tool_registry::ToolSource;
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use std::sync::Arc;
use tracing::debug;

/// Optional connection parameters sent as the first WebSocket message.
///
/// If the first message after upgrade is `{"type":"connect",...}`, these
/// parameters are extracted and an acknowledgement is sent back. Old clients
/// that send `{"type":"message",...}` as the first frame still work — the
/// message is processed normally (backward-compatible).
#[derive(Debug, Deserialize)]
struct ConnectParams {
    #[serde(rename = "type")]
    msg_type: String,
    /// Protocol version the client supports
    #[serde(default)]
    v: Option<u32>,
    /// Client-chosen session ID for memory persistence
    #[serde(default)]
    session_id: Option<String>,
    /// Device name for device registry tracking
    #[serde(default)]
    device_name: Option<String>,
    /// Client capabilities
    #[serde(default)]
    capabilities: Vec<String>,
}

// ── Remote Tool Registry (Android Integration PoC) ────────────────────────────
// Stores tools registered by WebSocket client (e.g., Android Kotlin UI) that
// should be executed on the client side rather than locally by the agent.
//
// Phase 2: Uses RemoteToolRegistryHandle from remote_tool.rs for execution.
// The handle contains an mpsc channel for sending requests to ws.rs,
// and oneshot channels for waiting for responses.

use crate::remote_tool::{
    RemoteTool, RemoteToolRegistryHandle, RemoteToolRequest, RemoteToolResult, RemoteToolSpec,
};

/// Pending remote tool calls awaiting response from WebSocket client.
/// Keyed by call_id, contains oneshot sender to complete the tool execution.
type PendingRemoteCalls =
    std::collections::HashMap<String, tokio::sync::oneshot::Sender<RemoteToolResult>>;

/// The sub-protocol we support for the chat WebSocket.
const WS_PROTOCOL: &str = "clawseed.v1";

/// Message protocol version for compatibility detection.
/// Clients should include `"v": MSG_PROTOCOL_VERSION` in their connect message.
/// Server includes it in session_start and connected responses.
pub const MSG_PROTOCOL_VERSION: u32 = 1;

/// Prefix used in `Sec-WebSocket-Protocol` to carry a bearer token.
const BEARER_SUBPROTO_PREFIX: &str = "bearer.";

#[derive(Deserialize)]
pub struct WsQuery {
    pub token: Option<String>,
    pub session_id: Option<String>,
    /// Optional human-readable name for the session.
    pub name: Option<String>,
}

/// Extract a bearer token from WebSocket-compatible sources.
///
/// Precedence (first non-empty wins):
/// 1. `Authorization: Bearer <token>` header
/// 2. `Sec-WebSocket-Protocol: bearer.<token>` subprotocol
/// 3. `?token=<token>` query parameter
///
/// Browsers cannot set custom headers on `new WebSocket(url)`, so the query
/// parameter and subprotocol paths are required for browser-based clients.
fn extract_ws_token<'a>(headers: &'a HeaderMap, query_token: Option<&'a str>) -> Option<&'a str> {
    // 1. Authorization header
    if let Some(t) = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|auth| auth.strip_prefix("Bearer "))
        && !t.is_empty()
    {
        return Some(t);
    }

    // 2. Sec-WebSocket-Protocol: bearer.<token>
    if let Some(t) = headers
        .get("sec-websocket-protocol")
        .and_then(|v| v.to_str().ok())
        .and_then(|protos| {
            protos
                .split(',')
                .map(|p| p.trim())
                .find_map(|p| p.strip_prefix(BEARER_SUBPROTO_PREFIX))
        })
        && !t.is_empty()
    {
        return Some(t);
    }

    // 3. ?token= query parameter
    if let Some(t) = query_token
        && !t.is_empty()
    {
        return Some(t);
    }

    None
}

/// GET /ws/chat — WebSocket upgrade for agent chat
pub async fn handle_ws_chat(
    State(state): State<AppState>,
    Query(params): Query<WsQuery>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    // Auth: check header, subprotocol, then query param (precedence order)
    if state.pairing.require_pairing() {
        let token = extract_ws_token(&headers, params.token.as_deref()).unwrap_or("");
        if !state.pairing.is_authenticated(token) {
            return (
                axum::http::StatusCode::UNAUTHORIZED,
                "Unauthorized — provide Authorization header, Sec-WebSocket-Protocol bearer, or ?token= query param",
            )
                .into_response();
        }
    }

    // Echo Sec-WebSocket-Protocol if the client requests our sub-protocol.
    let ws = if headers
        .get("sec-websocket-protocol")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|protos| protos.split(',').any(|p| p.trim() == WS_PROTOCOL))
    {
        ws.protocols([WS_PROTOCOL])
    } else {
        ws
    };

    let session_id = params.session_id;
    let session_name = params.name;
    ws.on_upgrade(move |socket| handle_socket(socket, state, session_id, session_name))
        .into_response()
}

/// Gateway session key prefix to avoid collisions with channel sessions.
const GW_SESSION_PREFIX: &str = "gw_";

async fn handle_socket(
    socket: WebSocket,
    state: AppState,
    session_id: Option<String>,
    session_name: Option<String>,
) {
    let (mut sender, mut receiver) = socket.split();

    // Resolve session ID: use provided or generate a new UUID
    let session_id = session_id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let session_key = format!("{GW_SESSION_PREFIX}{session_id}");

    // Build a persistent Agent for this connection so history is maintained across turns.
    let config = state.config.lock().clone();
    let mut agent = match clawseed_agent::agent::Agent::from_config(&config).await {
        Ok(a) => a,
        Err(e) => {
            tracing::error!(error = %e, "Agent initialization failed");
            let err = serde_json::json!({
                "type": "error",
                "message": format!("Failed to initialise agent: {e}"),
                "code": "AGENT_INIT_FAILED"
            });
            let _ = sender.send(Message::Text(err.to_string().into())).await;
            let _ = sender
                .send(Message::Close(Some(axum::extract::ws::CloseFrame {
                    code: 1011,
                    reason: axum::extract::ws::Utf8Bytes::from_static(
                        "Agent initialization failed",
                    ),
                })))
                .await;
            return;
        }
    };
    agent.set_memory_session_id(Some(session_id.clone()));

    // ── Remote tool registry for Android integration PoC (Phase 2) ────────────────
    // Creates an mpsc channel for RemoteTool to send execution requests.
    // Requests are handled in the main select! loop to send tool_call_request to client.
    let (remote_request_tx, mut remote_request_rx) =
        tokio::sync::mpsc::channel::<RemoteToolRequest>(32);

    // Pending calls awaiting response from client (call_id -> oneshot sender)
    let pending_remote_calls: std::sync::Arc<tokio::sync::RwLock<PendingRemoteCalls>> =
        std::sync::Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new()));

    // Shared handle for RemoteTool instances to send requests
    let remote_registry_handle = std::sync::Arc::new(tokio::sync::RwLock::new(
        RemoteToolRegistryHandle::new(remote_request_tx),
    ));

    // Hydrate agent from persisted session (if available)
    let mut resumed = false;
    let mut message_count: usize = 0;
    let mut effective_name: Option<String> = None;
    if let Some(ref backend) = state.session_backend {
        let messages = backend.load(&session_key);
        if !messages.is_empty() {
            message_count = messages.len();
            agent.seed_history(&messages);
            resumed = true;
        }
        // Set session name if provided (non-empty) on connect
        if let Some(ref name) = session_name
            && !name.is_empty()
        {
            let _ = backend.set_session_name(&session_key, name);
            effective_name = Some(name.clone());
        }
        // If no name was provided via query param, load the stored name
        if effective_name.is_none() {
            effective_name = backend.get_session_name(&session_key).unwrap_or(None);
        }
    }

    // Send session_start message to client
    let mut session_start = serde_json::json!({
        "type": "session_start",
        "v": MSG_PROTOCOL_VERSION,
        "session_id": session_id,
        "resumed": resumed,
        "message_count": message_count,
    });
    if let Some(ref name) = effective_name {
        session_start["name"] = serde_json::Value::String(name.clone());
    }
    let _ = sender
        .send(Message::Text(session_start.to_string().into()))
        .await;

    // ── Optional connect handshake ──────────────────────────────────
    // The first message may be a `{"type":"connect",...}` frame carrying
    // connection parameters.  If it is, we extract the params, send an
    // ack, and proceed to the normal message loop.  If the first message
    // is a regular `{"type":"message",...}` frame, we fall through and
    // process it immediately (backward-compatible).
    let mut first_msg_fallback: Option<String> = None;

    if let Some(first) = receiver.next().await {
        match first {
            Ok(Message::Text(text)) => {
                if let Ok(cp) = serde_json::from_str::<ConnectParams>(&text) {
                    if cp.msg_type == "connect" {
                        if let Some(client_v) = cp.v {
                            if client_v != MSG_PROTOCOL_VERSION {
                                tracing::warn!(
                                    client_version = client_v,
                                    server_version = MSG_PROTOCOL_VERSION,
                                    "Client protocol version mismatch"
                                );
                            }
                        } else {
                            tracing::debug!(
                                "Client did not send protocol version in connect message"
                            );
                        }
                        debug!(
                            session_id = ?cp.session_id,
                            device_name = ?cp.device_name,
                            capabilities = ?cp.capabilities,
                            "WebSocket connect params received"
                        );
                        // Override session_id if provided in connect params
                        if let Some(sid) = &cp.session_id {
                            agent.set_memory_session_id(Some(sid.clone()));
                        }
                        let ack = serde_json::json!({
                            "type": "connected",
                            "v": MSG_PROTOCOL_VERSION,
                            "message": "Connection established"
                        });
                        let _ = sender.send(Message::Text(ack.to_string().into())).await;
                    } else {
                        // Not a connect message — fall through to normal processing
                        first_msg_fallback = Some(text.to_string());
                    }
                } else {
                    // Not parseable as ConnectParams — fall through
                    first_msg_fallback = Some(text.to_string());
                }
            }
            Ok(Message::Close(_)) | Err(_) => return,
            _ => {}
        }
    }

    // Process the first message if it was not a connect frame
    if let Some(ref text) = first_msg_fallback {
        if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(text) {
            let msg_type = parsed["type"].as_str().unwrap_or("unknown");
            if msg_type == "message" {
                let content = parsed["content"].as_str().unwrap_or("").to_string();
                if !content.is_empty() {
                    let debug = parsed["debug"].as_bool().unwrap_or(false);
                    // Inject remote tools into agent before processing
                    {
                        let handle = remote_registry_handle.read().await;
                        if !handle.is_empty() {
                            let remote_tools = std::sync::Arc::new(handle.clone()).build_tools();
                            agent.add_remote_tools(remote_tools, session_id.clone());
                        }
                    }
                    // Persist user message
                    if let Some(ref backend) = state.session_backend {
                        let user_msg = clawseed_api::provider::ChatMessage::user(&content);
                        let _ = backend.append(&session_key, &user_msg);
                    }
                    process_chat_message(
                        &state,
                        &mut agent,
                        &mut sender,
                        &mut receiver,
                        &mut remote_request_rx,
                        pending_remote_calls.clone(),
                        &content,
                        &session_key,
                        debug,
                    )
                    .await;
                }
            } else if msg_type == "register_tools" {
                // Allow register_tools as first message (Android integration PoC)
                let tools = parsed["tools"].as_array().cloned().unwrap_or_default();
                let count = tools.len();
                {
                    let mut handle = remote_registry_handle.write().await;
                    for tool_json in tools {
                        if let Ok(spec) = serde_json::from_value::<RemoteToolSpec>(tool_json) {
                            debug!(
                                tool_name = %spec.name,
                                description = %spec.description,
                                "Remote tool registered (first message)"
                            );
                            // Register in shared tool_registry so /api/tools includes it
                            let remote_tool = RemoteTool::new(
                                spec.name.clone(),
                                spec.description.clone(),
                                spec.parameters.clone(),
                                Arc::new(handle.clone()),
                            );
                            state.tool_registry.register_or_replace(
                                Box::new(remote_tool),
                                ToolSource::Remote {
                                    session: session_id.clone(),
                                },
                            );
                            handle.register(spec);
                        }
                    }
                }
                let handle = remote_registry_handle.read().await;
                let ack = serde_json::json!({
                    "type": "tools_registered",
                    "count": count,
                    "registered": handle.len()
                });
                let _ = sender.send(Message::Text(ack.to_string().into())).await;
            } else {
                let err = serde_json::json!({
                    "type": "error",
                    "message": format!(
                        "Unsupported message type \"{msg_type}\". Send {{\"type\":\"message\",\"content\":\"your text\"}}"
                    )
                });
                let _ = sender.send(Message::Text(err.to_string().into())).await;
            }
        } else {
            let err = serde_json::json!({
                "type": "error",
                "message": "Invalid JSON. Send {\"type\":\"message\",\"content\":\"your text\"}"
            });
            let _ = sender.send(Message::Text(err.to_string().into())).await;
        }
    }

    // Subscribe to the shared broadcast channel so cron/heartbeat events
    // are forwarded to this WebSocket client.
    let mut broadcast_rx = state.event_tx.subscribe();

    loop {
        tokio::select! {
            // ── Client message ────────────────────────────────────────
            client_msg = receiver.next() => {
                let Some(msg) = client_msg else { break };
                let msg = match msg {
                    Ok(Message::Text(text)) => text,
                    Ok(Message::Close(_)) | Err(_) => break,
                    _ => continue,
                };

                // Parse incoming message
                let parsed: serde_json::Value = match serde_json::from_str(&msg) {
                    Ok(v) => v,
                    Err(e) => {
                        let err = serde_json::json!({
                            "type": "error",
                            "message": format!("Invalid JSON: {}", e),
                            "code": "INVALID_JSON"
                        });
                        let _ = sender.send(Message::Text(err.to_string().into())).await;
                        continue;
                    }
                };

                let msg_type = parsed["type"].as_str().unwrap_or("");

                // ── Remote tool protocol (Android integration PoC) ────────────────
                // Handle register_tools: client registers tools it can execute locally
                if msg_type == "register_tools" {
                    let tools = parsed["tools"].as_array().cloned().unwrap_or_default();
                    let count = tools.len();
                    {
                        let mut handle = remote_registry_handle.write().await;
                        for tool_json in tools {
                            if let Ok(spec) = serde_json::from_value::<RemoteToolSpec>(tool_json) {
                                debug!(
                                    tool_name = %spec.name,
                                    description = %spec.description,
                                    "Remote tool registered"
                                );
                                // Register in shared tool_registry so /api/tools includes it
                                let remote_tool = RemoteTool::new(
                                    spec.name.clone(),
                                    spec.description.clone(),
                                    spec.parameters.clone(),
                                    Arc::new(handle.clone()),
                                );
                                state.tool_registry.register_or_replace(
                                    Box::new(remote_tool),
                                    ToolSource::Remote { session: session_id.clone() },
                                );
                                handle.register(spec);
                            } else {
                                tracing::warn!("Failed to parse remote tool spec");
                            }
                        }
                    }
                    let handle = remote_registry_handle.read().await;
                    let ack = serde_json::json!({
                        "type": "tools_registered",
                        "count": count,
                        "registered": handle.len()
                    });
                    let _ = sender.send(Message::Text(ack.to_string().into())).await;
                    continue;
                }

                // Handle tool_result / tool_error: client returns result of a remote tool execution.
                // Reuses the same parse helper used inside process_chat_message.
                if msg_type == "tool_result" || msg_type == "tool_error" {
                    handle_mid_turn_client_message(&msg, &mut sender, &pending_remote_calls).await;
                    continue;
                }

                // Handle get_registered_tools: client queries available remote tools
                if msg_type == "get_registered_tools" {
                    let handle = remote_registry_handle.read().await;
                    let tools: Vec<serde_json::Value> = handle
                        .specs
                        .values()
                        .map(|spec| {
                            serde_json::json!({
                                "name": spec.name,
                                "description": spec.description,
                                "parameters": spec.parameters
                            })
                        })
                        .collect();
                    let response = serde_json::json!({
                        "type": "registered_tools",
                        "tools": tools
                    });
                    let _ = sender.send(Message::Text(response.to_string().into())).await;
                    continue;
                }

                if msg_type == "abort" {
                    let token = state
                        .cancel_tokens
                        .lock()
                        .expect("cancel_tokens lock poisoned")
                        .get(&session_key)
                        .cloned();
                    if let Some(token) = token {
                        token.cancel();
                        tracing::info!(session_key, "session abort via WebSocket");
                        let ack = serde_json::json!({ "type": "abort_ack", "status": "aborted" });
                        let _ = sender.send(Message::Text(ack.to_string().into())).await;
                    } else {
                        let ack = serde_json::json!({ "type": "abort_ack", "status": "no_active_response" });
                        let _ = sender.send(Message::Text(ack.to_string().into())).await;
                    }
                    continue;
                }

                if msg_type != "message" {
                    let err = serde_json::json!({
                        "type": "error",
                        "message": format!(
                            "Unsupported message type \"{msg_type}\". Send {{\"type\":\"message\",\"content\":\"your text\"}}"
                        ),
                        "code": "UNKNOWN_MESSAGE_TYPE"
                    });
                    let _ = sender.send(Message::Text(err.to_string().into())).await;
                    continue;
                }

                let content = parsed["content"].as_str().unwrap_or("").to_string();
                let debug = parsed["debug"].as_bool().unwrap_or(false);
                if content.is_empty() {
                    let err = serde_json::json!({
                        "type": "error",
                        "message": "Message content cannot be empty",
                        "code": "EMPTY_CONTENT"
                    });
                    let _ = sender.send(Message::Text(err.to_string().into())).await;
                    continue;
                }

                // Inject remote tools into agent before processing (Phase 2)
                {
                    let handle = remote_registry_handle.read().await;
                    if !handle.is_empty() {
                        let remote_tools = std::sync::Arc::new(handle.clone()).build_tools();
                        agent.add_remote_tools(remote_tools, session_id.clone());
                    }
                }

                // Acquire session lock to serialize concurrent turns
                let _session_guard = match state.session_queue.acquire(&session_key).await {
                    Ok(guard) => guard,
                    Err(e) => {
                        let err = serde_json::json!({
                            "type": "error",
                            "message": e.to_string(),
                            "code": "SESSION_BUSY"
                        });
                        let _ = sender.send(Message::Text(err.to_string().into())).await;
                        continue;
                    }
                };

                // Persist user message
                if let Some(ref backend) = state.session_backend {
                    let user_msg = clawseed_api::provider::ChatMessage::user(&content);
                    let _ = backend.append(&session_key, &user_msg);
                }

                process_chat_message(
                    &state,
                    &mut agent,
                    &mut sender,
                    &mut receiver,
                    &mut remote_request_rx,
                    pending_remote_calls.clone(),
                    &content,
                    &session_key,
                    debug,
                )
                .await;
            }

            // ── Broadcast event (cron/heartbeat results) ──────────────
            event = broadcast_rx.recv() => {
                if let Ok(event) = event {
                    let _ = sender.send(Message::Text(event.to_string().into())).await;
                }
            }
        }
    }

    // ── Cleanup: unregister remote tools from shared registry on disconnect ──
    state
        .tool_registry
        .unregister_by_source(&ToolSource::Remote {
            session: session_id.clone(),
        });
}

// ── Remote tool message helpers ───────────────────────────────────────────────
//
// `parse_mid_turn_message` is pure (no I/O) and can be unit-tested directly.
// `handle_mid_turn_client_message` wraps it with WebSocket I/O and is called
// from both the outer loop (inter-turn stray messages) and `process_chat_message`
// (messages arriving while a turn is running).

/// Parse a `tool_result` or `tool_error` client→server WebSocket message.
///
/// Returns `(call_id, RemoteToolResult, ack_json)` on success, or `None` if the
/// message type is not recognized or the `id` field is absent/empty.
///
/// Differences between the two message types:
/// - **`tool_result`**: `output` ← JSON `output` field; `error` ← optional JSON `error` field.
///   `success` is taken from the JSON `success` field (default `true`).
/// - **`tool_error`**: `success = false`; both `output` and `error` are set to the
///   JSON `error` field so the LLM can read the failure reason as tool output.
fn parse_mid_turn_message(
    parsed: &serde_json::Value,
) -> Option<(String, RemoteToolResult, serde_json::Value)> {
    let msg_type = parsed["type"].as_str()?;
    let call_id = parsed["id"]
        .as_str()
        .filter(|id| !id.is_empty())?
        .to_string();
    let ack = serde_json::json!({ "type": "result_acknowledged", "id": call_id });

    let result = match msg_type {
        "tool_result" => {
            let output = parsed["output"].as_str().unwrap_or("").to_string();
            let success = parsed["success"].as_bool().unwrap_or(true);
            let error = parsed["error"].as_str().map(|e| e.to_string());
            RemoteToolResult {
                success,
                output,
                error,
            }
        }
        "tool_error" => {
            let error = parsed["error"]
                .as_str()
                .unwrap_or("Unknown error")
                .to_string();
            RemoteToolResult {
                success: false,
                output: error.clone(),
                error: Some(error),
            }
        }
        _ => return None,
    };

    Some((call_id, result, ack))
}

/// Process a `tool_result` or `tool_error` message from the WebSocket client.
///
/// Completes the matching pending oneshot channel so `RemoteTool::execute` can
/// return, then sends a `result_acknowledged` ack back to the client.
/// Unknown message types are silently ignored (debug-logged).
async fn handle_mid_turn_client_message(
    msg_text: &str,
    sender: &mut futures_util::stream::SplitSink<WebSocket, Message>,
    pending_remote_calls: &tokio::sync::RwLock<PendingRemoteCalls>,
) {
    let Ok(parsed) = serde_json::from_str::<serde_json::Value>(msg_text) else {
        return;
    };
    if let Some((call_id, result, ack)) = parse_mid_turn_message(&parsed) {
        debug!(
            call_id = %call_id,
            success = result.success,
            "Remote tool result received"
        );
        if let Some(response_tx) = pending_remote_calls.write().await.remove(&call_id) {
            let _ = response_tx.send(result);
        } else {
            tracing::debug!(call_id = %call_id, "No pending remote call found for id");
        }
        let _ = sender.send(Message::Text(ack.to_string().into())).await;
    } else {
        let msg_type = parsed["type"].as_str().unwrap_or("?");
        tracing::debug!(
            msg_type = %msg_type,
            "Unexpected message received during active agent turn — ignored"
        );
    }
}

/// Process a single chat message through the agent and send the response.
///
/// Uses [`Agent::turn_streamed`] so that intermediate text chunks, tool calls,
/// and tool results are forwarded to the WebSocket client in real time.
///
/// `receiver`, `remote_request_rx`, and `pending_remote_calls` are needed to
/// concurrently handle the remote tool bridge: while the agent turn is running,
/// this function drains remote tool requests from `remote_request_rx` (forwarding
/// them to the client) and reads `tool_result`/`tool_error` replies back from
/// `receiver`.  Without this, both channels would be blocked by the outer
/// `select!` loop and every remote tool call would hit its 30-second timeout.
#[allow(clippy::too_many_arguments)]
async fn process_chat_message(
    state: &AppState,
    agent: &mut clawseed_agent::agent::Agent,
    sender: &mut futures_util::stream::SplitSink<WebSocket, Message>,
    receiver: &mut futures_util::stream::SplitStream<WebSocket>,
    remote_request_rx: &mut tokio::sync::mpsc::Receiver<RemoteToolRequest>,
    pending_remote_calls: std::sync::Arc<tokio::sync::RwLock<PendingRemoteCalls>>,
    content: &str,
    session_key: &str,
    debug: bool,
) {
    use clawseed_agent::agent::TurnEvent;

    let provider_label = state
        .config
        .lock()
        .providers
        .fallback
        .clone()
        .unwrap_or_else(|| "unknown".to_string());

    // Broadcast agent_start event
    let _ = state.event_tx.send(serde_json::json!({
        "type": "agent_start",
        "provider": provider_label,
        "model": state.model,
    }));

    // Set session state to running
    let turn_id = uuid::Uuid::new_v4().to_string();
    if let Some(ref backend) = state.session_backend {
        let _ = backend.set_session_state(session_key, "running", Some(&turn_id));
    }

    // ── Cancellation token lifecycle ─────────────────────────────
    // Create a token before the turn starts so the abort endpoint
    // can cancel it. Remove it after the turn completes regardless
    // of outcome (normal, error, or cancelled).
    let cancel_token = tokio_util::sync::CancellationToken::new();
    {
        state
            .cancel_tokens
            .lock()
            .expect("cancel_tokens lock poisoned")
            .insert(session_key.to_string(), cancel_token.clone());
    }

    // Channel for streaming turn events from the agent.
    let (event_tx, mut event_rx) = tokio::sync::mpsc::channel::<TurnEvent>(64);

    // Run the streamed turn concurrently: the agent produces events
    // while we forward them to the WebSocket below.  We cannot move
    // `agent` into a spawned task (it is `&mut`), so we use a join
    // instead — `turn_streamed` writes to the channel and we drain it
    // from the other branch.
    let content_owned = content.to_string();
    let turn_fut = async {
        agent
            .turn_streamed(&content_owned, event_tx, Some(cancel_token.clone()), debug)
            .await
    };

    // Drive both futures concurrently: the agent turn produces events
    // and we relay them over WebSocket. Track streamed chunks so we
    // can reconstruct partial content on cancellation.
    //
    // WHY incremental persistence: If the process crashes during streaming,
    // the assistant's response is lost — only the user message survives.
    // We append a placeholder assistant message on the first chunk, then
    // update_last periodically (every 500ms) so partial content survives.
    // The final response overwrites this via update_last on completion.
    let mut accumulated_text = String::new();
    let mut partial_saved = false;
    let mut last_partial_save = std::time::Instant::now();
    let partial_save_interval = std::time::Duration::from_millis(500);

    let forward_fut = async {
        loop {
            tokio::select! {
                // ── Agent events ────────────────────────────────────────
                event = event_rx.recv() => {
                    let Some(event) = event else { break };
                    let ws_msg = match event {
                        TurnEvent::Chunk { ref delta } => {
                            accumulated_text.push_str(delta);

                            // Incremental persistence: save partial content so it
                            // survives a crash. First chunk appends, subsequent
                            // chunks update in-place.
                            if last_partial_save.elapsed() >= partial_save_interval {
                                if let Some(ref backend) = state.session_backend {
                                    let partial =
                                        clawseed_api::provider::ChatMessage::assistant(&accumulated_text);
                                    if partial_saved {
                                        let _ = backend.update_last(session_key, &partial);
                                    } else {
                                        let _ = backend.append(session_key, &partial);
                                        partial_saved = true;
                                    }
                                }
                                last_partial_save = std::time::Instant::now();
                            }

                            serde_json::json!({ "type": "chunk", "content": delta })
                        }
                        TurnEvent::Thinking { delta } => {
                            serde_json::json!({ "type": "thinking", "content": delta })
                        }
                        TurnEvent::ToolCall { id, name, args } => {
                            serde_json::json!({ "type": "tool_call", "id": id, "name": name, "args": args })
                        }
                        TurnEvent::ToolResult { id, name, output } => {
                            serde_json::json!({ "type": "tool_result", "id": id, "name": name, "output": output })
                        }
                        TurnEvent::DebugPrompt { messages_json, estimated_tokens } => {
                            serde_json::json!({ "type": "debug_prompt", "messages": messages_json, "estimated_tokens": estimated_tokens })
                        }
                    };
                    let _ = sender.send(Message::Text(ws_msg.to_string().into())).await;
                }

                // ── Remote tool request (Android bridge) ──────────────
                // RemoteTool.execute() sends requests here while the turn is active.
                // Forward the request to the WebSocket client and store the pending
                // oneshot channel so the result can be routed back when it arrives.
                request = remote_request_rx.recv() => {
                    if let Some(req) = request {
                        let msg = serde_json::json!({
                            "type": "tool_call_request",
                            "id": req.call_id,
                            "name": req.tool_name,
                            "args": req.args,
                        });
                        if sender.send(Message::Text(msg.to_string().into())).await.is_err() {
                            let _ = req.response_tx.send(RemoteToolResult {
                                success: false,
                                output: String::new(),
                                error: Some("WebSocket send failed".into()),
                            });
                        } else {
                            pending_remote_calls.write().await.insert(req.call_id, req.response_tx);
                        }
                    }
                }

                // ── Incoming client messages during the turn ───────────
                // While the turn is running, the client may send tool_result or
                // tool_error replies for remote tool calls.  Handle them here so
                // the outer select! loop is not needed during the turn.
                ws_msg = receiver.next() => {
                    match ws_msg {
                        Some(Ok(Message::Text(ref text))) => {
                            handle_mid_turn_client_message(text, sender, &pending_remote_calls).await;
                        }
                        Some(Ok(Message::Close(_))) | None | Some(Err(_)) => {
                            cancel_token.cancel();
                            break;
                        }
                        _ => {}
                    }
                }
            }
        }
    };

    let (result, ()) = tokio::join!(turn_fut, forward_fut);

    // ── Remove cancel token (turn finished) ──────────────────────
    {
        state
            .cancel_tokens
            .lock()
            .expect("cancel_tokens lock poisoned")
            .remove(session_key);
    }

    // Check if this turn was cancelled. `turn_streamed` propagates
    // `ToolLoopCancelled` through anyhow, so we detect it here.
    let was_cancelled = match &result {
        Err(e) => e.to_string().contains("ToolLoopCancelled"),
        Ok(_) => false,
    };

    if was_cancelled {
        // Store partial content with interruption marker so the
        // conversation stays coherent for subsequent turns.
        let truncated = if accumulated_text.is_empty() {
            "[interrupted by user]".to_string()
        } else {
            format!("{accumulated_text}\n\n[interrupted by user]")
        };

        if let Some(ref backend) = state.session_backend {
            let assistant_msg = clawseed_api::provider::ChatMessage::assistant(&truncated);
            if partial_saved {
                let _ = backend.update_last(session_key, &assistant_msg);
            } else {
                let _ = backend.append(session_key, &assistant_msg);
            }
        }

        // Inform the client the turn was aborted
        let aborted = serde_json::json!({ "type": "aborted" });
        let _ = sender.send(Message::Text(aborted.to_string().into())).await;

        // Set session state to idle
        if let Some(ref backend) = state.session_backend {
            let _ = backend.set_session_state(session_key, "idle", None);
        }

        // Broadcast agent_end event
        let _ = state.event_tx.send(serde_json::json!({
            "type": "agent_end",
            "provider": provider_label,
            "model": state.model,
        }));

        return;
    }

    match result {
        Ok(response) => {
            // Persist final assistant response. If we saved partial content
            // during streaming, update it in-place; otherwise append fresh.
            if let Some(ref backend) = state.session_backend {
                let assistant_msg = clawseed_api::provider::ChatMessage::assistant(&response);
                if partial_saved {
                    let _ = backend.update_last(session_key, &assistant_msg);
                } else {
                    let _ = backend.append(session_key, &assistant_msg);
                }
            }

            // Fire-and-forget memory consolidation so facts from WS sessions
            // are extracted to long-term memory (Daily + Core categories).
            if state.auto_save {
                let mem = state.mem.clone();
                let provider = state.provider.clone();
                let model = state.model.clone();
                let user_msg = content.to_string();
                let assistant_resp = response.clone();
                tokio::spawn(async move {
                    if let Err(e) = clawseed_memory::consolidation::consolidate_turn(
                        provider.as_ref(),
                        &model,
                        mem.as_ref(),
                        &user_msg,
                        &assistant_resp,
                    )
                    .await
                    {
                        tracing::debug!("WS memory consolidation skipped: {e}");
                    }
                });
            }

            // ── Auto title generation (first turn only) ────────────
            // When the session still has the default title "新会话", use the
            // provider to generate a proper title from the first Q&A pair.
            // On failure, fall back to the first 15 characters of user message.
            let title_rx = if let Some(ref backend) = state.session_backend {
                let current_name = backend.get_session_name(session_key).ok().flatten();
                let needs_title = current_name.as_deref() == Some("新会话");
                if needs_title {
                    let provider = state.provider.clone();
                    let model = state.model.clone();
                    let user_msg = content.to_string();
                    let assistant_resp = response.clone();
                    let backend_clone: Arc<dyn SessionBackend> = Arc::clone(backend);
                    let session_key_owned = session_key.to_string();
                    let (tx, rx) = tokio::sync::oneshot::channel::<String>();
                    tokio::spawn(async move {
                        let prompt = format!(
                            "用户：{}\n助手：{}\n\n根据以上对话内容，生成一个简短的会话标题（20字以内）。\
                             只回复标题本身，不要加引号或其他内容。",
                            user_msg, assistant_resp,
                        );
                        let title = match provider
                            .chat_with_system(
                                Some("你是一个会话标题生成器。"),
                                &prompt,
                                &model,
                                Some(0.3),
                            )
                            .await
                        {
                            Ok(t) if !t.trim().is_empty() => {
                                tracing::info!(title = %t.trim(), "Auto-generated session title");
                                t.trim().to_string()
                            }
                            Ok(t) => {
                                tracing::warn!(raw = %t, "LLM returned empty title, using fallback");
                                let end = user_msg
                                    .char_indices()
                                    .nth(15)
                                    .map_or(user_msg.len(), |(i, _)| i);
                                user_msg[..end].to_string()
                            }
                            Err(e) => {
                                tracing::warn!(error = %e, "LLM title generation failed, using fallback");
                                let end = user_msg
                                    .char_indices()
                                    .nth(15)
                                    .map_or(user_msg.len(), |(i, _)| i);
                                user_msg[..end].to_string()
                            }
                        };
                        let _ = backend_clone.set_session_name(&session_key_owned, &title);
                        let _ = tx.send(title);
                    });
                    Some(rx)
                } else {
                    None
                }
            } else {
                None
            };

            // Send chunk_reset so the client clears any accumulated draft
            // before the authoritative done message.
            let reset = serde_json::json!({ "type": "chunk_reset" });
            let _ = sender.send(Message::Text(reset.to_string().into())).await;

            let done = serde_json::json!({
                "type": "done",
                "full_response": response,
            });
            let _ = sender.send(Message::Text(done.to_string().into())).await;

            // Set session state to idle
            if let Some(ref backend) = state.session_backend {
                let _ = backend.set_session_state(session_key, "idle", None);
            }

            // Broadcast agent_end event
            let _ = state.event_tx.send(serde_json::json!({
                "type": "agent_end",
                "provider": provider_label,
                "model": state.model,
            }));

            // Send title_updated event after everything else is complete.
            if let Some(rx) = title_rx
                && let Ok(title) = rx.await
            {
                let msg = serde_json::json!({
                    "type": "title_updated",
                    "title": title,
                });
                let _ = sender.send(Message::Text(msg.to_string().into())).await;
            }
        }
        Err(e) => {
            // Set session state to error
            if let Some(ref backend) = state.session_backend {
                let _ = backend.set_session_state(session_key, "error", Some(&turn_id));
            }

            tracing::error!(error = %e, "Agent turn failed");
            let sanitized = clawseed_providers::sanitize_api_error(&e.to_string());
            let error_code = if sanitized.to_lowercase().contains("api key")
                || sanitized.to_lowercase().contains("authentication")
                || sanitized.to_lowercase().contains("unauthorized")
            {
                "AUTH_ERROR"
            } else if sanitized.to_lowercase().contains("provider")
                || sanitized.to_lowercase().contains("model")
            {
                "PROVIDER_ERROR"
            } else {
                "AGENT_ERROR"
            };
            let err = serde_json::json!({
                "type": "error",
                "message": sanitized,
                "code": error_code,
            });
            let _ = sender.send(Message::Text(err.to_string().into())).await;

            // Broadcast error event
            let _ = state.event_tx.send(serde_json::json!({
                "type": "error",
                "component": "ws_chat",
                "message": sanitized,
            }));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderMap;

    #[test]
    fn extract_ws_token_from_authorization_header() {
        let mut headers = HeaderMap::new();
        headers.insert("authorization", "Bearer zc_test123".parse().unwrap());
        assert_eq!(extract_ws_token(&headers, None), Some("zc_test123"));
    }

    #[test]
    fn extract_ws_token_from_subprotocol() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "sec-websocket-protocol",
            "clawseed.v1, bearer.zc_sub456".parse().unwrap(),
        );
        assert_eq!(extract_ws_token(&headers, None), Some("zc_sub456"));
    }

    #[test]
    fn extract_ws_token_from_query_param() {
        let headers = HeaderMap::new();
        assert_eq!(
            extract_ws_token(&headers, Some("zc_query789")),
            Some("zc_query789")
        );
    }

    #[test]
    fn extract_ws_token_precedence_header_over_subprotocol() {
        let mut headers = HeaderMap::new();
        headers.insert("authorization", "Bearer zc_header".parse().unwrap());
        headers.insert("sec-websocket-protocol", "bearer.zc_sub".parse().unwrap());
        assert_eq!(
            extract_ws_token(&headers, Some("zc_query")),
            Some("zc_header")
        );
    }

    #[test]
    fn extract_ws_token_precedence_subprotocol_over_query() {
        let mut headers = HeaderMap::new();
        headers.insert("sec-websocket-protocol", "bearer.zc_sub".parse().unwrap());
        assert_eq!(extract_ws_token(&headers, Some("zc_query")), Some("zc_sub"));
    }

    #[test]
    fn extract_ws_token_returns_none_when_empty() {
        let headers = HeaderMap::new();
        assert_eq!(extract_ws_token(&headers, None), None);
    }

    #[test]
    fn extract_ws_token_skips_empty_header_value() {
        let mut headers = HeaderMap::new();
        headers.insert("authorization", "Bearer ".parse().unwrap());
        assert_eq!(
            extract_ws_token(&headers, Some("zc_fallback")),
            Some("zc_fallback")
        );
    }

    #[test]
    fn extract_ws_token_skips_empty_query_param() {
        let headers = HeaderMap::new();
        assert_eq!(extract_ws_token(&headers, Some("")), None);
    }

    #[test]
    fn extract_ws_token_subprotocol_with_multiple_entries() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "sec-websocket-protocol",
            "clawseed.v1, bearer.zc_tok, other".parse().unwrap(),
        );
        assert_eq!(extract_ws_token(&headers, None), Some("zc_tok"));
    }
}

#[cfg(test)]
mod mid_turn_tests {
    use super::*;

    // ── parse_mid_turn_message ────────────────────────────────────────────────

    #[test]
    fn parse_tool_result_success() {
        let parsed = serde_json::json!({
            "type": "tool_result",
            "id": "call_123",
            "output": "result data",
            "success": true
        });
        let (call_id, result, ack) = parse_mid_turn_message(&parsed).unwrap();
        assert_eq!(call_id, "call_123");
        assert!(result.success);
        assert_eq!(result.output, "result data");
        assert!(result.error.is_none());
        assert_eq!(ack["type"], "result_acknowledged");
        assert_eq!(ack["id"], "call_123");
    }

    #[test]
    fn parse_tool_result_failure_with_error_field() {
        let parsed = serde_json::json!({
            "type": "tool_result",
            "id": "call_456",
            "output": "partial output",
            "success": false,
            "error": "Something went wrong"
        });
        let (_call_id, result, _ack) = parse_mid_turn_message(&parsed).unwrap();
        assert!(!result.success);
        assert_eq!(result.output, "partial output");
        assert_eq!(result.error.as_deref(), Some("Something went wrong"));
    }

    #[test]
    fn parse_tool_result_failure_no_error_field() {
        // success=false without an "error" field — output is NOT duplicated into error
        let parsed = serde_json::json!({
            "type": "tool_result",
            "id": "call_789",
            "output": "failure description",
            "success": false
        });
        let (_call_id, result, _ack) = parse_mid_turn_message(&parsed).unwrap();
        assert!(!result.success);
        assert_eq!(result.output, "failure description");
        assert!(
            result.error.is_none(),
            "error should not be duplicated from output"
        );
    }

    #[test]
    fn parse_tool_error_sets_output_and_error() {
        // tool_error puts the error message into both output (visible to LLM) and error
        let parsed = serde_json::json!({
            "type": "tool_error",
            "id": "call_abc",
            "error": "Permission denied: READ_CONTACTS required"
        });
        let (call_id, result, ack) = parse_mid_turn_message(&parsed).unwrap();
        assert_eq!(call_id, "call_abc");
        assert!(!result.success);
        assert_eq!(result.output, "Permission denied: READ_CONTACTS required");
        assert_eq!(
            result.error.as_deref(),
            Some("Permission denied: READ_CONTACTS required")
        );
        assert_eq!(ack["type"], "result_acknowledged");
    }

    #[test]
    fn parse_tool_error_missing_error_field_uses_default() {
        let parsed = serde_json::json!({ "type": "tool_error", "id": "call_x" });
        let (_call_id, result, _ack) = parse_mid_turn_message(&parsed).unwrap();
        assert!(!result.success);
        assert_eq!(result.output, "Unknown error");
    }

    #[test]
    fn parse_unknown_type_returns_none() {
        let parsed = serde_json::json!({ "type": "register_tools", "id": "x", "tools": [] });
        assert!(parse_mid_turn_message(&parsed).is_none());
    }

    #[test]
    fn parse_missing_id_returns_none() {
        let parsed = serde_json::json!({ "type": "tool_result", "output": "foo", "success": true });
        assert!(parse_mid_turn_message(&parsed).is_none());
    }

    #[test]
    fn parse_empty_id_returns_none() {
        let parsed = serde_json::json!({
            "type": "tool_result",
            "id": "",
            "output": "foo",
            "success": true
        });
        assert!(parse_mid_turn_message(&parsed).is_none());
    }

    #[test]
    fn parse_tool_result_defaults_success_to_true_when_absent() {
        let parsed = serde_json::json!({
            "type": "tool_result",
            "id": "call_d",
            "output": "data"
        });
        let (_call_id, result, _ack) = parse_mid_turn_message(&parsed).unwrap();
        assert!(result.success);
    }
}
