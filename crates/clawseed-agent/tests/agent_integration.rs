//! End-to-end integration tests for agent orchestration.
//!
//! These tests exercise the full agent turn cycle through the public API,
//! using mock providers and tools to validate orchestration behavior without
//! external service dependencies.

mod common;

use common::helpers::{build_agent, build_agent_xml, build_recording_agent, text_response, tool_response};
use common::mock_provider::RecordingProvider;
use common::mock_tools::{CountingTool, EchoTool};
use clawseed_api::provider::{ChatMessage, ChatResponse, ConversationMessage, ToolCall};

// ═════════════════════════════════════════════════════════════════════════════
// E2E smoke tests — full agent turn cycle
// ═════════════════════════════════════════════════════════════════════════════

/// Validates the simplest happy path: user message → LLM text response.
#[tokio::test]
async fn e2e_simple_text_response() {
    let provider = Box::new(common::mock_provider::MockProvider::new(vec![text_response(
        "Hello from mock provider",
    )]));
    let mut agent = build_agent(provider, vec![Box::new(EchoTool)]);

    let response = agent.turn("hi").await.unwrap();
    assert!(!response.is_empty(), "Expected non-empty text response");
}

/// Validates single tool call → tool execution → final LLM response.
#[tokio::test]
async fn e2e_single_tool_call_cycle() {
    let provider = Box::new(common::mock_provider::MockProvider::new(vec![
        tool_response(vec![ToolCall {
            id: "tc1".into(),
            name: "echo".into(),
            arguments: r#"{"message": "hello from tool"}"#.into(),
        }]),
        text_response("Tool executed successfully"),
    ]));

    let mut agent = build_agent(provider, vec![Box::new(EchoTool)]);
    let response = agent.turn("run echo").await.unwrap();
    assert!(
        !response.is_empty(),
        "Expected non-empty response after tool execution"
    );
}

/// Validates multi-step tool chain: tool A → tool B → tool C → final response.
#[tokio::test]
async fn e2e_multi_step_tool_chain() {
    let (counting_tool, count) = CountingTool::new();

    let provider = Box::new(common::mock_provider::MockProvider::new(vec![
        tool_response(vec![ToolCall {
            id: "tc1".into(),
            name: "counter".into(),
            arguments: "{}".into(),
        }]),
        tool_response(vec![ToolCall {
            id: "tc2".into(),
            name: "counter".into(),
            arguments: "{}".into(),
        }]),
        text_response("Done after 2 tool calls"),
    ]));

    let mut agent = build_agent(provider, vec![Box::new(counting_tool)]);
    let response = agent.turn("count twice").await.unwrap();
    assert!(
        !response.is_empty(),
        "Expected non-empty response after tool chain"
    );
    assert_eq!(*count.lock(), 2);
}

/// Validates that the XML dispatcher path also works end-to-end.
#[tokio::test]
async fn e2e_xml_dispatcher_tool_call() {
    let provider = Box::new(common::mock_provider::MockProvider::new(vec![
        ChatResponse {
            text: Some(
                r#"◁
{"name": "echo", "arguments": {"message": "xml dispatch"}}
▷"#
                    .into(),
            ),
            tool_calls: vec![],
            usage: None,
            reasoning_content: None,
        },
        text_response("XML tool executed"),
    ]));

    let mut agent = build_agent_xml(provider, vec![Box::new(EchoTool)]);
    let response = agent.turn("test xml dispatch").await.unwrap();
    assert!(
        !response.is_empty(),
        "Expected non-empty response from XML dispatcher"
    );
}

/// Validates that multiple sequential turns maintain conversation coherence.
#[tokio::test]
async fn e2e_multi_turn_conversation() {
    let provider = Box::new(common::mock_provider::MockProvider::new(vec![
        text_response("First response"),
        text_response("Second response"),
        text_response("Third response"),
    ]));

    let mut agent = build_agent(provider, vec![Box::new(EchoTool)]);

    let r1 = agent.turn("turn 1").await.unwrap();
    assert!(!r1.is_empty(), "Expected non-empty first response");

    let r2 = agent.turn("turn 2").await.unwrap();
    assert!(!r2.is_empty(), "Expected non-empty second response");
    assert_ne!(r1, r2, "Sequential turn responses should be distinct");

    let r3 = agent.turn("turn 3").await.unwrap();
    assert!(!r3.is_empty(), "Expected non-empty third response");
    assert_ne!(r2, r3, "Sequential turn responses should be distinct");
}

/// Validates that the agent handles unknown tool names gracefully.
#[tokio::test]
async fn e2e_unknown_tool_recovery() {
    let provider = Box::new(common::mock_provider::MockProvider::new(vec![
        tool_response(vec![ToolCall {
            id: "tc1".into(),
            name: "nonexistent_tool".into(),
            arguments: "{}".into(),
        }]),
        text_response("Recovered from unknown tool"),
    ]));

    let mut agent = build_agent(provider, vec![Box::new(EchoTool)]);
    let response = agent.turn("call missing tool").await.unwrap();
    assert!(
        !response.is_empty(),
        "Expected non-empty response after unknown tool recovery"
    );
}

/// Validates parallel tool dispatch in a single response.
#[tokio::test]
async fn e2e_parallel_tool_dispatch() {
    let (counting_tool, count) = CountingTool::new();

    let provider = Box::new(common::mock_provider::MockProvider::new(vec![
        tool_response(vec![
            ToolCall {
                id: "tc1".into(),
                name: "counter".into(),
                arguments: "{}".into(),
            },
            ToolCall {
                id: "tc2".into(),
                name: "counter".into(),
                arguments: "{}".into(),
            },
        ]),
        text_response("Both tools ran"),
    ]));

    let mut agent = build_agent(provider, vec![Box::new(counting_tool)]);
    let response = agent.turn("run both").await.unwrap();
    assert!(
        !response.is_empty(),
        "Expected non-empty response after parallel dispatch"
    );
    assert_eq!(*count.lock(), 2);
}

// ═════════════════════════════════════════════════════════════════════════════
// Multi-turn history fidelity tests
// ═════════════════════════════════════════════════════════════════════════════

/// Validates that multi-turn conversation correctly accumulates history
/// and passes growing message sequences to the provider on each turn.
#[tokio::test]
async fn e2e_multi_turn_history_fidelity() {
    let (provider, recorded) = RecordingProvider::new(vec![
        text_response("response 1"),
        text_response("response 2"),
        text_response("response 3"),
    ]);

    let mut agent = build_recording_agent(Box::new(provider), vec![]);

    let r1 = agent.turn("msg 1").await.unwrap();
    assert_eq!(r1, "response 1");

    let r2 = agent.turn("msg 2").await.unwrap();
    assert_eq!(r2, "response 2");

    let r3 = agent.turn("msg 3").await.unwrap();
    assert_eq!(r3, "response 3");

    let requests = recorded.lock().unwrap();
    assert_eq!(requests.len(), 3, "Provider should receive 3 requests");

    // Request 1: system + user("msg 1")
    let req1 = &requests[0];
    assert!(req1.len() >= 2);
    assert_eq!(req1[0].role, "system");
    assert_eq!(req1[1].role, "user");
    assert!(req1[1].content.contains("msg 1"));

    // Request 2: system + user("msg 1") + assistant("response 1") + user("msg 2")
    let req2 = &requests[1];
    let req2_users: Vec<&ChatMessage> = req2.iter().filter(|m| m.role == "user").collect();
    let req2_assts: Vec<&ChatMessage> = req2.iter().filter(|m| m.role == "assistant").collect();
    assert_eq!(req2_users.len(), 2, "Request 2: expected 2 user messages");
    assert_eq!(
        req2_assts.len(),
        1,
        "Request 2: expected 1 assistant message"
    );
    assert!(req2_users[0].content.contains("msg 1"));
    assert!(req2_users[1].content.contains("msg 2"));
    assert_eq!(req2_assts[0].content, "response 1");

    // Request 3: full history — 3 user + 2 assistant messages
    let req3 = &requests[2];
    let req3_users: Vec<&ChatMessage> = req3.iter().filter(|m| m.role == "user").collect();
    let req3_assts: Vec<&ChatMessage> = req3.iter().filter(|m| m.role == "assistant").collect();
    assert_eq!(req3_users.len(), 3, "Request 3: expected 3 user messages");
    assert_eq!(
        req3_assts.len(),
        2,
        "Request 3: expected 2 assistant messages"
    );
    assert!(req3_users[0].content.contains("msg 1"));
    assert!(req3_users[1].content.contains("msg 2"));
    assert!(req3_users[2].content.contains("msg 3"));
    assert_eq!(req3_assts[0].content, "response 1");
    assert_eq!(req3_assts[1].content, "response 2");

    // Verify agent history: system + 3*(user + assistant) = 7
    let history = agent.history();
    assert_eq!(history.len(), 7);
    assert!(matches!(&history[0], ConversationMessage::Chat(c) if c.role == "system"));
    assert!(matches!(&history[1], ConversationMessage::Chat(c) if c.role == "user"));
    assert!(matches!(&history[2], ConversationMessage::Chat(c) if c.role == "assistant"));
    assert!(
        matches!(&history[6], ConversationMessage::Chat(c) if c.role == "assistant" && c.content == "response 3")
    );
}
