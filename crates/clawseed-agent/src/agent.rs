//! Agent: a registry that holds tools, hooks, and context providers.
//!
//! The Agent accepts a message via `turn()`, sends it to the provider,
//! parses tool calls, dispatches to registered tools, and loops until done.

use crate::context::AgentToolContext;
use crate::dispatcher::{ParsedToolCall, ToolDispatcher, ToolExecutionResult};
use crate::hooks::HookRunner;
use crate::observer::{Observer, ObserverEvent};
use crate::prompt::{PromptContext, SystemPromptBuilder};
use crate::security::SecurityPolicy;
use crate::tool_registry::DefaultToolRegistry;
use anyhow::Result;
use clawseed_api::memory_traits::{Memory, MemoryCategory};
use clawseed_api::provider::{
    ChatMessage, ChatRequest, ChatResponse, ConversationMessage, Provider,
};
use clawseed_api::tool::{Tool, ToolResult};
use clawseed_api::tool_registry::{ToolRegistry, ToolSource};
use clawseed_config::schema::{AutonomyLevel, IdentityConfig};
use std::sync::Arc;
use std::time::Instant;

/// Streaming events emitted during an agent turn.
#[derive(Debug, Clone)]
pub enum TurnEvent {
    Chunk {
        delta: String,
    },
    Thinking {
        delta: String,
    },
    ToolCall {
        id: String,
        name: String,
        args: serde_json::Value,
    },
    ToolResult {
        id: String,
        name: String,
        output: String,
    },
    DebugPrompt {
        messages_json: String,
        estimated_tokens: usize,
    },
}

/// A tool call resolved through before-hooks, ready for execution.
struct ResolvedToolCall {
    name: String,
    args: serde_json::Value,
    /// Set when a before-hook cancelled this call; `output` contains the reason.
    cancelled: bool,
    output: String,
    tool_call_id: Option<String>,
}

/// The core Agent struct — a registry of tools, hooks, and context providers.
pub struct Agent {
    provider: Arc<dyn Provider>,
    tool_registry: Arc<dyn ToolRegistry>,
    memory: Arc<dyn Memory>,
    observer: Arc<dyn Observer>,
    tool_dispatcher: Box<dyn ToolDispatcher>,
    config: clawseed_config::schema::AgentConfig,
    model_name: String,
    temperature: f64,
    workspace_dir: std::path::PathBuf,
    autonomy_level: AutonomyLevel,
    identity_config: IdentityConfig,
    auto_save: bool,
    auto_recall: bool,
    auto_recall_limit: usize,
    memory_session_id: Option<String>,
    history: Vec<ConversationMessage>,
    hook_runner: Option<Arc<HookRunner>>,
    skill_index: Vec<crate::skills::SkillIndexEntry>,
    active_skills: Vec<crate::skills::ActiveSkill>,
    max_active_skills: usize,
    skills_extra_roots: Vec<String>,
    skills_enabled: bool,
    skills_excluded: Vec<String>,
}

/// Builder for constructing an Agent.
pub struct AgentBuilder {
    provider: Option<Arc<dyn Provider>>,
    tools: Option<Vec<Box<dyn Tool>>>,
    tool_registry: Option<Arc<dyn ToolRegistry>>,
    memory: Option<Arc<dyn Memory>>,
    observer: Option<Arc<dyn Observer>>,
    tool_dispatcher: Option<Box<dyn ToolDispatcher>>,
    config: Option<clawseed_config::schema::AgentConfig>,
    model_name: Option<String>,
    temperature: Option<f64>,
    workspace_dir: Option<std::path::PathBuf>,
    autonomy_level: Option<AutonomyLevel>,
    identity_config: Option<IdentityConfig>,
    auto_save: Option<bool>,
    auto_recall: Option<bool>,
    auto_recall_limit: Option<usize>,
    memory_session_id: Option<String>,
    allowed_tools: Option<Vec<String>>,
    denied_tools: Option<Vec<String>>,
    mcp_tool_filters: Option<std::collections::HashMap<String, Vec<String>>>,
    hook_runner: Option<Arc<HookRunner>>,
    skill_index: Option<Vec<crate::skills::SkillIndexEntry>>,
    max_active_skills: Option<usize>,
    skills_extra_roots: Option<Vec<String>>,
    skills_enabled: Option<bool>,
    skills_excluded: Option<Vec<String>>,
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
            tool_registry: None,
            memory: None,
            observer: None,
            tool_dispatcher: None,
            config: None,
            model_name: None,
            temperature: None,
            workspace_dir: None,
            autonomy_level: None,
            identity_config: None,
            auto_save: None,
            auto_recall: None,
            auto_recall_limit: None,
            memory_session_id: None,
            allowed_tools: None,
            denied_tools: None,
            mcp_tool_filters: None,
            hook_runner: None,
            skill_index: None,
            max_active_skills: None,
            skills_extra_roots: None,
            skills_enabled: None,
            skills_excluded: None,
        }
    }

    pub fn provider(mut self, provider: Box<dyn Provider>) -> Self {
        self.provider = Some(Arc::from(provider));
        self
    }

    pub fn shared_provider(mut self, provider: Arc<dyn Provider>) -> Self {
        self.provider = Some(provider);
        self
    }

    pub fn tools(mut self, tools: Vec<Box<dyn Tool>>) -> Self {
        self.tools = Some(tools);
        self
    }

    /// Provide a pre-built ToolRegistry. If set, `tools()` is ignored.
    pub fn tool_registry(mut self, registry: Arc<dyn ToolRegistry>) -> Self {
        self.tool_registry = Some(registry);
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

    pub fn identity_config(mut self, config: IdentityConfig) -> Self {
        self.identity_config = Some(config);
        self
    }

    pub fn auto_save(mut self, auto_save: bool) -> Self {
        self.auto_save = Some(auto_save);
        self
    }

    pub fn auto_recall(mut self, auto_recall: bool) -> Self {
        self.auto_recall = Some(auto_recall);
        self
    }

    pub fn auto_recall_limit(mut self, limit: usize) -> Self {
        self.auto_recall_limit = Some(limit);
        self
    }

    pub fn memory_session_id(mut self, session_id: Option<String>) -> Self {
        self.memory_session_id = session_id;
        self
    }

    pub fn allowed_tools(mut self, allowed_tools: Option<Vec<String>>) -> Self {
        self.allowed_tools = allowed_tools;
        self
    }

    pub fn denied_tools(mut self, denied_tools: Option<Vec<String>>) -> Self {
        self.denied_tools = denied_tools;
        self
    }

    pub fn mcp_tool_filters(
        mut self,
        filters: Option<std::collections::HashMap<String, Vec<String>>>,
    ) -> Self {
        self.mcp_tool_filters = filters;
        self
    }

    pub fn hook_runner(mut self, runner: Option<Arc<HookRunner>>) -> Self {
        self.hook_runner = runner;
        self
    }

    pub fn skill_index(mut self, index: Vec<crate::skills::SkillIndexEntry>) -> Self {
        self.skill_index = Some(index);
        self
    }

    pub fn max_active_skills(mut self, max: usize) -> Self {
        self.max_active_skills = Some(max);
        self
    }

    pub fn skills_extra_roots(mut self, roots: Vec<String>) -> Self {
        self.skills_extra_roots = Some(roots);
        self
    }

    pub fn skills_enabled(mut self, enabled: bool) -> Self {
        self.skills_enabled = Some(enabled);
        self
    }

    pub fn skills_excluded(mut self, excluded: Vec<String>) -> Self {
        self.skills_excluded = Some(excluded);
        self
    }

    pub fn build(self) -> Result<Agent> {
        // Build the tool registry: prefer pre-built registry, otherwise create from tools
        let registry: Arc<dyn ToolRegistry> = if let Some(reg) = self.tool_registry {
            reg
        } else {
            let tools = self
                .tools
                .ok_or_else(|| anyhow::anyhow!("tools are required"))?;

            let allowed = self.allowed_tools.unwrap_or_default();
            let denied = self.denied_tools.unwrap_or_default();
            let mcp_filters = self.mcp_tool_filters.unwrap_or_default();
            let reg = DefaultToolRegistry::with_filters(allowed, denied, mcp_filters);
            for tool in tools {
                reg.register(tool, ToolSource::BuiltIn);
            }
            Arc::new(reg)
        };

        Ok(Agent {
            provider: self
                .provider
                .ok_or_else(|| anyhow::anyhow!("provider is required"))?,
            tool_registry: registry,
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
            identity_config: self.identity_config.unwrap_or_default(),
            auto_save: self.auto_save.unwrap_or(false),
            auto_recall: self.auto_recall.unwrap_or(true),
            auto_recall_limit: self.auto_recall_limit.unwrap_or(5),
            memory_session_id: self.memory_session_id,
            history: Vec::new(),
            hook_runner: self.hook_runner,
            skill_index: self.skill_index.unwrap_or_default(),
            active_skills: Vec::new(),
            max_active_skills: self.max_active_skills.unwrap_or(5),
            skills_extra_roots: self.skills_extra_roots.unwrap_or_default(),
            skills_enabled: self.skills_enabled.unwrap_or(true),
            skills_excluded: self.skills_excluded.unwrap_or_default(),
        })
    }
}

impl Agent {
    pub fn builder() -> AgentBuilder {
        AgentBuilder::new()
    }

    /// Build an agent from the full config.
    pub async fn from_config(config: &clawseed_config::schema::Config) -> anyhow::Result<Self> {
        Self::from_config_with_registry(config, None).await
    }

    /// Build an agent from the full config with an optional provider factory registry.
    ///
    /// When a custom registry is provided, it is used instead of the default
    /// built-in registry for provider construction. Useful for Android/embedded
    /// use cases with minimal provider sets.
    pub async fn from_config_with_registry(
        config: &clawseed_config::schema::Config,
        provider_factory_registry: Option<
            Arc<clawseed_providers::factory::ProviderFactoryRegistry>,
        >,
    ) -> anyhow::Result<Self> {
        let fallback = config.providers.fallback_provider();

        // Provider — use custom registry if available
        let provider: Arc<dyn Provider> = if let Some(ref registry) = provider_factory_registry {
            clawseed_providers::create_resilient_provider_with_registry(
                registry,
                config.providers.fallback.as_deref().unwrap_or("openrouter"),
                fallback.and_then(|e| e.api_key.as_deref()),
                fallback.and_then(|e| e.base_url.as_deref()),
                &config.reliability,
                &clawseed_providers::provider_runtime_options_from_config(config),
            )?
            .into()
        } else {
            clawseed_providers::create_resilient_provider_with_options(
                config.providers.fallback.as_deref().unwrap_or("openrouter"),
                fallback.and_then(|e| e.api_key.as_deref()),
                fallback.and_then(|e| e.base_url.as_deref()),
                &config.reliability,
                &clawseed_providers::provider_runtime_options_from_config(config),
            )?
            .into()
        };

        // Memory
        let mem = clawseed_memory::create_memory_with_storage_and_routes(
            &config.memory,
            &config.providers,
            Some(&config.storage),
            &config.workspace_dir,
            fallback.and_then(|e| e.api_key.as_deref()),
        )
        .await?;

        // Observer
        let observer: Arc<dyn Observer> = Arc::new(crate::observer::NoopObserver);

        // Model and temperature from fallback provider config
        let model_name = fallback
            .and_then(|e| e.model.clone())
            .unwrap_or_else(|| "anthropic/claude-sonnet-4".into());
        let temperature = fallback.and_then(|e| e.temperature).unwrap_or(0.7);

        Self::build_from_config(
            config,
            provider,
            mem,
            observer,
            model_name,
            temperature,
            None,
        )
    }

    /// Build an agent from config, reusing externally-provided shared components.
    ///
    /// Unlike `from_config()` which creates its own provider/memory/observer,
    /// this method accepts pre-built instances — typically shared across
    /// gateway WebSocket connections.
    ///
    /// model_name and temperature are also taken from the shared bundle
    /// (state.model / state.temperature), not re-read from config, to avoid
    /// provider-config skew (e.g., old provider + new model after a config update).
    pub async fn from_config_with_shared_components(
        config: &clawseed_config::schema::Config,
        provider: Arc<dyn Provider>,
        memory: Arc<dyn Memory>,
        observer: Arc<dyn Observer>,
        model_name: String,
        temperature: f64,
        shared_builtin_tools: Option<Arc<[Arc<dyn clawseed_api::tool::Tool>]>>,
    ) -> anyhow::Result<Self> {
        Self::build_from_config(
            config,
            provider,
            memory,
            observer,
            model_name,
            temperature,
            shared_builtin_tools,
        )
    }

    /// Private: shared assembly logic for both public constructors.
    fn build_from_config(
        config: &clawseed_config::schema::Config,
        provider: Arc<dyn Provider>,
        memory: Arc<dyn Memory>,
        observer: Arc<dyn Observer>,
        model_name: String,
        temperature: f64,
        shared_builtin_tools: Option<Arc<[Arc<dyn clawseed_api::tool::Tool>]>>,
    ) -> anyhow::Result<Self> {
        // Dispatcher: native if provider supports it, otherwise XML
        let dispatcher: Box<dyn ToolDispatcher> = if provider.supports_native_tools() {
            Box::new(crate::dispatcher::NativeToolDispatcher)
        } else {
            Box::new(crate::dispatcher::XmlToolDispatcher)
        };

        // Hook runner: SecurityPolicy is always the first hook
        let mut hook_runner = HookRunner::new();
        hook_runner.register(Box::new(SecurityPolicy::from_config(
            &config.autonomy,
            &config.workspace_dir,
        )));

        // Process declarative hook chain from config
        if config.hooks.enabled || !config.hooks.chain.is_empty() {
            let mut factory_reg = crate::hooks::HookFactoryRegistry::new();
            factory_reg.register(Box::new(crate::hooks::SecurityPolicyHookFactory));
            for decl in &config.hooks.chain {
                if let Some(hook) = factory_reg.create_hook(&decl.hook_type, &decl.config) {
                    hook_runner.register(hook);
                } else {
                    tracing::warn!(hook_type = %decl.hook_type, "Unknown hook type in config, skipping");
                }
            }
        }

        // Determine tool filtering from agent config
        let allowed = if config.agent.allowed_tools.is_empty() {
            None
        } else {
            Some(config.agent.allowed_tools.clone())
        };
        let denied = if config.agent.denied_tools.is_empty() {
            None
        } else {
            Some(config.agent.denied_tools.clone())
        };
        let mcp_filters = if config.agent.mcp_tool_filters.is_empty() {
            None
        } else {
            Some(config.agent.mcp_tool_filters.clone())
        };

        // Build the tool registry — reuse shared Arc instances when available
        let registry: Arc<dyn ToolRegistry> = if let Some(ref shared) = shared_builtin_tools {
            let reg = DefaultToolRegistry::with_filters(
                allowed.unwrap_or_default(),
                denied.unwrap_or_default(),
                mcp_filters.unwrap_or_default(),
            );
            reg.register_all_arc(shared.to_vec(), ToolSource::BuiltIn);
            Arc::new(reg)
        } else {
            let tools = clawseed_tools::registry::all_tools(
                config.workspace_dir.clone(),
                config,
                memory.clone(),
            );
            let reg = DefaultToolRegistry::with_filters(
                allowed.unwrap_or_default(),
                denied.unwrap_or_default(),
                mcp_filters.unwrap_or_default(),
            );
            for tool in tools {
                reg.register(tool, ToolSource::BuiltIn);
            }
            Arc::new(reg)
        };

        // Load skill index
        let extra_roots: Vec<String> = config.skills.extra_roots.clone();
        let skill_index = if config.skills.enabled {
            crate::skills::load_skill_index_with_roots(&config.workspace_dir, &extra_roots)
                .into_iter()
                .filter(|e| !config.skills.excluded.contains(&e.name))
                .collect()
        } else {
            Vec::new()
        };

        let mut builder = Agent::builder()
            .shared_provider(provider)
            .tool_registry(registry)
            .memory(memory)
            .observer(observer)
            .tool_dispatcher(dispatcher)
            .model_name(model_name)
            .temperature(temperature)
            .workspace_dir(config.workspace_dir.clone())
            .autonomy_level(config.autonomy.level)
            .identity_config(config.identity.clone())
            .auto_save(config.memory.auto_save)
            .auto_recall(config.memory.auto_recall)
            .auto_recall_limit(config.memory.auto_recall_limit)
            .hook_runner(Some(Arc::new(hook_runner)))
            .skill_index(skill_index)
            .max_active_skills(config.skills.max_active)
            .skills_extra_roots(extra_roots)
            .skills_enabled(config.skills.enabled)
            .skills_excluded(config.skills.excluded.clone());

        if let Some(ref session_id) = config.memory.namespace {
            builder = builder.memory_session_id(Some(session_id.clone()));
        }

        builder.build()
    }

    pub fn history(&self) -> &[ConversationMessage] {
        &self.history
    }

    pub fn clear_history(&mut self) {
        self.history.clear();
    }

    /// Remove the last assistant turn and the preceding user message from history.
    /// Returns the original user message content (without timestamp prefix) if found,
    /// so the caller can re-run the turn.
    pub fn remove_last_assistant_turn(&mut self) -> Option<String> {
        let last_user_idx = self
            .history
            .iter()
            .rposition(|m| matches!(m, ConversationMessage::Chat(chat) if chat.role == "user"))?;
        let user_content = match &self.history[last_user_idx] {
            ConversationMessage::Chat(chat) => chat.content.clone(),
            _ => return None,
        };
        // Remove the user message and everything after it
        self.history.truncate(last_user_idx);
        // Strip the timestamp prefix that prepare_turn adds: "[YYYY-MM-DD HH:MM:SS TZ] actual message"
        let stripped = user_content
            .strip_prefix('[')
            .and_then(|s| s.find("] "))
            .map(|idx| user_content[idx + 2..].to_string())
            .unwrap_or(user_content);
        Some(stripped)
    }

    pub fn set_memory_session_id(&mut self, session_id: Option<String>) {
        self.memory_session_id = session_id;
    }

    /// Activate a skill by name.
    pub fn activate_skill(&mut self, name: &str) -> Result<String> {
        // Check if skill system is enabled
        if !self.skills_enabled {
            return Err(anyhow::anyhow!(
                "Skill system is disabled. Enable it in config to use skills."
            ));
        }

        // Check if skill is excluded
        if self.skills_excluded.contains(&name.to_string()) {
            return Err(anyhow::anyhow!(
                "Skill '{}' is disabled and cannot be activated.",
                name
            ));
        }

        // Check if skill is in the index (i.e. actually discoverable)
        if !self.skill_index.iter().any(|e| e.name == name) {
            return Err(anyhow::anyhow!(
                "Skill '{}' not found in available skills.",
                name
            ));
        }

        // Check if already active
        if self.active_skills.iter().any(|s| s.skill.name == name) {
            return Ok(format!(
                "Skill '{}' is already active. Its instructions are in your system prompt.",
                name
            ));
        }

        // Load the full skill
        let skill = crate::skills::load_skill_by_name_with_roots(
            name,
            &self.workspace_dir,
            &self.skills_extra_roots,
        )
        .map_err(|e| anyhow::anyhow!("Failed to load skill '{}': {}", name, e))?;

        // Permission check
        let tool_names = self.tool_registry.tool_names();
        crate::skills::check_permissions(&skill, &tool_names)
            .map_err(|e| anyhow::anyhow!("{}", e))?;

        // Check max active
        if self.active_skills.len() >= self.max_active_skills {
            return Err(anyhow::anyhow!(
                "Maximum number of active skills ({}) reached. Deactivate a skill first.",
                self.max_active_skills
            ));
        }

        // Push to active_skills
        self.active_skills
            .push(crate::skills::ActiveSkill { skill });

        // Rebuild system prompt
        self.rebuild_system_prompt()?;

        Ok(format!(
            "Skill '{}' activated. Full instructions have been added to your system prompt.",
            name
        ))
    }

    /// Deactivate a skill by name.
    pub fn deactivate_skill(&mut self, name: &str) -> Result<String> {
        let idx = self
            .active_skills
            .iter()
            .position(|s| s.skill.name == name)
            .ok_or_else(|| anyhow::anyhow!("Skill '{}' is not active.", name))?;

        self.active_skills.remove(idx);
        self.rebuild_system_prompt()?;

        Ok(format!(
            "Skill '{}' deactivated. Its instructions have been removed from your system prompt.",
            name
        ))
    }

    /// Rebuild the system prompt and replace the system message in history.
    fn rebuild_system_prompt(&mut self) -> Result<()> {
        let new_prompt = self.build_system_prompt()?;

        for msg in &mut self.history {
            if let ConversationMessage::Chat(chat) = msg
                && chat.role == "system"
            {
                chat.content = new_prompt;
                return Ok(());
            }
        }

        // No system message found — prepend one
        self.history.insert(
            0,
            ConversationMessage::Chat(ChatMessage::system(new_prompt)),
        );
        Ok(())
    }

    /// Handle Skill tool calls: activate or deactivate skills.
    ///
    /// Reads action/skill_name from the original tool call arguments (not from
    /// the tool output), performs activation/deactivation, and updates the
    /// result output with the final semantic message. This way no sentinel
    /// JSON ever appears in observer events, hooks, or history.
    fn handle_skill_tool_results(
        &mut self,
        calls: &[ParsedToolCall],
        results: &mut [crate::dispatcher::ToolExecutionResult],
    ) {
        for (i, call) in calls.iter().enumerate() {
            if call.name != "Skill" || i >= results.len() {
                continue;
            }

            let result = &mut results[i];
            if !result.success {
                continue;
            }

            let skill_name = match call.arguments.get("skill").and_then(|v| v.as_str()) {
                Some(name) if !name.is_empty() => name,
                _ => continue,
            };

            let action = call
                .arguments
                .get("action")
                .and_then(|v| v.as_str())
                .unwrap_or("activate");

            let new_output = match action {
                "activate" => match self.activate_skill(skill_name) {
                    Ok(msg) => msg,
                    Err(e) => {
                        result.success = false;
                        format!("Failed to activate skill '{}': {}", skill_name, e)
                    }
                },
                "deactivate" => match self.deactivate_skill(skill_name) {
                    Ok(msg) => msg,
                    Err(e) => {
                        result.success = false;
                        format!("Failed to deactivate skill '{}': {}", skill_name, e)
                    }
                },
                _ => {
                    result.success = false;
                    format!(
                        "Unknown skill action '{}'. Use 'activate' or 'deactivate'.",
                        action
                    )
                }
            };

            result.output = new_output;
        }
    }

    /// Add remote tools to the agent's tool registry.
    pub fn add_remote_tools(&mut self, tools: Vec<Box<dyn Tool>>, session: String) {
        for tool in tools {
            self.tool_registry.register_or_replace(
                tool,
                ToolSource::Remote {
                    session: session.clone(),
                },
            );
        }
    }

    /// Hydrate the agent with prior chat messages.
    pub fn seed_history(&mut self, messages: &[ChatMessage]) {
        if self.history.is_empty()
            && let Ok(sys) = self.build_system_prompt()
        {
            self.history
                .push(ConversationMessage::Chat(ChatMessage::system(sys)));
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
        let specs = self.tool_registry.tool_specs();
        let instructions = self.tool_dispatcher.prompt_instructions(&specs);

        let ctx = PromptContext {
            workspace_dir: &self.workspace_dir,
            model_name: &self.model_name,
            tool_specs: &specs,
            dispatcher_instructions: &instructions,
            identity_config: &self.identity_config,
            autonomy_level: self.autonomy_level,
            skill_index: &self.skill_index,
            active_skills: &self.active_skills,
        };

        SystemPromptBuilder::with_defaults().build(&ctx)
    }

    /// Build the tool context for a single tool execution.
    fn build_tool_context(&self) -> AgentToolContext {
        AgentToolContext::new(self.workspace_dir.clone())
    }

    async fn execute_tool_call(&self, call: &ParsedToolCall) -> ToolExecutionResult {
        let start = Instant::now();

        // Hook: before_tool_call
        let mut tool_name = call.name.clone();
        let mut tool_args = call.arguments.clone();
        if let Some(ref hooks) = self.hook_runner {
            match hooks
                .run_before_tool_call(tool_name.clone(), tool_args.clone())
                .await
            {
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
        let ctx = self.build_tool_context();
        let (result, success) = if let Some(tool) = self.tool_registry.get_tool(&tool_name) {
            match tool.execute(tool_args.clone(), &ctx).await {
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
            hooks
                .fire_after_tool_call(&tool_name, &tool_result_obj, duration)
                .await;
        }

        ToolExecutionResult {
            name: tool_name,
            output: result,
            success,
            tool_call_id: call.tool_call_id.clone(),
        }
    }

    async fn execute_tools(&self, calls: &[ParsedToolCall]) -> Vec<ToolExecutionResult> {
        if calls.len() <= 1 {
            let mut results = Vec::with_capacity(calls.len());
            for call in calls {
                results.push(self.execute_tool_call(call).await);
            }
            return results;
        }

        // Multiple tool calls: run before-hooks serially, then execute in parallel
        let resolved = self.resolve_before_hooks(calls).await;
        let futures: Vec<_> = resolved
            .iter()
            .map(|r| self.execute_resolved_tool(r))
            .collect();
        futures_util::future::join_all(futures).await
    }

    /// Resolve before-hooks for all tool calls (serially), returning the
    /// resolved names/args and whether each was cancelled.
    async fn resolve_before_hooks(&self, calls: &[ParsedToolCall]) -> Vec<ResolvedToolCall> {
        let mut resolved = Vec::with_capacity(calls.len());
        for call in calls {
            let mut tool_name = call.name.clone();
            let mut tool_args = call.arguments.clone();

            if let Some(ref hooks) = self.hook_runner {
                match hooks
                    .run_before_tool_call(tool_name.clone(), tool_args.clone())
                    .await
                {
                    crate::hooks::HookRunnerResult::Continue { name, arguments } => {
                        tool_name = name;
                        tool_args = arguments;
                    }
                    crate::hooks::HookRunnerResult::Cancel(reason) => {
                        tracing::info!(tool = %call.name, %reason, "tool call cancelled by hook");
                        resolved.push(ResolvedToolCall {
                            name: call.name.clone(),
                            args: serde_json::Value::Null,
                            cancelled: true,
                            output: format!("Cancelled by hook: {reason}"),
                            tool_call_id: call.tool_call_id.clone(),
                        });
                        continue;
                    }
                }
            }

            resolved.push(ResolvedToolCall {
                name: tool_name,
                args: tool_args,
                cancelled: false,
                output: String::new(),
                tool_call_id: call.tool_call_id.clone(),
            });
        }
        resolved
    }

    /// Execute a single resolved tool call (no before-hooks, they already ran).
    async fn execute_resolved_tool(&self, resolved: &ResolvedToolCall) -> ToolExecutionResult {
        if resolved.cancelled {
            return ToolExecutionResult {
                name: resolved.name.clone(),
                output: resolved.output.clone(),
                success: false,
                tool_call_id: resolved.tool_call_id.clone(),
            };
        }

        let start = Instant::now();
        let ctx = self.build_tool_context();
        let (result, success) = if let Some(tool) = self.tool_registry.get_tool(&resolved.name) {
            match tool.execute(resolved.args.clone(), &ctx).await {
                Ok(r) => {
                    self.observer.record_event(&ObserverEvent::ToolCall {
                        tool: resolved.name.clone(),
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
                        tool: resolved.name.clone(),
                        duration: start.elapsed(),
                        success: false,
                    });
                    (format!("Error executing {}: {e}", resolved.name), false)
                }
            }
        } else {
            (format!("Unknown tool: {}", resolved.name), false)
        };

        let duration = start.elapsed();

        // Hook: after_tool_call
        if let Some(ref hooks) = self.hook_runner {
            let tool_result_obj = ToolResult {
                success,
                output: result.clone(),
                error: None,
            };
            hooks
                .fire_after_tool_call(&resolved.name, &tool_result_obj, duration)
                .await;
        }

        ToolExecutionResult {
            name: resolved.name.clone(),
            output: result,
            success,
            tool_call_id: resolved.tool_call_id.clone(),
        }
    }

    /// Prepare for a turn: add system prompt if needed, auto-save, enrich with timestamp.
    fn prepare_turn(&mut self, user_message: &str) -> Result<()> {
        if self.history.is_empty() {
            let system_prompt = self.build_system_prompt()?;
            self.history
                .push(ConversationMessage::Chat(ChatMessage::system(
                    system_prompt,
                )));
        }

        // Note: auto_save is async but we intentionally fire-and-forget here
        // to avoid making prepare_turn async (the caller already awaits it).
        if self.auto_save {
            // Will be awaited in the caller context
        }

        let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S %Z");
        let enriched = format!("[{now}] {user_message}");

        self.history
            .push(ConversationMessage::Chat(ChatMessage::user(enriched)));

        Ok(())
    }

    /// Execute tool calls, handle skill activations, format results, and append to history.
    async fn process_tool_calls(&mut self, calls: &[ParsedToolCall]) -> Vec<ToolExecutionResult> {
        let mut results = self.execute_tools(calls).await;
        self.handle_skill_tool_results(calls, &mut results);
        let formatted = self.tool_dispatcher.format_results(&results);
        self.history.push(formatted);
        self.trim_history();
        results
    }

    /// Execute a single agent turn: send message, dispatch tools, return final text.
    pub async fn turn(&mut self, user_message: &str) -> Result<String> {
        self.prepare_turn(user_message)?;
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

        // Auto-recall relevant memories and prepend context to user message.
        if self.auto_recall
            && self.memory.name() != "none"
            && let Ok(entries) = self
                .memory
                .recall(user_message, self.auto_recall_limit, None, None, None)
                .await
        {
            let ctx: String = entries
                .iter()
                .filter(|e| !matches!(e.category, MemoryCategory::Conversation))
                .map(|e| format!("- {}: {}", e.key, e.content))
                .collect::<Vec<_>>()
                .join("\n");
            if !ctx.is_empty() {
                let memory_prefix = format!("[Memory context]\n{ctx}\n[/Memory context]\n\n");
                if let Some(ConversationMessage::Chat(msg)) = self.history.last_mut() {
                    msg.content = format!("{memory_prefix}{}", msg.content);
                }
            }
        }

        let effective_model = self.model_name.clone();

        let mut auto_continue_count: usize = 0;

        for _ in 0..self.config.max_tool_iterations {
            let messages = self.tool_dispatcher.to_provider_messages(&self.history);

            let tool_specs = self.tool_registry.tool_specs();
            let response = match self
                .provider
                .chat(
                    ChatRequest {
                        messages: &messages,
                        tools: if self.tool_dispatcher.should_send_tool_specs() {
                            Some(&tool_specs)
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

                // Auto-continue when truncated due to max_tokens
                if response.stop_reason == clawseed_api::provider::StopReason::MaxTokens
                    && self.config.auto_continue_on_truncation
                    && auto_continue_count < self.config.max_auto_continue
                {
                    auto_continue_count += 1;
                    tracing::warn!(
                        auto_continue = auto_continue_count,
                        max = self.config.max_auto_continue,
                        "Response truncated by max_tokens, auto-continuing"
                    );
                    print!("\n[⚠ 输出被截断，自动续接中...]\n");
                    use std::io::Write;
                    let _ = std::io::stdout().lock().flush();
                    self.history
                        .push(ConversationMessage::Chat(ChatMessage::user(
                            "请继续输出，不要重复已输出的内容",
                        )));
                    continue;
                }

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

            let _results = self.process_tool_calls(&calls).await;
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
        debug: bool,
    ) -> Result<String> {
        self.prepare_turn(user_message)?;

        // Auto-recall relevant memories and prepend context to user message.
        if self.auto_recall
            && self.memory.name() != "none"
            && let Ok(entries) = self
                .memory
                .recall(user_message, self.auto_recall_limit, None, None, None)
                .await
        {
            let ctx: String = entries
                .iter()
                .filter(|e| !matches!(e.category, MemoryCategory::Conversation))
                .map(|e| format!("- {}: {}", e.key, e.content))
                .collect::<Vec<_>>()
                .join("\n");
            if !ctx.is_empty() {
                let memory_prefix = format!("[Memory context]\n{ctx}\n[/Memory context]\n\n");
                if let Some(ConversationMessage::Chat(msg)) = self.history.last_mut() {
                    msg.content = format!("{memory_prefix}{}", msg.content);
                }
            }
        }

        let effective_model = self.model_name.clone();

        // Try streaming first, fall back to non-streaming
        use futures_util::StreamExt;

        let mut auto_continue_count: usize = 0;

        for iteration in 0..self.config.max_tool_iterations {
            if cancel_token
                .as_ref()
                .is_some_and(tokio_util::sync::CancellationToken::is_cancelled)
            {
                return Err(anyhow::anyhow!("ToolLoopCancelled"));
            }

            let messages = self.tool_dispatcher.to_provider_messages(&self.history);

            if debug && iteration == 0 {
                let messages_json = serde_json::to_string(&messages).unwrap_or_default();
                let estimated_tokens = crate::history::estimate_history_tokens(&messages);
                let _ = event_tx
                    .send(TurnEvent::DebugPrompt {
                        messages_json,
                        estimated_tokens,
                    })
                    .await;
            }

            // Try streaming
            let stream_opts = clawseed_api::provider::StreamOptions::new(true);
            let tool_specs = self.tool_registry.tool_specs();
            let mut stream = self.provider.stream_chat(
                ChatRequest {
                    messages: &messages,
                    tools: if self.tool_dispatcher.should_send_tool_specs() {
                        Some(&tool_specs)
                    } else {
                        None
                    },
                },
                &effective_model,
                Some(self.temperature),
                stream_opts,
            );

            let mut streamed_text = String::new();
            let mut streamed_reasoning = String::new();
            let mut streamed_tool_calls: Vec<clawseed_api::provider::ToolCall> = Vec::new();
            let mut streamed_stop_reason = clawseed_api::provider::StopReason::EndTurn;
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
                            if let Some(reasoning) = chunk.reasoning
                                && !reasoning.is_empty()
                            {
                                streamed_reasoning.push_str(&reasoning);
                                let _ = event_tx
                                    .send(TurnEvent::Thinking { delta: reasoning })
                                    .await;
                            }
                            if !chunk.delta.is_empty() {
                                got_stream = true;
                                streamed_text.push_str(&chunk.delta);
                                let _ =
                                    event_tx.send(TurnEvent::Chunk { delta: chunk.delta }).await;
                            }
                        }
                        clawseed_api::provider::StreamEvent::ToolCall(tc) => {
                            got_stream = true;
                            streamed_tool_calls.push(tc);
                        }
                        clawseed_api::provider::StreamEvent::PreExecutedToolCall { name, args } => {
                            let call_id = uuid::Uuid::new_v4().to_string();
                            let _ = event_tx
                                .send(TurnEvent::ToolCall {
                                    id: call_id,
                                    name,
                                    args: serde_json::from_str(&args).unwrap_or_default(),
                                })
                                .await;
                        }
                        clawseed_api::provider::StreamEvent::PreExecutedToolResult {
                            name,
                            output,
                        } => {
                            let result_id = uuid::Uuid::new_v4().to_string();
                            let _ = event_tx
                                .send(TurnEvent::ToolResult {
                                    id: result_id,
                                    name,
                                    output,
                                })
                                .await;
                        }
                        clawseed_api::provider::StreamEvent::Final { stop_reason } => {
                            streamed_stop_reason = stop_reason;
                            break;
                        }
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
                    reasoning_content: if streamed_reasoning.is_empty() {
                        None
                    } else {
                        Some(streamed_reasoning)
                    },
                    stop_reason: streamed_stop_reason,
                }
            } else {
                // Fall back to non-streaming
                let tool_specs = self.tool_registry.tool_specs();
                let chat_result = self.provider.chat(
                    ChatRequest {
                        messages: &messages,
                        tools: if self.tool_dispatcher.should_send_tool_specs() {
                            Some(&tool_specs)
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
                    let _ = event_tx
                        .send(TurnEvent::Chunk {
                            delta: final_text.clone(),
                        })
                        .await;
                }

                self.history
                    .push(ConversationMessage::Chat(ChatMessage::assistant(
                        final_text.clone(),
                    )));
                self.trim_history();

                // Auto-continue when truncated due to max_tokens
                if response.stop_reason == clawseed_api::provider::StopReason::MaxTokens
                    && self.config.auto_continue_on_truncation
                    && auto_continue_count < self.config.max_auto_continue
                {
                    auto_continue_count += 1;
                    tracing::warn!(
                        auto_continue = auto_continue_count,
                        max = self.config.max_auto_continue,
                        "Response truncated by max_tokens, auto-continuing"
                    );
                    let _ = event_tx
                        .send(TurnEvent::Chunk {
                            delta: "\n[⚠ 输出被截断，自动续接中...]\n".to_string(),
                        })
                        .await;
                    self.history
                        .push(ConversationMessage::Chat(ChatMessage::user(
                            "请继续输出，不要重复已输出的内容",
                        )));
                    continue;
                }

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
                let _ = event_tx
                    .send(TurnEvent::ToolCall {
                        id: call_id,
                        name: call.name.clone(),
                        args: call.arguments.clone(),
                    })
                    .await;
            }

            let mut results = self.execute_tools(&calls).await;

            // Handle skill activations BEFORE emitting events or formatting to history.
            self.handle_skill_tool_results(&calls, &mut results);

            for result in &results {
                let result_id = result.tool_call_id.as_ref().unwrap().clone();
                let _ = event_tx
                    .send(TurnEvent::ToolResult {
                        id: result_id,
                        name: result.name.clone(),
                        output: result.output.clone(),
                    })
                    .await;
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

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
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
                    stop_reason: clawseed_api::provider::StopReason::EndTurn,
                });
            }
            Ok(guard.remove(0))
        }
    }

    struct MockTool;

    #[async_trait]
    impl Tool for MockTool {
        fn name(&self) -> &str {
            "echo"
        }
        fn description(&self) -> &str {
            "echo"
        }
        fn parameters_schema(&self) -> serde_json::Value {
            serde_json::json!({"type": "object"})
        }

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
                stop_reason: clawseed_api::provider::StopReason::EndTurn,
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
                    stop_reason: clawseed_api::provider::StopReason::EndTurn,
                },
                ChatResponse {
                    text: Some("done".into()),
                    tool_calls: vec![],
                    usage: None,
                    reasoning_content: None,
                    stop_reason: clawseed_api::provider::StopReason::EndTurn,
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

        assert_eq!(agent.tool_registry.tool_specs().len(), 1);
        assert_eq!(agent.tool_registry.tool_specs()[0].name, "echo");
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

        assert!(agent.tool_registry.tool_specs().is_empty());
    }

    #[test]
    fn add_remote_tools_no_duplicates_on_repeated_calls() {
        struct NamedMockTool {
            name: String,
        }
        #[async_trait]
        impl Tool for NamedMockTool {
            fn name(&self) -> &str {
                &self.name
            }
            fn description(&self) -> &str {
                "mock"
            }
            fn parameters_schema(&self) -> serde_json::Value {
                serde_json::json!({"type": "object"})
            }
            async fn execute(
                &self,
                _args: serde_json::Value,
                _ctx: &dyn clawseed_api::tool_context::ToolContext,
            ) -> Result<ToolResult> {
                Ok(ToolResult {
                    success: true,
                    output: "ok".into(),
                    error: None,
                })
            }
        }

        let provider = Box::new(MockProvider {
            responses: Mutex::new(vec![]),
        });
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
            Box::new(NamedMockTool {
                name: n.to_string(),
            })
        };

        agent.add_remote_tools(
            vec![make_named("tool_a"), make_named("tool_b")],
            "s1".to_string(),
        );
        assert_eq!(agent.tool_registry.len(), 2);
        agent.add_remote_tools(
            vec![make_named("tool_a"), make_named("tool_b")],
            "s1".to_string(),
        );
        assert_eq!(agent.tool_registry.len(), 2);
    }

    #[tokio::test]
    async fn skill_activation_updates_history_not_sentinel() {
        // Create a skill directory with manifest.toml + SKILL.md
        let dir = tempfile::tempdir().unwrap();
        let skill_dir = dir
            .path()
            .join(".clawseed")
            .join("skills")
            .join("test-skill");
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(
            skill_dir.join("manifest.toml"),
            r#"[skill]
name = "test-skill"
description = "A test skill"
"#,
        )
        .unwrap();
        std::fs::write(skill_dir.join("SKILL.md"), "# Test Skill\nDo the thing.\n").unwrap();

        // Load the skill index
        let skill_index = crate::skills::load_skill_index(dir.path());

        // Provider returns a tool call for Skill, then a final response
        let provider = Box::new(MockProvider {
            responses: Mutex::new(vec![
                ChatResponse {
                    text: Some(String::new()),
                    tool_calls: vec![clawseed_api::provider::ToolCall {
                        id: "tc-skill-1".into(),
                        name: "Skill".into(),
                        arguments: r#"{"skill": "test-skill"}"#.into(),
                    }],
                    usage: None,
                    reasoning_content: None,
                    stop_reason: clawseed_api::provider::StopReason::EndTurn,
                },
                ChatResponse {
                    text: Some("done".into()),
                    tool_calls: vec![],
                    usage: None,
                    reasoning_content: None,
                    stop_reason: clawseed_api::provider::StopReason::EndTurn,
                },
            ]),
        });

        let observer: Arc<dyn Observer> = Arc::new(crate::observer::NoopObserver);
        let mut agent = Agent::builder()
            .provider(provider)
            .tools(vec![Box::new(clawseed_tools::skill_tool::SkillTool::new())])
            .memory(make_memory())
            .observer(observer)
            .tool_dispatcher(Box::new(NativeToolDispatcher))
            .workspace_dir(dir.path().to_path_buf())
            .skill_index(skill_index)
            .build()
            .expect("agent builder should succeed");

        let _response = agent.turn("use test-skill").await.unwrap();

        // Verify no sentinel JSON in history
        for msg in agent.history() {
            if let ConversationMessage::ToolResults(results) = msg {
                for result in results {
                    assert!(
                        !result.content.contains("__skill_action"),
                        "Sentinel JSON leaked into history: {}",
                        result.content
                    );
                    assert!(
                        !result.content.contains("__skill_name"),
                        "Sentinel JSON leaked into history: {}",
                        result.content
                    );
                }
            }
            if let ConversationMessage::Chat(chat) = msg {
                assert!(
                    !chat.content.contains("__skill_action"),
                    "Sentinel JSON leaked into history chat: {}",
                    chat.content
                );
            }
        }

        // Verify the skill was activated and history contains the activation message
        assert_eq!(agent.active_skills.len(), 1);
        assert_eq!(agent.active_skills[0].skill.name, "test-skill");
        let has_activation_msg = agent.history().iter().any(|msg| {
            if let ConversationMessage::ToolResults(results) = msg {
                results.iter().any(|r| r.content.contains("activated"))
            } else {
                false
            }
        });
        assert!(
            has_activation_msg,
            "History should contain skill activation message"
        );
    }
}
