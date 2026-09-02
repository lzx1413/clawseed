//! Conversation state, prompt context, memory, and skill lifecycle.

use super::Agent;
use crate::dispatcher::ParsedToolCall;
use crate::prompt::{PartitionedSystemPrompt, PromptContext, SystemPromptBuilder};
use anyhow::Result;
use clawseed_api::provider::{ChatMessage, ConversationMessage};
use clawseed_api::tool::Tool;
use clawseed_api::tool_registry::ToolSource;
use clawseed_api::user_profile::{ProfileItem, ProfileStatus, UserContext, UserProfileStore};
use std::sync::Arc;

impl Agent {
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

    pub fn set_user_profile_store(
        &mut self,
        store: Option<Arc<dyn UserProfileStore>>,
        max_prompt_items: usize,
    ) {
        self.user_profile_store = store;
        self.max_profile_prompt_items = max_prompt_items;
        self.user_profile_version = None;
        self.user_profile_items.clear();
    }

    pub fn set_user_context(&mut self, context: Option<UserContext>) {
        self.user_context = context;
        self.user_profile_version = None;
        self.user_profile_items.clear();
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

    /// Refresh stable Core memories from the memory backend.
    ///
    /// Returns `true` if the Core memories changed (new keys, removed keys,
    /// or content hash differences), indicating that the system prompt
    /// should be rebuilt. Returns `false` if unchanged.
    pub(super) async fn refresh_stable_core_memories(&mut self) -> bool {
        if !self.stable_memory_in_system_prompt || self.memory.name() == "none" {
            // Feature disabled — clear any existing state
            self.stable_core_memories.clear();
            self.injected_core_state.clear();
            return false;
        }

        let entries = match self.memory.top_core_memories(self.auto_recall_limit).await {
            Ok(e) => e,
            Err(_) => return false, // Silently skip on error
        };

        // Build new state: key → content_hash
        let new_state: std::collections::HashMap<String, String> = entries
            .iter()
            .map(|e| {
                (
                    e.key.clone(),
                    clawseed_memory::sqlite::SqliteMemory::content_hash(&e.content),
                )
            })
            .collect();

        // Compare against previous state
        if new_state == self.injected_core_state {
            // Unchanged — no rebuild needed
            return false;
        }

        // Changed — update state and stored memories
        self.injected_core_state = new_state;
        self.stable_core_memories = entries;
        true
    }

    /// Refresh the profile for the authenticated user.
    pub(super) async fn refresh_user_profile(&mut self) -> bool {
        let (Some(store), Some(context)) = (&self.user_profile_store, &self.user_context) else {
            let changed = !self.user_profile_items.is_empty();
            self.user_profile_items.clear();
            self.user_profile_version = None;
            return changed;
        };
        let profile = match store.load(&context.user_id).await {
            Ok(profile) => profile,
            Err(error) => {
                tracing::debug!(user_id = %context.user_id, %error, "user profile refresh skipped");
                return false;
            }
        };
        let now = chrono::Utc::now();
        let items: Vec<ProfileItem> = profile
            .items
            .into_iter()
            .filter(|item| item.status == ProfileStatus::Active)
            .filter(|item| {
                item.expires_at.as_deref().is_none_or(|expires_at| {
                    chrono::DateTime::parse_from_rfc3339(expires_at)
                        .map(|expiry| expiry > now)
                        .unwrap_or(false)
                })
            })
            .take(self.max_profile_prompt_items)
            .collect();
        let changed =
            self.user_profile_version != Some(profile.version) || self.user_profile_items != items;
        self.user_profile_items = items;
        self.user_profile_version = Some(profile.version);
        changed
    }

    pub(super) fn schedule_user_profile_inference(
        &self,
        user_message: &str,
        assistant_response: &str,
    ) {
        if !self.user_model_config.enabled || !self.user_model_config.auto_infer {
            return;
        }
        let (Some(store), Some(context)) = (&self.user_profile_store, &self.user_context) else {
            return;
        };
        crate::user_model::spawn_profile_inference(
            self.provider.clone(),
            store.clone(),
            context.clone(),
            self.model_name.clone(),
            user_message.to_string(),
            assistant_response.to_string(),
            crate::user_model::InferenceOptions {
                min_confidence: self.user_model_config.inference_min_confidence,
                max_items: self.user_model_config.max_inferred_items_per_turn,
            },
        );
    }

    /// Rebuild the system prompt and replace the system message in history.
    pub(super) fn rebuild_system_prompt(&mut self) -> Result<()> {
        let partitioned = self.build_system_prompt_partitioned()?;
        self.stable_system_content = partitioned.stable.clone();

        for msg in &mut self.history {
            if let ConversationMessage::Chat(chat) = msg
                && chat.role == "system"
            {
                chat.content = partitioned.full.clone();
                chat.stable_prefix = Some(partitioned.stable.clone());
                return Ok(());
            }
        }

        // No system message found — prepend one
        self.history.insert(
            0,
            ConversationMessage::Chat(ChatMessage::system_partitioned(
                partitioned.stable,
                partitioned.full,
            )),
        );
        Ok(())
    }

    /// Refresh the skill index from disk and rebuild the system prompt.
    fn refresh_skills(&mut self) -> Result<()> {
        if !self.skills_enabled {
            return Ok(());
        }
        self.skill_index = crate::skills::load_skill_index_with_roots(
            &self.workspace_dir,
            &self.skills_extra_roots,
        )
        .into_iter()
        .collect();
        self.rebuild_system_prompt()?;
        Ok(())
    }

    /// Handle Skill tool calls: activate or deactivate skills.
    ///
    /// Reads action/skill_name from the original tool call arguments (not from
    /// the tool output), performs activation/deactivation, and updates the
    /// result output with the final semantic message. This way no sentinel
    /// JSON ever appears in observer events, hooks, or history.
    pub(super) fn handle_skill_tool_results(
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

    /// Handle skill_create tool calls: refresh skill index and optionally activate.
    pub(super) fn handle_skill_create_results(
        &mut self,
        calls: &[ParsedToolCall],
        results: &mut [crate::dispatcher::ToolExecutionResult],
    ) {
        for (i, call) in calls.iter().enumerate() {
            if call.name != "skill_create" || i >= results.len() {
                continue;
            }

            let result = &mut results[i];
            if !result.success {
                continue;
            }

            let skill_name = match call.arguments.get("name").and_then(|v| v.as_str()) {
                Some(name) if !name.is_empty() => name,
                _ => continue,
            };

            let activate = call
                .arguments
                .get("activate")
                .and_then(|v| v.as_bool())
                .unwrap_or(true);

            // Refresh skill index from disk so the new skill appears
            if let Err(e) = self.refresh_skills() {
                result.output = format!(
                    "Created skill '{}' but failed to refresh skill index: {}",
                    skill_name, e
                );
                continue;
            }

            if activate {
                match self.activate_skill(skill_name) {
                    Ok(msg) => {
                        result.output =
                            format!("Created and activated skill '{}'. {}", skill_name, msg);
                    }
                    Err(e) => {
                        result.output = format!(
                            "Created skill '{}' but activation failed: {}",
                            skill_name, e
                        );
                    }
                }
            } else {
                result.output = format!(
                    "Created skill '{}'. Use Skill({{\"skill\": \"{}\"}}) to activate it.",
                    skill_name, skill_name
                );
            }
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

    /// Return the final enriched content of the last user message in history.
    /// This is what the LLM actually saw (timestamp prefix + memory context + original text).
    /// Used by the gateway to persist enriched content so session resume preserves
    /// prompt cache fidelity — the restored content matches byte-for-byte what the
    /// LLM originally processed.
    pub fn last_user_message_content(&self) -> Option<String> {
        self.history.iter().rev().find_map(|msg| {
            if let ConversationMessage::Chat(chat) = msg
                && chat.role == "user"
            {
                Some(chat.content.clone())
            } else {
                None
            }
        })
    }

    /// Hydrate the agent with prior chat messages.
    pub fn seed_history(&mut self, messages: &[ChatMessage]) {
        // Discard any existing system message from input and rebuild from current
        // context. A restored session's system message reflects state at save time;
        // rebuilding ensures the partition reflects reality at resume time.
        if self.history.is_empty() {
            if let Ok(partitioned) = self.build_system_prompt_partitioned() {
                self.stable_system_content = partitioned.stable.clone();
                self.history
                    .push(ConversationMessage::Chat(ChatMessage::system_partitioned(
                        partitioned.stable,
                        partitioned.full,
                    )));
            } else if let Ok(sys) = self.build_system_prompt() {
                // Fallback: if partitioned build fails, use legacy single-block.
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

    pub(super) fn trim_history(&mut self) {
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
            user_profile_items: &self.user_profile_items,
            stable_core_memories: &self.stable_core_memories,
            system_prompt_override: self.config.system_prompt.as_deref(),
        };

        SystemPromptBuilder::with_defaults().build(&ctx)
    }

    pub(super) fn build_system_prompt_partitioned(&self) -> Result<PartitionedSystemPrompt> {
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
            user_profile_items: &self.user_profile_items,
            stable_core_memories: &self.stable_core_memories,
            system_prompt_override: self.config.system_prompt.as_deref(),
        };

        SystemPromptBuilder::with_defaults().build_partitioned(&ctx)
    }
}
