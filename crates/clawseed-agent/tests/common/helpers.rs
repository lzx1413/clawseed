//! Shared builder helpers for constructing test agents.

use std::sync::Arc;

use clawseed_agent::agent::Agent;
use clawseed_agent::dispatcher::{NativeToolDispatcher, XmlToolDispatcher};
use clawseed_agent::observer::NoopObserver;
use clawseed_api::memory_traits::Memory;
use clawseed_api::provider::{ChatResponse, Provider, ToolCall};
use clawseed_api::tool::Tool;
use clawseed_memory;

/// Create an in-memory "none" backend for tests.
pub fn make_memory() -> Arc<dyn Memory> {
    Arc::new(clawseed_memory::none::NoneMemory::new())
}

/// Create a `NoopObserver` for tests.
pub fn make_observer() -> Arc<dyn clawseed_agent::observer::Observer> {
    Arc::new(NoopObserver)
}

/// Create a text-only `ChatResponse`.
pub fn text_response(text: &str) -> ChatResponse {
    ChatResponse {
        text: Some(text.into()),
        tool_calls: vec![],
        usage: None,
        reasoning_content: None,
    }
}

/// Create a `ChatResponse` with tool calls.
pub fn tool_response(calls: Vec<ToolCall>) -> ChatResponse {
    ChatResponse {
        text: Some(String::new()),
        tool_calls: calls,
        usage: None,
        reasoning_content: None,
    }
}

/// Build an agent with `NativeToolDispatcher`.
pub fn build_agent(provider: Box<dyn Provider>, tools: Vec<Box<dyn Tool>>) -> Agent {
    Agent::builder()
        .provider(provider)
        .tools(tools)
        .memory(make_memory())
        .observer(make_observer())
        .tool_dispatcher(Box::new(NativeToolDispatcher))
        .workspace_dir(std::env::temp_dir())
        .build()
        .unwrap()
}

/// Build an agent with `XmlToolDispatcher`.
pub fn build_agent_xml(provider: Box<dyn Provider>, tools: Vec<Box<dyn Tool>>) -> Agent {
    Agent::builder()
        .provider(provider)
        .tools(tools)
        .memory(make_memory())
        .observer(make_observer())
        .tool_dispatcher(Box::new(XmlToolDispatcher))
        .workspace_dir(std::env::temp_dir())
        .build()
        .unwrap()
}

/// Build a recording agent with `NativeToolDispatcher`.
pub fn build_recording_agent(provider: Box<dyn Provider>, tools: Vec<Box<dyn Tool>>) -> Agent {
    build_agent(provider, tools)
}

/// Build an agent with real `SqliteMemory` in a temporary directory.
pub fn build_agent_with_sqlite_memory(
    provider: Box<dyn Provider>,
    tools: Vec<Box<dyn Tool>>,
    temp_dir: &std::path::Path,
) -> Agent {
    let mem = Arc::new(
        clawseed_memory::sqlite::SqliteMemory::new(temp_dir).unwrap(),
    );
    Agent::builder()
        .provider(provider)
        .tools(tools)
        .memory(mem)
        .observer(make_observer())
        .tool_dispatcher(Box::new(NativeToolDispatcher))
        .workspace_dir(std::env::temp_dir())
        .build()
        .unwrap()
}
