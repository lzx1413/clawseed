//! Agent types, builder, and configuration-based runtime assembly.
//!
//! Conversation state, tool execution, and turn orchestration live in focused
//! child modules while the public `agent::Agent` path remains stable.

use crate::dispatcher::ToolDispatcher;
use crate::hooks::HookRunner;
use crate::observer::Observer;
use crate::security::SecurityPolicy;
use crate::tool_registry::DefaultToolRegistry;
use anyhow::Result;
use clawseed_api::memory_traits::Memory;
use clawseed_api::provider::{ConversationMessage, Provider};
use clawseed_api::tool::Tool;
use clawseed_api::tool_registry::{ToolRegistry, ToolSource};
use clawseed_api::user_profile::{ProfileItem, UserContext, UserProfileStore};
use clawseed_config::schema::{AutonomyLevel, IdentityConfig};
use std::sync::Arc;

mod metrics;
mod state;
mod tool_execution;
mod turn;

/// Streaming events emitted during an agent turn.
#[derive(Debug, Clone)]
pub enum TurnEvent {
    Metrics(clawseed_api::provider::ResponseMetrics),
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
        presentation: Option<clawseed_api::tool::ToolPresentation>,
    },
    DebugPrompt {
        messages_json: String,
        estimated_tokens: usize,
        tools_json: Option<String>,
        estimated_tool_tokens: usize,
    },
}

/// A successfully completed user/assistant exchange passed to post-turn learning.
pub struct CompletedTurn<'a> {
    pub user_text: &'a str,
    pub assistant_text: &'a str,
    pub session_id: Option<&'a str>,
    pub persona_id: Option<&'a str>,
    pub memory_namespace: &'a str,
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
    provider_extra: Option<serde_json::Value>,
    temperature: f64,
    workspace_dir: std::path::PathBuf,
    autonomy_level: AutonomyLevel,
    identity_config: IdentityConfig,
    auto_save: bool,
    auto_recall: bool,
    auto_recall_limit: usize,
    memory_min_relevance_score: f64,
    memory_conflict_mode: clawseed_api::memory_traits::ConflictMode,
    memory_conflict_threshold: f64,
    stable_memory_in_system_prompt: bool,
    memory_session_id: Option<String>,
    user_profile_store: Option<Arc<dyn UserProfileStore>>,
    user_context: Option<UserContext>,
    active_turn_id: Option<String>,
    user_profile_version: Option<u64>,
    user_profile_items: Vec<ProfileItem>,
    max_profile_prompt_items: usize,
    user_model_config: clawseed_config::schema::UserModelConfig,
    knowledge_coordinator: Arc<crate::knowledge_coordinator::KnowledgeCoordinator>,
    history: Vec<ConversationMessage>,
    hook_runner: Option<Arc<HookRunner>>,
    skill_index: Vec<crate::skills::SkillIndexEntry>,
    active_skills: Vec<crate::skills::ActiveSkill>,
    max_active_skills: usize,
    skills_extra_roots: Vec<String>,
    skills_enabled: bool,
    skills_excluded: Vec<String>,
    /// Tracks which Core memories are currently rendered in the system prompt.
    /// Maps key → content_hash for dedup against dynamic auto-recall.
    injected_core_state: std::collections::HashMap<String, String>,
    /// Cached Core memory entries for system prompt rendering.
    stable_core_memories: Vec<clawseed_api::memory_traits::MemoryEntry>,
    /// Cached stable portion of the system prompt. Mirrors the `stable_prefix`
    /// on the system ChatMessage in history. With DateTimeSection removed,
    /// this is always equal to the full system prompt content.
    stable_system_content: String,
}

fn replace_memory_tools(registry: &DefaultToolRegistry, memory: Arc<dyn Memory>) {
    registry.register_or_replace(
        Box::new(clawseed_tools::memory_export::MemoryExportTool::new(
            memory.clone(),
        )),
        ToolSource::BuiltIn,
    );
    registry.register_or_replace(
        Box::new(clawseed_tools::memory_forget::MemoryForgetTool::new(
            memory.clone(),
        )),
        ToolSource::BuiltIn,
    );
    registry.register_or_replace(
        Box::new(clawseed_tools::memory_purge::MemoryPurgeTool::new(
            memory.clone(),
        )),
        ToolSource::BuiltIn,
    );
    registry.register_or_replace(
        Box::new(clawseed_tools::memory_recall::MemoryRecallTool::new(
            memory.clone(),
        )),
        ToolSource::BuiltIn,
    );
    registry.register_or_replace(
        Box::new(clawseed_tools::memory_store::MemoryStoreTool::new(memory)),
        ToolSource::BuiltIn,
    );
}

const PROFILE_TOOL_NAMES: [&str; 5] = [
    "user_profile_search",
    "user_profile_change_plan",
    "user_profile_apply_plan",
    "user_profile_delete",
    "user_profile_undo",
];

fn replace_profile_tools(
    registry: &dyn ToolRegistry,
    store: Arc<dyn UserProfileStore>,
    config: &clawseed_config::schema::UserModelConfig,
) {
    registry.register_or_replace(
        Box::new(clawseed_tools::user_profile::UserProfileSearchTool::new(
            store.clone(),
        )),
        ToolSource::BuiltIn,
    );
    registry.register_or_replace(
        Box::new(
            clawseed_tools::user_profile::UserProfileChangePlanTool::new(
                store.clone(),
                config.change_plan_ttl_minutes,
            ),
        ),
        ToolSource::BuiltIn,
    );
    registry.register_or_replace(
        Box::new(clawseed_tools::user_profile::UserProfileApplyPlanTool::new(
            store.clone(),
        )),
        ToolSource::BuiltIn,
    );
    registry.register_or_replace(
        Box::new(clawseed_tools::user_profile::UserProfileDeleteTool::new(
            store,
        )),
        ToolSource::BuiltIn,
    );
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
    provider_extra: Option<serde_json::Value>,
    temperature: Option<f64>,
    workspace_dir: Option<std::path::PathBuf>,
    autonomy_level: Option<AutonomyLevel>,
    identity_config: Option<IdentityConfig>,
    auto_save: Option<bool>,
    auto_recall: Option<bool>,
    auto_recall_limit: Option<usize>,
    memory_min_relevance_score: Option<f64>,
    memory_conflict_mode: Option<clawseed_api::memory_traits::ConflictMode>,
    memory_conflict_threshold: Option<f64>,
    stable_memory_in_system_prompt: Option<bool>,
    memory_session_id: Option<String>,
    user_profile_store: Option<Arc<dyn UserProfileStore>>,
    user_context: Option<UserContext>,
    max_profile_prompt_items: Option<usize>,
    user_model_config: Option<clawseed_config::schema::UserModelConfig>,
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
            provider_extra: None,
            temperature: None,
            workspace_dir: None,
            autonomy_level: None,
            identity_config: None,
            auto_save: None,
            auto_recall: None,
            auto_recall_limit: None,
            memory_min_relevance_score: None,
            memory_conflict_mode: None,
            memory_conflict_threshold: None,
            stable_memory_in_system_prompt: None,
            memory_session_id: None,
            user_profile_store: None,
            user_context: None,
            max_profile_prompt_items: None,
            user_model_config: None,
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

    pub fn provider_extra(mut self, provider_extra: Option<serde_json::Value>) -> Self {
        self.provider_extra = provider_extra;
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

    pub fn memory_min_relevance_score(mut self, score: f64) -> Self {
        self.memory_min_relevance_score = Some(score);
        self
    }

    pub fn memory_conflict_mode(mut self, mode: clawseed_api::memory_traits::ConflictMode) -> Self {
        self.memory_conflict_mode = Some(mode);
        self
    }

    pub fn memory_conflict_threshold(mut self, threshold: f64) -> Self {
        self.memory_conflict_threshold = Some(threshold);
        self
    }

    pub fn stable_memory_in_system_prompt(mut self, enabled: bool) -> Self {
        self.stable_memory_in_system_prompt = Some(enabled);
        self
    }

    pub fn memory_session_id(mut self, session_id: Option<String>) -> Self {
        self.memory_session_id = session_id;
        self
    }

    pub fn user_profile_store(mut self, store: Arc<dyn UserProfileStore>) -> Self {
        self.user_profile_store = Some(store);
        self
    }

    pub fn user_context(mut self, context: UserContext) -> Self {
        self.user_context = Some(context);
        self
    }

    pub fn max_profile_prompt_items(mut self, limit: usize) -> Self {
        self.max_profile_prompt_items = Some(limit);
        self
    }

    pub fn user_model_config(mut self, config: clawseed_config::schema::UserModelConfig) -> Self {
        self.user_model_config = Some(config);
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

        let user_model_config = self.user_model_config.unwrap_or_default();
        if let Some(store) = self.user_profile_store.as_ref() {
            replace_profile_tools(registry.as_ref(), store.clone(), &user_model_config);
        }

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
            provider_extra: self.provider_extra,
            temperature: self.temperature.unwrap_or(0.7),
            workspace_dir: self
                .workspace_dir
                .unwrap_or_else(|| std::path::PathBuf::from(".")),
            autonomy_level: self.autonomy_level.unwrap_or_default(),
            identity_config: self.identity_config.unwrap_or_default(),
            auto_save: self.auto_save.unwrap_or(false),
            auto_recall: self.auto_recall.unwrap_or(true),
            auto_recall_limit: self.auto_recall_limit.unwrap_or(3),
            memory_min_relevance_score: self.memory_min_relevance_score.unwrap_or(0.4),
            memory_conflict_mode: self.memory_conflict_mode.unwrap_or_default(),
            memory_conflict_threshold: self.memory_conflict_threshold.unwrap_or(0.82),
            stable_memory_in_system_prompt: self.stable_memory_in_system_prompt.unwrap_or(true),
            memory_session_id: self.memory_session_id,
            user_profile_store: self.user_profile_store,
            user_context: self.user_context,
            active_turn_id: None,
            user_profile_version: None,
            user_profile_items: Vec::new(),
            max_profile_prompt_items: self.max_profile_prompt_items.unwrap_or(20),
            user_model_config,
            knowledge_coordinator: Arc::new(
                crate::knowledge_coordinator::KnowledgeCoordinator::new(),
            ),
            history: Vec::new(),
            hook_runner: self.hook_runner,
            skill_index: self.skill_index.unwrap_or_default(),
            active_skills: Vec::new(),
            max_active_skills: self.max_active_skills.unwrap_or(5),
            skills_extra_roots: self.skills_extra_roots.unwrap_or_default(),
            skills_enabled: self.skills_enabled.unwrap_or(true),
            skills_excluded: self.skills_excluded.unwrap_or_default(),
            injected_core_state: std::collections::HashMap::new(),
            stable_core_memories: Vec::new(),
            stable_system_content: String::new(),
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

        let mut agent = Self::build_from_config(
            config,
            provider,
            mem,
            observer,
            model_name,
            temperature,
            None,
        )?;
        if config.user_model.enabled {
            let store = clawseed_memory::user_profile::SqliteUserProfileStore::with_governance(
                &config.workspace_dir,
                config.user_model.max_active_items_per_category,
                config.user_model.min_observations_for_implicit_fact,
                config.user_model.undo_retention_hours,
            )?;
            agent.set_user_profile_store(Some(Arc::new(store)), config.user_model.max_prompt_items);
            agent.set_user_context(Some(UserContext {
                user_id: "owner".into(),
                session_id: None,
                persona_id: None,
            }));
        }
        Ok(agent)
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
        // A global memory namespace is a storage isolation boundary, not a
        // session identifier. Persona paths already pass a namespaced wrapper.
        let memory = if config.agent.memory_namespace.is_none() {
            config
                .memory
                .namespace
                .as_ref()
                .map(|namespace| {
                    Arc::new(clawseed_memory::namespaced::NamespacedMemory::new(
                        memory.clone(),
                        namespace.clone(),
                    )) as Arc<dyn Memory>
                })
                .unwrap_or(memory)
        } else {
            memory
        };

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
            if config.agent.memory_namespace.is_some() || config.memory.namespace.is_some() {
                replace_memory_tools(&reg, memory.clone());
            }
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

        // Seed built-in skills before loading the index
        crate::skills::builtin::ensure_builtin_skills(&config.workspace_dir);

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

        let builder = Agent::builder()
            .shared_provider(provider)
            .tool_registry(registry)
            .memory(memory)
            .observer(observer)
            .tool_dispatcher(dispatcher)
            .config(config.agent.clone())
            .model_name(model_name)
            .provider_extra(
                config
                    .providers
                    .fallback_provider()
                    .and_then(|entry| entry.provider_extra.clone()),
            )
            .temperature(temperature)
            .workspace_dir(config.workspace_dir.clone())
            .autonomy_level(config.autonomy.level)
            .identity_config(config.identity.clone())
            .auto_save(config.memory.auto_save)
            .auto_recall(config.memory.auto_recall)
            .auto_recall_limit(config.memory.auto_recall_limit)
            .memory_min_relevance_score(config.memory.min_relevance_score)
            .memory_conflict_mode(config.memory.effective_conflict_mode())
            .memory_conflict_threshold(config.memory.conflict_threshold)
            .stable_memory_in_system_prompt(
                config.memory.effective_stable_memory_in_system_prompt(),
            )
            .user_model_config(config.user_model.clone())
            .hook_runner(Some(Arc::new(hook_runner)))
            .skill_index(skill_index)
            .max_active_skills(config.skills.max_active)
            .skills_extra_roots(extra_roots)
            .skills_enabled(config.skills.enabled)
            .skills_excluded(config.skills.excluded.clone());

        builder.build()
    }
}

#[cfg(test)]
mod tests;
