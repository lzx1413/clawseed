//! Agent: a registry that holds tools, hooks, and context providers.
//!
//! The Agent accepts a message via `turn()`, sends it to the provider,
//! parses tool calls, dispatches to registered tools, and loops until done.

use crate::dispatcher::{
    ParsedToolCall, ToolDispatcher, ToolExecutionResult,
};
use crate::hooks::HookRunner;
use crate::observer::{Observer, ObserverEvent};
use clawseed_config::schema::AutonomyLevel;
use clawseed_api::provider::{
    ChatMessage, ChatRequest, ChatResponse, ConversationMessage, Provider,
};
use clawseed_api::tool::{Tool, ToolResult, ToolSpec};
use anyhow::Result;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use clawseed_api::memory_traits::{Memory, MemoryCategory};
use chrono::{Datelike, Timelike};

/// Streaming events emitted during an agent turn.
#[derive(Debug, Clone)]
pub enum TurnEvent {
    Chunk { delta: String },
    Thinking { delta: String },
    ToolCall { id: String, name: String, args: serde_json::Value },
    ToolResult { id: String, name: String, output: String },
}

/// The core Agent struct — a registry of tools, hooks, and context providers.
pub struct Agent {
    provider: Box<dyn Provider>,
    tools: Vec<Box<dyn Tool>>,
    tool_specs: Vec<ToolSpec>,
    memory: Arc<dyn Memory>,
    observer: Arc<dyn Observer>,
    tool_dispatcher: Box<dyn ToolDispatcher>,
    config: clawseed_config::schema::AgentConfig,
    model_name: String,
    temperature: f64,
    workspace_dir: std::path::PathBuf,
    autonomy_level: AutonomyLevel,
    auto_save: bool,
    memory_session_id: Option<String>,
    history: Vec<ConversationMessage>,
    _available_hints: Vec<String>,
    _route_model_by_hint: HashMap<String, String>,
    hook_runner: Option<Arc<HookRunner>>,
}

/// Builder for constructing an Agent.
pub struct AgentBuilder {
    provider: Option<Box<dyn Provider>>,
    tools: Option<Vec<Box<dyn Tool>>>,
    memory: Option<Arc<dyn Memory>>,
    observer: Option<Arc<dyn Observer>>,
    tool_dispatcher: Option<Box<dyn ToolDispatcher>>,
    config: Option<clawseed_config::schema::AgentConfig>,
    model_name: Option<String>,
    temperature: Option<f64>,
    workspace_dir: Option<std::path::PathBuf>,
    autonomy_level: Option<AutonomyLevel>,
    auto_save: Option<bool>,
    memory_session_id: Option<String>,
    available_hints: Option<Vec<String>>,
    route_model_by_hint: Option<HashMap<String, String>>,
    allowed_tools: Option<Vec<String>>,
    hook_runner: Option<Arc<HookRunner>>,
}

impl Default for AgentBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl AgentBuilder {
    pub fn new() -> Self {
        Self {
            provider: None,
            tools: None,
            memory: None,
            observer: None,
            tool_dispatcher: None,
            config: None,
            model_name: None,
            temperature: None,
            workspace_dir: None,
            autonomy_level: None,
            auto_save: None,
            memory_session_id: None,
            available_hints: None,
            route_model_by_hint: None,
            allowed_tools: None,
            hook_runner: None,
        }
    }

    pub fn provider(mut self, provider: Box<dyn Provider>) -> Self {
        self.provider = Some(provider);
        self
    }

    pub fn tools(mut self, tools: Vec<Box<dyn Tool>>) -> Self {
        self.tools = Some(tools);
        self
    }

    pub fn memory(mut self, memory: Arc<dyn Memory>) -> Self {
        self.memory = Some(memory);
        self
    }

    pub fn observer(mut self, observer: Arc<dyn Observer>) -> Self {
        self.observer = Some(observer);
        self
    }

    pub fn tool_dispatcher(mut self, tool_dispatcher: Box<dyn ToolDispatcher>) -> Self {
        self.tool_dispatcher = Some(tool_dispatcher);
        self
    }

    pub fn config(mut self, config: clawseed_config::schema::AgentConfig) -> Self {
        self.config = Some(config);
        self
    }

    pub fn model_name(mut self, model_name: String) -> Self {
        self.model_name = Some(model_name);
        self
    }

    pub fn temperature(mut self, temperature: f64) -> Self {
        self.temperature = Some(temperature);
        self
    }

    pub fn workspace_dir(mut self, workspace_dir: std::path::PathBuf) -> Self {
        self.workspace_dir = Some(workspace_dir);
        self
    }

    pub fn autonomy_level(mut self, level: AutonomyLevel) -> Self {
        self.autonomy_level = Some(level);
        self
    }

    pub fn auto_save(mut self, auto_save: bool) -> Self {
        self.auto_save = Some(auto_save);
        self
    }

    pub fn memory_session_id(mut self, session_id: Option<String>) -> Self {
        self.memory_session_id = session_id;
        self
    }

    pub fn available_hints(mut self, available_hints: Vec<String>) -> Self {
        self.available_hints = Some(available_hints);
        self
    }

    pub fn route_model_by_hint(mut self, route_model_by_hint: HashMap<String, String>) -> Self {
        self.route_model_by_hint = Some(route_model_by_hint);
        self
    }

    pub fn allowed_tools(mut self, allowed_tools: Option<Vec<String>>) -> Self {
        self.allowed_tools = allowed_tools;
        self
    }

    pub fn hook_runner(mut self, runner: Option<Arc<HookRunner>>) -> Self {
        self.hook_runner = runner;
        self
    }

    pub fn build(self) -> Result<Agent> {
        let mut tools = self
            .tools
            .ok_or_else(|| anyhow::anyhow!("tools are required"))?;
        let allowed = self.allowed_tools.clone();
        if let Some(ref allow_list) = allowed {
            tools.retain(|t| allow_list.iter().any(|name| name == t.name()));
        }
        let tool_specs = tools.iter().map(|tool| tool.spec()).collect();

        Ok(Agent {
            provider: self
                .provider
                .ok_or_else(|| anyhow::anyhow!("provider is required"))?,
            tools,
            tool_specs,
            memory: self
                .memory
                .ok_or_else(|| anyhow::anyhow!("memory is required"))?,
            observer: self
                .observer
                .ok_or_else(|| anyhow::anyhow!("observer is required"))?,
            tool_dispatcher: self
                .tool_dispatcher
                .ok_or_else(|| anyhow::anyhow!("tool_dispatcher is required"))?,
            config: self.config.unwrap_or_default(),
            model_name: self.model_name.unwrap_or_else(|| "<unconfigured>".into()),
            temperature: self.temperature.unwrap_or(0.7),
            workspace_dir: self
                .workspace_dir
                .unwrap_or_else(|| std::path::PathBuf::from(".")),
            autonomy_level: self.autonomy_level.unwrap_or_default(),
            auto_save: self.auto_save.unwrap_or(false),
            memory_session_id: self.memory_session_id,
            history: Vec::new(),
            _available_hints: self.available_hints.unwrap_or_default(),
            _route_model_by_hint: self.route_model_by_hint.unwrap_or_default(),
            hook_runner: self.hook_runner,
        })
    }
}

impl Agent {
    pub fn builder() -> AgentBuilder {
        AgentBuilder::new()
    }

    /// Build an agent from the full config (stub).
    pub async fn from_config(_config: &clawseed_config::schema::Config) -> anyhow::Result<Self> {
        anyhow::bail!("Agent::from_config stub: not available in minimal crate")
    }

    pub fn history(&self) -> &[ConversationMessage] {
        &self.history
    }

    pub fn clear_history(&mut self) {
        self.history.clear();
    }

    pub fn set_memory_session_id(&mut self, session_id: Option<String>) {
        self.memory_session_id = session_id;
    }

    /// Add remote tools to the agent's tool registry.
    pub fn add_remote_tools(&mut self, tools: Vec<Box<dyn Tool>>) {
        for tool in tools {
            let name = tool.name().to_string();
            if let Some(pos) = self.tools.iter().position(|t| t.name() == name) {
                self.tools.remove(pos);
                self.tool_specs.remove(pos);
            }
            let spec = tool.spec();
            self.tools.push(tool);
            self.tool_specs.push(spec);
        }
    }

    /// Hydrate the agent with prior chat messages.
    pub fn seed_history(&mut self, messages: &[ChatMessage]) {
        if self.history.is_empty() {
            if let Ok(sys) = self.build_system_prompt() {
                self.history
                    .push(ConversationMessage::Chat(ChatMessage::system(sys)));
            }
        }
        for msg in messages {
            if msg.role != "system" {
                self.history.push(ConversationMessage::Chat(msg.clone()));
            }
        }
    }

    fn trim_history(&mut self) {
        let max = self.config.max_tool_iterations * 4; // reasonable default
        if self.history.len() <= max {
            return;
        }

        let mut system_messages = Vec::new();
        let mut other_messages = Vec::new();

        for msg in self.history.drain(..) {
            match &msg {
                ConversationMessage::Chat(chat) if chat.role == "system" => {
                    system_messages.push(msg);
                }
                _ => other_messages.push(msg),
            }
        }

        if other_messages.len() > max {
            let mut drop_count = other_messages.len() - max;
            while drop_count < other_messages.len()
                && matches!(
                    &other_messages[drop_count],
                    ConversationMessage::ToolResults(_)
                )
            {
                drop_count += 1;
            }
            other_messages.drain(0..drop_count);
        }

        self.history = system_messages;
        self.history.extend(other_messages);
    }

    fn build_system_prompt(&self) -> Result<String> {
        let mut output = String::new();

        // Date/time
        let now = chrono::Local::now();
        let (year, month, day) = (now.year(), now.month(), now.day());
        let (hour, minute, second) = (now.hour(), now.minute(), now.second());
        let tz = now.format("%Z");
        output.push_str(&format!(
            "## CRITICAL CONTEXT: CURRENT DATE & TIME\n\n\
             The following is the ABSOLUTE TRUTH regarding the current date and time. \
             Use this for all relative time calculations.\n\n\
             Date: {year:04}-{month:02}-{day:02}\n\
             Time: {hour:02}:{minute:02}:{second:02} ({tz})\n\n"
        ));

        // Workspace
        output.push_str(&format!(
            "## Workspace\n\nWorking directory: `{}`\n\n",
            self.workspace_dir.display()
        ));

        // Tools
        output.push_str("## Tools\n\n");
        for tool in &self.tools {
            output.push_str(&format!(
                "- **{}**: {}\n  Parameters: `{}`\n",
                tool.name(),
                tool.description(),
                tool.parameters_schema()
            ));
        }

        // Dispatcher instructions
        let instructions = self.tool_dispatcher.prompt_instructions(&self.tools);
        if !instructions.is_empty() {
            output.push_str(&instructions);
            output.push_str("\n\n");
        }

        // Safety
        output.push_str("## Safety\n\n- Do not exfiltrate private data.\n");
        if self.autonomy_level != AutonomyLevel::Full {
            output.push_str(
                "- Do not run destructive commands without asking.\n\
                 - Do not bypass oversight or approval mechanisms.\n",
            );
        }
        output.push_str("- Prefer `trash` over `rm`.\n");

        // Tool honesty
        output.push_str(
            "\n## CRITICAL: Tool Honesty\n\n\
             - NEVER fabricate, invent, or guess tool results.\n\
             - If a tool call fails, report the error — never make up data.\n\
             - When unsure, ask the user rather than guessing.\n",
        );

        Ok(output)
    }

    async fn execute_tool_call(&self, call: &ParsedToolCall) -> ToolExecutionResult {
        let start = Instant::now();

        // Hook: before_tool_call
        let mut tool_name = call.name.clone();
        let mut tool_args = call.arguments.clone();
        if let Some(ref hooks) = self.hook_runner {
            match hooks.run_before_tool_call(tool_name.clone(), tool_args.clone()).await {
                crate::hooks::HookRunnerResult::Continue { name, arguments } => {
                    tool_name = name;
                    tool_args = arguments;
                }
                crate::hooks::HookRunnerResult::Cancel(reason) => {
                    tracing::info!(tool = %call.name, %reason, "tool call cancelled by hook");
                    return ToolExecutionResult {
                        name: call.name.clone(),
                        output: format!("Cancelled by hook: {reason}"),
                        success: false,
                        tool_call_id: call.tool_call_id.clone(),
                    };
                }
            }
        }

        // Execute the tool
        let (result, success) =
            if let Some(tool) = self.tools.iter().find(|t| t.name() == tool_name) {
                match tool.execute(tool_args.clone(), &NoopToolContext).await {
                    Ok(r) => {
                        self.observer.record_event(&ObserverEvent::ToolCall {
                            tool: tool_name.clone(),
                            duration: start.elapsed(),
                            success: r.success,
                        });
                        if r.success {
                            (r.output, true)
                        } else {
                            (format!("Error: {}", r.error.unwrap_or(r.output)), false)
                        }
                    }
                    Err(e) => {
                        self.observer.record_event(&ObserverEvent::ToolCall {
                            tool: tool_name.clone(),
                            duration: start.elapsed(),
                            success: false,
                        });
                        (format!("Error executing {}: {e}", tool_name), false)
                    }
                }
            } else {
                (format!("Unknown tool: {}", tool_name), false)
            };

        let duration = start.elapsed();

        // Hook: after_tool_call
        if let Some(ref hooks) = self.hook_runner {
            let tool_result_obj = ToolResult {
                success,
                output: result.clone(),
                error: None,
            };
            hooks.fire_after_tool_call(&tool_name, &tool_result_obj, duration).await;
        }

        ToolExecutionResult {
            name: tool_name,
            output: result,
            success,
            tool_call_id: call.tool_call_id.clone(),
        }
    }

    async fn execute_tools(&self, calls: &[ParsedToolCall]) -> Vec<ToolExecutionResult> {
        let mut results = Vec::with_capacity(calls.len());
        for call in calls {
            results.push(self.execute_tool_call(call).await);
        }
        results
    }

    fn classify_model(&self, _user_message: &str) -> String {
        // In the minimal agent, no classification — just use the default model.
        self.model_name.clone()
    }

    /// Execute a single agent turn: send message, dispatch tools, return final text.
    pub async fn turn(&mut self, user_message: &str) -> Result<String> {
        if self.history.is_empty() {
            let system_prompt = self.build_system_prompt()?;
            self.history
                .push(ConversationMessage::Chat(ChatMessage::system(system_prompt)));
        }

        // Auto-save user message to memory
        if self.auto_save {
            let _ = self
                .memory
                .store(
                    "user_msg",
                    user_message,
                    MemoryCategory::Conversation,
                    self.memory_session_id.as_deref(),
                )
                .await;
        }

        // Enrich with timestamp
        let now = chrono::Local::now();
        let (year, month, day) = (now.year(), now.month(), now.day());
        let (hour, minute, second) = (now.hour(), now.minute(), now.second());
        let tz = now.format("%Z");
        let date_str =
            format!("{year:04}-{month:02}-{day:02} {hour:02}:{minute:02}:{second:02} {tz}");
        let enriched = format!("[CURRENT DATE & TIME: {date_str}]\n\n{user_message}");

        self.history
            .push(ConversationMessage::Chat(ChatMessage::user(enriched)));

        let effective_model = self.classify_model(user_message);

        for _ in 0..self.config.max_tool_iterations {
            let messages = self.tool_dispatcher.to_provider_messages(&self.history);

            let response = match self
                .provider
                .chat(
                    ChatRequest {
                        messages: &messages,
                        tools: if self.tool_dispatcher.should_send_tool_specs() {
                            Some(&self.tool_specs)
                        } else {
                            None
                        },
                    },
                    &effective_model,
                    Some(self.temperature),
                )
                .await
            {
                Ok(resp) => resp,
                Err(err) => return Err(err),
            };

            let (text, calls) = self.tool_dispatcher.parse_response(&response);
            if calls.is_empty() {
                let final_text = if text.is_empty() {
                    response.text.unwrap_or_default()
                } else {
                    text
                };

                self.history
                    .push(ConversationMessage::Chat(ChatMessage::assistant(
                        final_text.clone(),
                    )));
                self.trim_history();
                return Ok(final_text);
            }

            if !text.is_empty() {
                print!("{text}");
                use std::io::Write;
                let _ = std::io::stdout().lock().flush();
            }

            self.history.push(ConversationMessage::AssistantToolCalls {
                text: response.text.clone(),
                tool_calls: response.tool_calls.clone(),
                reasoning_content: response.reasoning_content.clone(),
            });

            let results = self.execute_tools(&calls).await;
            let formatted = self.tool_dispatcher.format_results(&results);
            self.history.push(formatted);
            self.trim_history();
        }

        anyhow::bail!(
            "Agent exceeded maximum tool iterations ({})",
            self.config.max_tool_iterations
        )
    }

    /// Execute a single agent turn while streaming intermediate events.
    pub async fn turn_streamed(
        &mut self,
        user_message: &str,
        event_tx: tokio::sync::mpsc::Sender<TurnEvent>,
        cancel_token: Option<tokio_util::sync::CancellationToken>,
    ) -> Result<String> {
        if self.history.is_empty() {
            let system_prompt = self.build_system_prompt()?;
            self.history
                .push(ConversationMessage::Chat(ChatMessage::system(system_prompt)));
        }

        if self.auto_save {
            let _ = self
                .memory
                .store(
                    "user_msg",
                    user_message,
                    MemoryCategory::Conversation,
                    self.memory_session_id.as_deref(),
                )
                .await;
        }

        let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S %Z");
        let enriched = format!("[{now}] {user_message}");

        self.history
            .push(ConversationMessage::Chat(ChatMessage::user(enriched)));

        let effective_model = self.classify_model(user_message);

        // Try streaming first, fall back to non-streaming
        use futures_util::StreamExt;

        for _ in 0..self.config.max_tool_iterations {
            if cancel_token
                .as_ref()
                .is_some_and(tokio_util::sync::CancellationToken::is_cancelled)
            {
                return Err(anyhow::anyhow!("tool loop cancelled"));
            }

            let messages = self.tool_dispatcher.to_provider_messages(&self.history);

            // Try streaming
            let stream_opts = clawseed_api::provider::StreamOptions::new(true);
            let mut stream = self.provider.stream_chat(
                ChatRequest {
                    messages: &messages,
                    tools: if self.tool_dispatcher.should_send_tool_specs() {
                        Some(&self.tool_specs)
                    } else {
                        None
                    },
                },
                &effective_model,
                Some(self.temperature),
                stream_opts,
            );

            let mut streamed_text = String::new();
            let mut streamed_tool_calls: Vec<clawseed_api::provider::ToolCall> = Vec::new();
            let mut got_stream = false;

            loop {
                let next_item = stream.next();
                let item = if let Some(ref token) = cancel_token {
                    tokio::select! {
                        biased;
                        () = token.cancelled() => break,
                        item = next_item => item,
                    }
                } else {
                    next_item.await
                };

                let Some(item) = item else { break };
                match item {
                    Ok(event) => match event {
                        clawseed_api::provider::StreamEvent::TextDelta(chunk) => {
                            if let Some(reasoning) = chunk.reasoning {
                                if !reasoning.is_empty() {
                                    let _ = event_tx.send(TurnEvent::Thinking { delta: reasoning }).await;
                                }
                            }
                            if !chunk.delta.is_empty() {
                                got_stream = true;
                                streamed_text.push_str(&chunk.delta);
                                let _ = event_tx.send(TurnEvent::Chunk { delta: chunk.delta }).await;
                            }
                        }
                        clawseed_api::provider::StreamEvent::ToolCall(tc) => {
                            got_stream = true;
                            streamed_tool_calls.push(tc);
                        }
                        clawseed_api::provider::StreamEvent::PreExecutedToolCall { name, args } => {
                            let call_id = uuid::Uuid::new_v4().to_string();
                            let _ = event_tx.send(TurnEvent::ToolCall {
                                id: call_id,
                                name,
                                args: serde_json::from_str(&args).unwrap_or_default(),
                            }).await;
                        }
                        clawseed_api::provider::StreamEvent::PreExecutedToolResult { name, output } => {
                            let result_id = uuid::Uuid::new_v4().to_string();
                            let _ = event_tx.send(TurnEvent::ToolResult {
                                id: result_id,
                                name,
                                output,
                            }).await;
                        }
                        clawseed_api::provider::StreamEvent::Final => break,
                    },
                    Err(_) => break,
                }
            }
            drop(stream);

            let response = if got_stream {
                ChatResponse {
                    text: Some(streamed_text),
                    tool_calls: streamed_tool_calls,
                    usage: None,
                    reasoning_content: None,
                }
            } else {
                // Fall back to non-streaming
                let chat_result = self.provider.chat(
                    ChatRequest {
                        messages: &messages,
                        tools: if self.tool_dispatcher.should_send_tool_specs() {
                            Some(&self.tool_specs)
                        } else {
                            None
                        },
                    },
                    &effective_model,
                    Some(self.temperature),
                );
                match chat_result.await {
                    Ok(resp) => resp,
                    Err(err) => return Err(err),
                }
            };

            let (text, mut calls) = self.tool_dispatcher.parse_response(&response);
            if calls.is_empty() {
                let final_text = if text.is_empty() {
                    response.text.unwrap_or_default()
                } else {
                    text
                };

                if !got_stream && !final_text.is_empty() {
                    let _ = event_tx.send(TurnEvent::Chunk { delta: final_text.clone() }).await;
                }

                self.history
                    .push(ConversationMessage::Chat(ChatMessage::assistant(
                        final_text.clone(),
                    )));
                self.trim_history();
                return Ok(final_text);
            }

            // Assign IDs to tool calls
            for call in &mut calls {
                if call.tool_call_id.is_none() {
                    call.tool_call_id = Some(uuid::Uuid::new_v4().to_string());
                }
            }

            self.history.push(ConversationMessage::AssistantToolCalls {
                text: response.text.clone(),
                tool_calls: response.tool_calls.clone(),
                reasoning_content: response.reasoning_content.clone(),
            });

            for call in &calls {
                let call_id = call.tool_call_id.as_ref().unwrap().clone();
                let _ = event_tx.send(TurnEvent::ToolCall {
                    id: call_id,
                    name: call.name.clone(),
                    args: call.arguments.clone(),
                }).await;
            }

            let results = self.execute_tools(&calls).await;

            for result in &results {
                let result_id = result.tool_call_id.as_ref().unwrap().clone();
                let _ = event_tx.send(TurnEvent::ToolResult {
                    id: result_id,
                    name: result.name.clone(),
                    output: result.output.clone(),
                }).await;
            }

            let formatted = self.tool_dispatcher.format_results(&results);
            self.history.push(formatted);
            self.trim_history();
        }

        anyhow::bail!(
            "Agent exceeded maximum tool iterations ({})",
            self.config.max_tool_iterations
        )
    }

    pub async fn run_single(&mut self, message: &str) -> Result<String> {
        self.turn(message).await
    }
}

/// No-op tool context for tool execution.
struct NoopToolContext;

impl clawseed_api::tool_context::ToolContext for NoopToolContext {
    fn workspace_dir(&self) -> &std::path::Path {
        std::path::Path::new(".")
    }

    fn get_any(&self, _type_id: std::any::TypeId) -> Option<&(dyn std::any::Any + Send + Sync)> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use clawseed_api::provider::ProviderCapabilities;
    use parking_lot::Mutex;

    use crate::dispatcher::{NativeToolDispatcher, XmlToolDispatcher};

    struct MockProvider {
        responses: Mutex<Vec<ChatResponse>>,
    }

    #[async_trait]
    impl Provider for MockProvider {
        async fn chat_with_system(
            &self,
            _system_prompt: Option<&str>,
            _message: &str,
            _model: &str,
            _temperature: Option<f64>,
        ) -> Result<String> {
            Ok("ok".into())
        }

        async fn chat(
            &self,
            _request: ChatRequest<'_>,
            _model: &str,
            _temperature: Option<f64>,
        ) -> Result<ChatResponse> {
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

    struct MockTool;

    #[async_trait]
    impl Tool for MockTool {
        fn name(&self) -> &str { "echo" }
        fn description(&self) -> &str { "echo" }
        fn parameters_schema(&self) -> serde_json::Value { serde_json::json!({"type": "object"}) }

        async fn execute(
            &self,
            _args: serde_json::Value,
            _ctx: &dyn clawseed_api::tool_context::ToolContext,
        ) -> Result<ToolResult> {
            Ok(ToolResult {
                success: true,
                output: "tool-out".into(),
                error: None,
            })
        }
    }

    fn make_memory() -> Arc<dyn Memory> {
        Arc::new(clawseed_memory::none::NoneMemory::new())
    }

    #[tokio::test]
    async fn turn_without_tools_returns_text() {
        let provider = Box::new(MockProvider {
            responses: Mutex::new(vec![ChatResponse {
                text: Some("hello".into()),
                tool_calls: vec![],
                usage: None,
                reasoning_content: None,
            }]),
        });

        let observer: Arc<dyn Observer> = Arc::new(crate::observer::NoopObserver);
        let mut agent = Agent::builder()
            .provider(provider)
            .tools(vec![Box::new(MockTool)])
            .memory(make_memory())
            .observer(observer)
            .tool_dispatcher(Box::new(XmlToolDispatcher))
            .workspace_dir(std::path::PathBuf::from("/tmp"))
            .build()
            .expect("agent builder should succeed");

        let response = agent.turn("hi").await.unwrap();
        assert_eq!(response, "hello");
    }

    #[tokio::test]
    async fn turn_with_native_dispatcher_handles_tool_results_variant() {
        let provider = Box::new(MockProvider {
            responses: Mutex::new(vec![
                ChatResponse {
                    text: Some(String::new()),
                    tool_calls: vec![clawseed_api::provider::ToolCall {
                        id: "tc1".into(),
                        name: "echo".into(),
                        arguments: "{}".into(),
                    }],
                    usage: None,
                    reasoning_content: None,
                },
                ChatResponse {
                    text: Some("done".into()),
                    tool_calls: vec![],
                    usage: None,
                    reasoning_content: None,
                },
            ]),
        });

        let observer: Arc<dyn Observer> = Arc::new(crate::observer::NoopObserver);
        let mut agent = Agent::builder()
            .provider(provider)
            .tools(vec![Box::new(MockTool)])
            .memory(make_memory())
            .observer(observer)
            .tool_dispatcher(Box::new(NativeToolDispatcher))
            .workspace_dir(std::path::PathBuf::from("/tmp"))
            .build()
            .expect("agent builder should succeed");

        let response = agent.turn("hi").await.unwrap();
        assert_eq!(response, "done");
        assert!(
            agent
                .history()
                .iter()
                .any(|msg| matches!(msg, ConversationMessage::ToolResults(_)))
        );
    }

    #[test]
    fn builder_allowed_tools_none_keeps_all_tools() {
        let provider = Box::new(MockProvider {
            responses: Mutex::new(vec![]),
        });

        let observer: Arc<dyn Observer> = Arc::new(crate::observer::NoopObserver);
        let agent = Agent::builder()
            .provider(provider)
            .tools(vec![Box::new(MockTool)])
            .memory(make_memory())
            .observer(observer)
            .tool_dispatcher(Box::new(NativeToolDispatcher))
            .workspace_dir(std::path::PathBuf::from("/tmp"))
            .allowed_tools(None)
            .build()
            .expect("agent builder should succeed");

        assert_eq!(agent.tool_specs.len(), 1);
        assert_eq!(agent.tool_specs[0].name, "echo");
    }

    #[test]
    fn builder_allowed_tools_some_filters_tools() {
        let provider = Box::new(MockProvider {
            responses: Mutex::new(vec![]),
        });

        let observer: Arc<dyn Observer> = Arc::new(crate::observer::NoopObserver);
        let agent = Agent::builder()
            .provider(provider)
            .tools(vec![Box::new(MockTool)])
            .memory(make_memory())
            .observer(observer)
            .tool_dispatcher(Box::new(NativeToolDispatcher))
            .workspace_dir(std::path::PathBuf::from("/tmp"))
            .allowed_tools(Some(vec!["nonexistent".to_string()]))
            .build()
            .expect("agent builder should succeed");

        assert!(agent.tool_specs.is_empty());
    }

    #[test]
    fn add_remote_tools_no_duplicates_on_repeated_calls() {
        struct NamedMockTool { name: String }
        #[async_trait]
        impl Tool for NamedMockTool {
            fn name(&self) -> &str { &self.name }
            fn description(&self) -> &str { "mock" }
            fn parameters_schema(&self) -> serde_json::Value { serde_json::json!({"type": "object"}) }
            async fn execute(&self, _args: serde_json::Value, _ctx: &dyn clawseed_api::tool_context::ToolContext) -> Result<ToolResult> {
                Ok(ToolResult { success: true, output: "ok".into(), error: None })
            }
        }

        let provider = Box::new(MockProvider { responses: Mutex::new(vec![]) });
        let observer: Arc<dyn Observer> = Arc::new(crate::observer::NoopObserver);
        let mut agent = Agent::builder()
            .provider(provider)
            .tools(vec![])
            .memory(make_memory())
            .observer(observer)
            .tool_dispatcher(Box::new(XmlToolDispatcher))
            .workspace_dir(std::path::PathBuf::from("/tmp"))
            .build()
            .expect("agent builder should succeed");

        let make_named = |n: &str| -> Box<dyn Tool> {
            Box::new(NamedMockTool { name: n.to_string() })
        };

        agent.add_remote_tools(vec![make_named("tool_a"), make_named("tool_b")]);
        assert_eq!(agent.tools.len(), 2);
        agent.add_remote_tools(vec![make_named("tool_a"), make_named("tool_b")]);
        assert_eq!(agent.tools.len(), 2);
    }
}
