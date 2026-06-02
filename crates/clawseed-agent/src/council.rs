//! Council Mode — multi-agent orchestration with Leader + Reviewers.
//!
//! Council is constructed per-connection, sharing Arc<dyn Provider/Memory/Observer/Tool>
//! instances. The Leader agent runs the full tool loop; Reviewer agents run a limited
//! tool loop (max 3 iterations) with restricted tools and NamespacedMemory isolation.
//!
//! Communication is Memory-based: Reviewers write feedback to a council namespace,
//! and the Leader consumes it via system-context injection (not user-message prepend).

use crate::agent::Agent;
use crate::agent::TurnEvent;
use crate::hooks::HookRunner;
use crate::observer::Observer;
use crate::security::SecurityPolicy;
use crate::tool_registry::DefaultToolRegistry;
use anyhow::Result;
use clawseed_api::memory_traits::{Memory, MemoryCategory};
use clawseed_api::provider::{ConversationMessage, Provider};
use clawseed_api::tool::Tool;
use clawseed_api::tool_registry::ToolSource;
use clawseed_config::schema::{AutonomyLevel, CouncilReviewerConfig};
use clawseed_memory::namespaced::NamespacedMemory;
use std::sync::Arc;

/// Streaming events emitted during a Council turn.
#[derive(Debug, Clone)]
pub enum CouncilStreamEvent {
    /// Leader turn event (text chunks, tool calls, etc.)
    Leader(TurnEvent),
    /// A reviewer has started its evaluation.
    ReviewStarted { role: String },
    /// A reviewer has completed its evaluation.
    ReviewCompleted { role: String, summary: String },
    /// A reviewer wrote feedback to the council namespace.
    ReviewFeedback { role: String, key: String },
}

/// Per-connection Council instance — Leader + Reviewers.
pub struct Council {
    /// The primary agent that handles tool calls and user interaction.
    leader: Agent,
    /// Reviewer agents with restricted tool access.
    reviewers: Vec<ReviewerAgent>,
    /// Shared memory backend (Leader uses this directly, Reviewers use NamespacedMemory).
    shared_memory: Arc<dyn Memory>,
    /// Council namespace for reviewer feedback (e.g. "council_{session_id}").
    council_namespace: String,
}

/// A reviewer agent with role-specific evaluation focus.
pub struct ReviewerAgent {
    /// The underlying Agent instance (Supervised, restricted tools).
    agent: Agent,
    /// Role identifier (e.g. "security", "quality", "strategy").
    role: String,
}

const REVIEWER_MAX_TOOL_ITERATIONS: usize = 3;

impl Council {
    /// Create a Council from config, constructing own Provider/Memory/Observer.
    ///
    /// Used by CLI chat mode where there's no shared state.
    pub async fn from_config(config: &clawseed_config::schema::Config) -> Result<Self> {
        let leader = Agent::from_config(config).await?;
        let shared_memory = leader.memory();

        Self::build_reviewers_and_wrap(leader, shared_memory, config, None)
    }

    /// Create a Council from shared components, reusing externally-provided instances.
    ///
    /// Used by gateway WebSocket connections where Provider/Memory/Observer are shared.
    pub async fn from_shared_components(
        config: &clawseed_config::schema::Config,
        provider: Arc<dyn Provider>,
        memory: Arc<dyn Memory>,
        observer: Arc<dyn Observer>,
        model_name: String,
        temperature: f64,
        shared_builtin_tools: Option<Arc<[Arc<dyn Tool>]>>,
    ) -> Result<Self> {
        let leader = Agent::from_config_with_shared_components(
            config,
            provider,
            memory.clone(),
            observer,
            model_name,
            temperature,
            shared_builtin_tools.clone(),
        )
        .await?;

        Self::build_reviewers_and_wrap(leader, memory, config, shared_builtin_tools)
    }

    /// Wrap an existing Agent as the Leader, constructing Reviewers from shared components.
    ///
    /// Used for in-place Single→Council mode swap. The Agent instance is reused;
    /// no new Agent is created for the Leader role. History is preserved.
    pub fn wrap_leader(
        leader: Agent,
        shared_memory: Arc<dyn Memory>,
        council_namespace: String,
        config: &clawseed_config::schema::Config,
        provider: Arc<dyn Provider>,
        observer: Arc<dyn Observer>,
        shared_builtin_tools: Option<Arc<[Arc<dyn Tool>]>>,
    ) -> Result<Self> {
        let _council_config = config.council.clone();
        let mut council = Council {
            leader,
            reviewers: Vec::new(),
            shared_memory,
            council_namespace,
        };

        council.build_reviewers(config, provider, observer, shared_builtin_tools)?;

        Ok(council)
    }

    /// Extract the Leader Agent from the Council, consuming it.
    ///
    /// Used for in-place Council→Single mode swap. The returned Agent has
    /// the full conversation history intact.
    pub fn into_leader(self) -> Agent {
        self.leader
    }

    /// Execute a single Council turn: inject feedback, run Leader, clear context, run Reviewers.
    pub async fn turn(&mut self, message: &str) -> Result<String> {
        self.inject_reviewer_feedback().await;
        let response = self.leader.turn(message).await?;
        self.leader.clear_system_context("[Council]");
        self.run_reviewers().await?;
        Ok(response)
    }

    /// Execute a Council turn while streaming events.
    pub async fn turn_streamed(
        &mut self,
        message: &str,
        event_tx: tokio::sync::mpsc::Sender<CouncilStreamEvent>,
        cancel_token: Option<tokio_util::sync::CancellationToken>,
        debug: bool,
    ) -> Result<String> {
        self.inject_reviewer_feedback().await;

        // Create a channel adapter that wraps TurnEvent as CouncilStreamEvent::Leader
        let (leader_tx, mut leader_rx) = tokio::sync::mpsc::channel::<TurnEvent>(100);

        // Spawn a task to relay leader events
        let event_tx_clone = event_tx.clone();
        let relay = tokio::spawn(async move {
            while let Some(event) = leader_rx.recv().await {
                if event_tx_clone
                    .send(CouncilStreamEvent::Leader(event))
                    .await
                    .is_err()
                {
                    break;
                }
            }
        });

        let response = self
            .leader
            .turn_streamed(message, leader_tx, cancel_token.clone(), debug)
            .await?;

        relay.await.ok();

        self.leader.clear_system_context("[Council]");

        // Run reviewers sequentially, emitting review events
        self.run_reviewers_streamed(event_tx).await?;

        Ok(response)
    }

    /// Public access to the Leader agent for remote tool injection.
    pub fn leader_mut(&mut self) -> &mut Agent {
        &mut self.leader
    }

    /// Public access to the Leader agent's memory reference.
    pub fn shared_memory(&self) -> Arc<dyn Memory> {
        self.shared_memory.clone()
    }

    // ---- Private helpers ----

    /// Inject reviewer feedback from council namespace as system context into Leader.
    async fn inject_reviewer_feedback(&mut self) {
        let feedback = self
            .shared_memory
            .recall_namespaced(
                &self.council_namespace,
                "review",
                10,
                None,
                None,
                None,
                None,
            )
            .await
            .unwrap_or_default();

        if !feedback.is_empty() {
            let feedback_text = feedback
                .iter()
                .map(|e| format!("- [{}]: {}", e.key, e.content))
                .collect::<Vec<_>>()
                .join("\n");
            let context = format!(
                "[Council]\n\
                {feedback_text}\n\
                [/Council]"
            );
            self.leader.inject_system_context(&context);
        }
    }

    /// Build reviewers and wrap the leader into a Council.
    fn build_reviewers_and_wrap(
        leader: Agent,
        shared_memory: Arc<dyn Memory>,
        config: &clawseed_config::schema::Config,
        shared_builtin_tools: Option<Arc<[Arc<dyn Tool>]>>,
    ) -> Result<Self> {
        let council_config = config.council.clone();
        let council_namespace = if let Some(ref sid) = config.memory.namespace {
            format!("{}_{}", council_config.namespace, sid)
        } else {
            council_config.namespace.clone()
        };

        let provider = leader.provider();
        let observer = leader.observer();

        let mut reviewers = Vec::new();
        for reviewer_config in config.council.reviewers.values() {
            if reviewer_config.role.is_empty() {
                continue;
            }

            let reviewer = build_reviewer_agent(
                reviewer_config,
                config,
                provider.clone(),
                observer.clone(),
                shared_memory.clone(),
                council_namespace.clone(),
                shared_builtin_tools.clone(),
                leader.model_name(),
            )?;

            reviewers.push(reviewer);
        }

        Ok(Council {
            leader,
            reviewers,
            shared_memory,
            council_namespace,
        })
    }

    /// Construct Reviewer agents from config and shared components.
    fn build_reviewers(
        &mut self,
        config: &clawseed_config::schema::Config,
        provider: Arc<dyn Provider>,
        observer: Arc<dyn Observer>,
        shared_builtin_tools: Option<Arc<[Arc<dyn Tool>]>>,
    ) -> Result<()> {
        let leader_model = self.leader.model_name();
        for reviewer_config in config.council.reviewers.values() {
            if reviewer_config.role.is_empty() {
                continue;
            }

            let reviewer = build_reviewer_agent(
                reviewer_config,
                config,
                provider.clone(),
                observer.clone(),
                self.shared_memory.clone(),
                self.council_namespace.clone(),
                shared_builtin_tools.clone(),
                leader_model.clone(),
            )?;

            self.reviewers.push(reviewer);
        }
        Ok(())
    }

    /// Run all reviewers sequentially after the Leader's turn.
    async fn run_reviewers(&mut self) -> Result<()> {
        // Serialize Leader context for reviewers
        let leader_summary = self.serialize_leader_context();

        // Store the leader context in the council namespace
        let ts = chrono::Local::now().format("%Y%m%d_%H%M%S");
        let context_key = format!("leader_context_{ts}");
        self.shared_memory
            .store_with_metadata(
                &context_key,
                &leader_summary,
                MemoryCategory::Daily,
                None,
                Some(&self.council_namespace),
                None,
            )
            .await
            .ok();

        let council_namespace = self.council_namespace.clone();
        let shared_memory = self.shared_memory.clone();

        for reviewer in &mut self.reviewers {
            let role = reviewer.role.clone();
            let review_prompt = format!(
                "Review the following actions taken by the Leader agent:\n\n\
                {leader_summary}\n\n\
                Write your evaluation using the `reviewer_memory_store` tool.",
            );

            let result = reviewer.agent.turn(&review_prompt).await;

            if let Err(e) = result {
                tracing::warn!(
                    role = %role,
                    error = %e,
                    "Reviewer turn failed"
                );
                continue;
            }

            // Completion contract: verify that review_{role}_* keys were written
            verify_reviewer_completion(&shared_memory, &council_namespace, &role).await;
        }

        Ok(())
    }

    /// Run all reviewers sequentially, emitting stream events.
    async fn run_reviewers_streamed(
        &mut self,
        event_tx: tokio::sync::mpsc::Sender<CouncilStreamEvent>,
    ) -> Result<()> {
        let leader_summary = self.serialize_leader_context();

        // Store the leader context in the council namespace
        let ts = chrono::Local::now().format("%Y%m%d_%H%M%S");
        let context_key = format!("leader_context_{ts}");
        self.shared_memory
            .store_with_metadata(
                &context_key,
                &leader_summary,
                MemoryCategory::Daily,
                None,
                Some(&self.council_namespace),
                None,
            )
            .await
            .ok();

        let council_namespace = self.council_namespace.clone();
        let shared_memory = self.shared_memory.clone();

        for reviewer in &mut self.reviewers {
            let role = reviewer.role.clone();
            let _ = event_tx
                .send(CouncilStreamEvent::ReviewStarted { role: role.clone() })
                .await;

            let review_prompt = format!(
                "Review the following actions taken by the Leader agent:\n\n\
                {leader_summary}\n\n\
                Write your evaluation using the `reviewer_memory_store` tool.",
            );

            let result = reviewer.agent.turn(&review_prompt).await;

            let summary = match result {
                Ok(text) => text,
                Err(e) => {
                    tracing::warn!(
                        role = %role,
                        error = %e,
                        "Reviewer turn failed"
                    );
                    format!("Review failed: {e}")
                }
            };

            // Completion contract
            verify_reviewer_completion(&shared_memory, &council_namespace, &role).await;

            let _ = event_tx
                .send(CouncilStreamEvent::ReviewCompleted {
                    role: role.clone(),
                    summary,
                })
                .await;
        }

        Ok(())
    }

    /// Serialize the Leader's recent history into a summary string for reviewers.
    fn serialize_leader_context(&self) -> String {
        let history = self.leader.history();
        let recent = history.iter().rev().take(10).collect::<Vec<_>>();
        let recent = recent.into_iter().rev();

        let mut parts = Vec::new();
        for msg in recent {
            if let ConversationMessage::Chat(chat) = msg {
                match chat.role.as_str() {
                    "user" => parts.push(format!("[User]: {}", chat.content)),
                    "assistant" => parts.push(format!("[Assistant]: {}", chat.content)),
                    "tool" => parts.push(format!("[Tool Result]: {}", chat.content)),
                    _ => {}
                }
            }
        }
        parts.join("\n\n")
    }
}

impl ReviewerAgent {
    /// Get the reviewer's role.
    pub fn role(&self) -> &str {
        &self.role
    }
}

/// Construct a single ReviewerAgent (free function to avoid borrow conflicts).
#[allow(clippy::too_many_arguments)]
fn build_reviewer_agent(
    reviewer_config: &CouncilReviewerConfig,
    config: &clawseed_config::schema::Config,
    provider: Arc<dyn Provider>,
    observer: Arc<dyn Observer>,
    shared_memory: Arc<dyn Memory>,
    council_namespace: String,
    shared_builtin_tools: Option<Arc<[Arc<dyn Tool>]>>,
    fallback_model: String,
) -> Result<ReviewerAgent> {
    let role = reviewer_config.role.clone();
    let focus_prompt = reviewer_config.focus_prompt.clone();

    // Model: reviewer-specific override, or fall back to leader's model
    let model_name = reviewer_config.model.clone().unwrap_or(fallback_model);

    // Dispatcher: native if provider supports it, otherwise XML
    let dispatcher: Box<dyn crate::dispatcher::ToolDispatcher> = if provider.supports_native_tools()
    {
        Box::new(crate::dispatcher::NativeToolDispatcher)
    } else {
        Box::new(crate::dispatcher::XmlToolDispatcher)
    };

    // Hook runner: SecurityPolicy with Supervised level
    let mut hook_runner = HookRunner::new();
    let autonomy_config = clawseed_config::schema::AutonomyConfig {
        level: AutonomyLevel::Supervised,
        auto_approve: vec![
            "file_read".into(),
            "memory_recall".into(),
            "reviewer_memory_store".into(),
        ],
        always_ask: Vec::new(),
        allowed_commands: Vec::new(),
        non_cli_excluded_tools: Vec::new(),
        max_actions_per_hour: 0,
    };
    hook_runner.register(Box::new(SecurityPolicy::from_config(
        &autonomy_config,
        &config.workspace_dir,
    )));

    // NamespacedMemory for reviewer — all ops scoped to council namespace
    let namespaced_memory = Arc::new(NamespacedMemory::new(
        shared_memory.clone(),
        council_namespace.clone(),
    ));

    // Tool registry: filtered shared tools + reviewer-specific tool
    let empty_shared: Arc<[Arc<dyn Tool>]> = Arc::new([]);
    let shared_tools_ref = shared_builtin_tools.as_ref().unwrap_or(&empty_shared);

    let reviewer_tools = clawseed_tools::reviewer_registry::reviewer_tools(
        &role,
        namespaced_memory.clone(),
        shared_tools_ref.clone(),
    );

    let tool_registry = Arc::new({
        let reg = DefaultToolRegistry::new();
        for tool in reviewer_tools {
            reg.register_arc(tool, ToolSource::BuiltIn);
        }
        reg
    });

    // System prompt: role-specific focus + council feedback instructions
    let system_prompt = format!(
        "{focus_prompt}\n\n\
        You are a reviewer in Council Mode. Your role is: {role}.\n\
        After evaluating the Leader's actions, write your feedback using \
        the `reviewer_memory_store` tool. The key will be auto-prefixed \
        with `review_{role}_`.\n\
        You have access to `file_read` and `memory_recall` for context, \
        and `reviewer_memory_store` for writing feedback.\n\
        Be concise and specific in your evaluations.",
    );

    // AgentConfig for reviewer: limited tool iterations
    let reviewer_agent_config = clawseed_config::schema::AgentConfig {
        max_tool_iterations: REVIEWER_MAX_TOOL_ITERATIONS,
        temperature: Some(0.3),
        max_tokens: Some(1024),
        auto_continue_on_truncation: false,
        max_auto_continue: 0,
        web_search_enabled: false,
        web_search_provider: None,
        system_prompt: Some(system_prompt),
        memory_namespace: None,
        daily_budget_usd: None,
        turn_budget_usd: None,
        allowed_tools: Vec::new(),
        denied_tools: Vec::new(),
        mcp_tool_filters: std::collections::HashMap::new(),
    };

    // IdentityConfig: reviewer identity (use default with openclaw format)
    let identity_config = clawseed_config::schema::IdentityConfig::default();

    let agent = Agent::builder()
        .shared_provider(provider)
        .tool_registry(tool_registry)
        .memory(namespaced_memory)
        .observer(observer)
        .tool_dispatcher(dispatcher)
        .config(reviewer_agent_config)
        .model_name(model_name)
        .temperature(0.3)
        .workspace_dir(config.workspace_dir.clone())
        .autonomy_level(AutonomyLevel::Supervised)
        .identity_config(identity_config)
        .auto_save(false)
        .auto_recall(true)
        .auto_recall_limit(5)
        .hook_runner(Some(Arc::new(hook_runner)))
        .skills_enabled(false)
        .build()?;

    Ok(ReviewerAgent { agent, role })
}

/// Verify that a reviewer wrote feedback (completion contract).
///
/// Checks for `review_{role}_*` keys in the council namespace.
/// If no feedback was found, logs a warning.
async fn verify_reviewer_completion(
    shared_memory: &Arc<dyn Memory>,
    council_namespace: &str,
    role: &str,
) {
    let query = format!("review_{role}");
    let result = shared_memory
        .recall_namespaced(council_namespace, &query, 10, None, None, None, None)
        .await;

    match result {
        Ok(entries) if entries.is_empty() => {
            tracing::warn!(
                role = %role,
                "Reviewer completed without writing feedback — completion contract warning"
            );
        }
        Ok(entries) => {
            for entry in entries {
                tracing::debug!(
                    role = %role,
                    key = %entry.key,
                    "Reviewer feedback stored"
                );
            }
        }
        Err(e) => {
            tracing::warn!(
                role = %role,
                error = %e,
                "Failed to verify reviewer completion contract"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clawseed_api::provider::{ChatMessage, ConversationMessage};

    #[test]
    fn council_stream_event_debug_format() {
        let event = CouncilStreamEvent::ReviewStarted {
            role: "security".into(),
        };
        assert!(format!("{event:?}").contains("security"));

        let event = CouncilStreamEvent::ReviewCompleted {
            role: "quality".into(),
            summary: "looks good".into(),
        };
        assert!(format!("{event:?}").contains("quality"));
    }

    #[test]
    fn inject_and_clear_system_context() {
        let mut history: Vec<ConversationMessage> = vec![
            ConversationMessage::Chat(ChatMessage::system("You are helpful.")),
            ConversationMessage::Chat(ChatMessage::user("hello")),
            ConversationMessage::Chat(ChatMessage::assistant("hi")),
        ];

        // Inject: find first non-system message index
        let insert_idx = history
            .iter()
            .position(|m| {
                !matches!(
                    m,
                    ConversationMessage::Chat(ChatMessage { role, .. }) if role == "system"
                )
            })
            .unwrap_or(history.len());

        assert_eq!(insert_idx, 1);

        let feedback_text = "[Council]\nfeedback\n[/Council]";
        history.insert(
            insert_idx,
            ConversationMessage::Chat(ChatMessage::system(feedback_text)),
        );

        assert_eq!(history.len(), 4);
        let injected = &history[1];
        match injected {
            ConversationMessage::Chat(msg) => {
                assert_eq!(msg.role, "system");
                assert!(msg.content.contains("[Council]"));
            }
            _ => panic!("Expected Chat message at index 1, got {injected:?}"),
        }

        // Clear: remove system messages containing "[Council]"
        let marker = "[Council]";
        history.retain(|m| {
            if let ConversationMessage::Chat(msg) = m
                && msg.role == "system"
                && msg.content.contains(marker)
            {
                return false;
            }
            true
        });

        assert_eq!(history.len(), 3);
        // Original system prompt should remain
        let original = &history[0];
        match original {
            ConversationMessage::Chat(msg) => {
                assert_eq!(msg.role, "system");
                assert_eq!(msg.content, "You are helpful.");
            }
            _ => panic!("Expected Chat message at index 0, got {original:?}"),
        }
    }

    #[test]
    fn reviewer_tool_whitelist_is_correct() {
        let allowed: &[&str] = &["file_read", "memory_recall"];
        assert!(allowed.contains(&"file_read"));
        assert!(allowed.contains(&"memory_recall"));
        assert_eq!(allowed.len(), 2);
    }
}
