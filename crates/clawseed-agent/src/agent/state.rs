//! Conversation state, prompt context, memory, and skill lifecycle.

use super::Agent;
use crate::dispatcher::ParsedToolCall;
use crate::prompt::{PartitionedSystemPrompt, PromptContext, SystemPromptBuilder};
use anyhow::Result;
use clawseed_api::provider::{ChatMessage, ConversationMessage};
use clawseed_api::tool::Tool;
use clawseed_api::tool_registry::ToolSource;
use clawseed_api::user_profile::{
    ProfileCategory, ProfileItem, ProfileSource, ProfileStatus, UserContext, UserProfileStore,
};
use std::collections::HashMap;
use std::sync::Arc;

fn profile_category_rank(category: ProfileCategory) -> u8 {
    match category {
        ProfileCategory::Accessibility => 0,
        ProfileCategory::Constraint => 1,
        ProfileCategory::Identity => 2,
        ProfileCategory::Goal => 3,
        ProfileCategory::Preference => 4,
        ProfileCategory::Expertise => 5,
    }
}

fn profile_source_rank(source: ProfileSource) -> u8 {
    match source {
        ProfileSource::Explicit => 0,
        ProfileSource::Imported => 1,
        ProfileSource::Inferred => 2,
    }
}

struct ContextAssembler;

impl ContextAssembler {
    fn select_profile_items(mut items: Vec<ProfileItem>, limit: usize) -> Vec<ProfileItem> {
        if limit == 0 {
            return Vec::new();
        }
        items.sort_by(|left, right| {
            profile_category_rank(left.category)
                .cmp(&profile_category_rank(right.category))
                .then_with(|| {
                    profile_source_rank(left.source).cmp(&profile_source_rank(right.source))
                })
                .then_with(|| {
                    right
                        .confidence
                        .partial_cmp(&left.confidence)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .then_with(|| right.updated_at.cmp(&left.updated_at))
                .then_with(|| left.key.cmp(&right.key))
        });

        let category_quota = limit.div_ceil(6).max(1);
        let mut category_counts = HashMap::<ProfileCategory, usize>::new();
        let mut selected = Vec::with_capacity(limit.min(items.len()));
        for item in &items {
            let count = category_counts.entry(item.category).or_default();
            if *count < category_quota {
                selected.push(item.clone());
                *count += 1;
            }
        }
        if selected.len() < limit {
            for item in items {
                if selected.iter().any(|selected| selected.id == item.id) {
                    continue;
                }
                selected.push(item);
                if selected.len() == limit {
                    break;
                }
            }
        }
        selected.truncate(limit);
        selected
    }

    fn memory_is_covered_by_profile(
        entry: &clawseed_api::memory_traits::MemoryEntry,
        profile: &[ProfileItem],
    ) -> bool {
        profile.iter().any(|profile| {
            if profile.key == entry.key {
                return true;
            }
            let profile_text = profile
                .value
                .as_str()
                .map(str::to_string)
                .unwrap_or_else(|| profile.value.to_string());
            profile_text.trim() == entry.content.trim()
        })
    }
}

impl Agent {
    pub fn history(&self) -> &[ConversationMessage] {
        &self.history
    }

    pub fn clear_history(&mut self) {
        self.history.clear();
    }

    pub fn validate_image_model(&self, new_images: bool) -> anyhow::Result<()> {
        let has_images = new_images || self.history.iter().any(|message| {
            matches!(message, ConversationMessage::Chat(chat) if !chat.attachments.is_empty())
        });
        anyhow::ensure!(
            !has_images || self.provider.supports_image_attachments(&self.model_name),
            "Model {} does not support image attachments; select a supported vision model",
            self.model_name
        );
        Ok(())
    }

    pub fn last_user_attachments(&self) -> Vec<clawseed_api::provider::ImageAttachment> {
        self.history
            .iter()
            .rev()
            .find_map(|message| match message {
                ConversationMessage::Chat(chat) if chat.role == "user" => {
                    Some(chat.attachments.clone())
                }
                _ => None,
            })
            .unwrap_or_default()
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
        if let Some(store) = store.as_ref() {
            super::replace_profile_tools(
                self.tool_registry.as_ref(),
                store.clone(),
                &self.user_model_config,
            );
        } else {
            for name in super::PROFILE_TOOL_NAMES {
                self.tool_registry.unregister(name);
            }
        }
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

        let candidates = match self
            .memory
            .top_core_memories(
                self.auto_recall_limit
                    .saturating_add(self.user_profile_items.len()),
            )
            .await
        {
            Ok(e) => e,
            Err(_) => return false, // Silently skip on error
        };
        let entries = candidates
            .into_iter()
            .filter(|entry| !self.memory_is_covered_by_profile(entry))
            .take(self.auto_recall_limit)
            .collect::<Vec<_>>();

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

    pub(super) fn memory_is_covered_by_profile(
        &self,
        entry: &clawseed_api::memory_traits::MemoryEntry,
    ) -> bool {
        ContextAssembler::memory_is_covered_by_profile(entry, &self.user_profile_items)
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
            .collect();
        let items = ContextAssembler::select_profile_items(items, self.max_profile_prompt_items);
        let changed =
            self.user_profile_version != Some(profile.version) || self.user_profile_items != items;
        self.user_profile_items = items;
        self.user_profile_version = Some(profile.version);
        changed
    }

    /// Schedule post-turn learning exactly once after a successful final reply.
    pub(super) fn complete_turn(&self, user_message: &str, assistant_response: &str) {
        let completed = super::CompletedTurn {
            user_text: user_message,
            assistant_text: assistant_response,
            session_id: self.memory_session_id.as_deref(),
            persona_id: self
                .user_context
                .as_ref()
                .and_then(|context| context.persona_id.as_deref()),
            memory_namespace: self.config.memory_namespace.as_deref().unwrap_or("default"),
        };
        let profile_inference = (self.user_model_config.enabled
            && self.user_model_config.auto_infer)
            .then_some(crate::user_model::InferenceOptions {
                min_confidence: self.user_model_config.inference_min_confidence,
                max_items: self.user_model_config.max_inferred_items_per_turn,
            });
        if !self.auto_save && profile_inference.is_none() {
            return;
        }
        self.knowledge_coordinator
            .submit(crate::knowledge_coordinator::LearningJob {
                provider: self.provider.clone(),
                memory: self.memory.clone(),
                profile_store: self.user_profile_store.clone(),
                user_context: self.user_context.clone(),
                model: self.model_name.clone(),
                user_text: completed.user_text.to_string(),
                assistant_text: completed.assistant_text.to_string(),
                session_id: completed.session_id.map(str::to_string),
                persona_id: completed.persona_id.map(str::to_string),
                memory_namespace: completed.memory_namespace.to_string(),
                auto_save: self.auto_save,
                profile_inference,
                conflict_mode: self.memory_conflict_mode.clone(),
                conflict_threshold: self.memory_conflict_threshold,
            });
    }

    /// Close the background learning queue and wait for already accepted jobs.
    pub async fn shutdown_learning(&self) {
        self.knowledge_coordinator.shutdown_and_drain().await;
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
            native_tools: self.tool_dispatcher.should_send_tool_specs(),
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
            native_tools: self.tool_dispatcher.should_send_tool_specs(),
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

#[cfg(test)]
mod context_assembler_tests {
    use super::*;
    use clawseed_api::memory_traits::{MemoryCategory, MemoryEntry};

    fn profile_item(id: &str, key: &str, value: &str, category: ProfileCategory) -> ProfileItem {
        ProfileItem {
            id: id.into(),
            user_id: "owner".into(),
            key: key.into(),
            value: serde_json::json!(value),
            category,
            confidence: 0.9,
            source: ProfileSource::Explicit,
            status: ProfileStatus::Active,
            evidence_session_id: None,
            expires_at: None,
            created_at: "2026-01-01T00:00:00Z".into(),
            updated_at: "2026-01-01T00:00:00Z".into(),
            version: 1,
        }
    }

    fn memory(key: &str, content: &str) -> MemoryEntry {
        MemoryEntry {
            id: key.into(),
            key: key.into(),
            content: content.into(),
            category: MemoryCategory::Core,
            timestamp: "2026-01-01T00:00:00Z".into(),
            session_id: None,
            score: None,
            namespace: "default".into(),
            importance: Some(1.0),
            superseded_by: None,
            embedding: None,
        }
    }

    #[test]
    fn profile_selection_reserves_space_across_categories() {
        let items = vec![
            profile_item(
                "p1",
                "custom.preference.one",
                "one",
                ProfileCategory::Preference,
            ),
            profile_item(
                "p2",
                "custom.preference.two",
                "two",
                ProfileCategory::Preference,
            ),
            profile_item(
                "p3",
                "custom.preference.three",
                "three",
                ProfileCategory::Preference,
            ),
            profile_item(
                "a1",
                "accessibility.screen_reader",
                "true",
                ProfileCategory::Accessibility,
            ),
            profile_item("g1", "custom.goal.ship", "ship", ProfileCategory::Goal),
        ];
        let selected = ContextAssembler::select_profile_items(items, 3);
        assert_eq!(selected.len(), 3);
        assert!(
            selected
                .iter()
                .any(|item| item.category == ProfileCategory::Accessibility)
        );
        assert!(
            selected
                .iter()
                .any(|item| item.category == ProfileCategory::Goal)
        );
        assert!(
            selected
                .iter()
                .any(|item| item.category == ProfileCategory::Preference)
        );
    }

    #[test]
    fn profile_authority_removes_same_key_or_exact_value_from_memory() {
        let profile = vec![profile_item(
            "p1",
            "preference.response_style",
            "concise",
            ProfileCategory::Preference,
        )];
        assert!(ContextAssembler::memory_is_covered_by_profile(
            &memory("preference.response_style", "different"),
            &profile,
        ));
        assert!(ContextAssembler::memory_is_covered_by_profile(
            &memory("legacy-key", "concise"),
            &profile,
        ));
        assert!(!ContextAssembler::memory_is_covered_by_profile(
            &memory("project-decision", "use SQLite"),
            &profile,
        ));
    }
}
