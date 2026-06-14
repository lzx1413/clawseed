//! Prompt building for the agent.
//!
//! The system prompt is assembled from modular sections (identity, tools, safety,
//! etc.) via the `PromptSection` trait and `SystemPromptBuilder`.

use anyhow::Result;
use std::fmt::Write;
use std::path::Path;

use clawseed_config::schema::IdentityConfig;

use crate::security::AutonomyLevel;

/// Context for building the system prompt.
pub struct PromptContext<'a> {
    pub workspace_dir: &'a Path,
    pub model_name: &'a str,
    pub tool_specs: &'a [clawseed_api::tool::ToolSpec],
    pub dispatcher_instructions: &'a str,
    pub identity_config: &'a IdentityConfig,
    pub autonomy_level: AutonomyLevel,
    pub skill_index: &'a [crate::skills::SkillIndexEntry],
    pub active_skills: &'a [crate::skills::ActiveSkill],
    /// Stable Core memories injected into system prompt for LLM cache benefit.
    /// Empty when stable_memory_in_system_prompt is disabled.
    pub stable_core_memories: &'a [clawseed_api::memory_traits::MemoryEntry],
}

/// Classification of a prompt section for caching purposes.
/// Stable sections change rarely and should be cached as a prefix.
/// Dynamic sections change per-turn (e.g., datetime) and should not be cached.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheClass {
    Stable,
    Dynamic,
}

/// Trait for a prompt section.
pub trait PromptSection: Send + Sync {
    fn name(&self) -> &str;
    fn build(&self, ctx: &PromptContext<'_>) -> Result<String>;
    /// Whether this section belongs in the stable or dynamic partition.
    /// Default: Stable. Override to Dynamic for per-turn sections (e.g., datetime).
    fn cache_class(&self) -> CacheClass {
        CacheClass::Stable
    }
}

/// Default prompt builder.
#[derive(Default)]
pub struct SystemPromptBuilder {
    sections: Vec<Box<dyn PromptSection>>,
}

impl SystemPromptBuilder {
    pub fn with_defaults() -> Self {
        Self {
            sections: vec![
                // DateTimeSection removed: time comes from the [YYYY-MM-DD HH:MM:SS TZ]
                // prefix on each user message, keeping the system prompt 100% stable
                // for automatic prefix caching (DeepSeek, OpenAI, Groq, etc.).
                Box::new(IdentitySection),
                Box::new(PlatformSection),
                Box::new(WorkspaceSection),
                Box::new(StableMemorySection),
                Box::new(ToolsSection),
                Box::new(MemorySection),
                Box::new(SafetySection),
                Box::new(ToolHonestySection),
                Box::new(SkillsIndexSection),
                Box::new(ActiveSkillsSection),
            ],
        }
    }

    pub fn add_section(mut self, section: Box<dyn PromptSection>) -> Self {
        self.sections.push(section);
        self
    }

    pub fn build(&self, ctx: &PromptContext<'_>) -> Result<String> {
        let mut output = String::new();
        for section in &self.sections {
            let part = section.build(ctx)?;
            if part.trim().is_empty() {
                continue;
            }
            output.push_str(part.trim_end());
            output.push_str("\n\n");
        }
        Ok(output)
    }

    /// Build the system prompt partitioned into stable and dynamic portions.
    /// Stable sections change rarely and should be cached as a prefix by providers.
    /// Dynamic sections change per-turn (e.g., datetime) and are rebuilt each turn.
    /// The `full` field is `stable + "\n\n" + dynamic` for providers that don't
    /// support partitioning.
    ///
    /// When both stable and dynamic are present, a preamble is appended to the
    /// END of the stable buffer to bridge the semantic gap caused by moving
    /// datetime from position 0 to the end. The preamble is part of the stable
    /// block so it gets cached (it never changes), and so the simple
    /// `full = stable + "\n\n" + dynamic` invariant holds — providers can split
    /// the full content at exactly `stable.len()` without needing to know about
    /// the preamble.
    pub fn build_partitioned(&self, ctx: &PromptContext<'_>) -> Result<PartitionedSystemPrompt> {
        let mut stable_buf = String::new();
        let mut dynamic_buf = String::new();
        for section in &self.sections {
            let part = section.build(ctx)?;
            if part.trim().is_empty() {
                continue;
            }
            let trimmed = part.trim_end();
            match section.cache_class() {
                CacheClass::Stable => {
                    if !stable_buf.is_empty() {
                        stable_buf.push_str("\n\n");
                    }
                    stable_buf.push_str(trimmed);
                }
                CacheClass::Dynamic => {
                    if !dynamic_buf.is_empty() {
                        dynamic_buf.push_str("\n\n");
                    }
                    dynamic_buf.push_str(trimmed);
                }
            }
        }
        // With DateTimeSection removed, dynamic_buf is always empty.
        // The preamble and split logic are kept for future dynamic sections.
        let full = if dynamic_buf.is_empty() {
            stable_buf.clone()
        } else {
            format!("{}\n\n{}", stable_buf, dynamic_buf)
        };
        Ok(PartitionedSystemPrompt {
            stable: stable_buf,
            dynamic: dynamic_buf,
            full,
        })
    }
}

/// A system prompt split into cacheable stable prefix and per-turn dynamic suffix.
/// With DateTimeSection removed, `dynamic` is always empty and `full == stable`.
pub struct PartitionedSystemPrompt {
    /// Sections that rarely change — should be cached as a prefix by providers.
    /// Currently always equal to `full` since no dynamic sections exist.
    pub stable: String,
    /// Sections that change per-turn — currently always empty.
    pub dynamic: String,
    /// Full concatenated prompt. When dynamic is empty, `full == stable`.
    pub full: String,
}

// ── Prompt sections ────────────────────────────────────────────────

pub struct IdentitySection;
pub struct PlatformSection;
pub struct WorkspaceSection;
pub struct ToolsSection;
pub struct StableMemorySection;
pub struct SafetySection;
pub struct ToolHonestySection;
pub struct MemorySection;
pub struct SkillsIndexSection;
pub struct ActiveSkillsSection;

impl PromptSection for PlatformSection {
    fn name(&self) -> &str {
        "platform"
    }

    fn build(&self, _ctx: &PromptContext<'_>) -> Result<String> {
        let os = std::env::consts::OS;
        let arch = std::env::consts::ARCH;

        #[allow(unused_mut)]
        let mut out = format!("## Platform Environment\n\nOS: {os} ({arch})\n");

        #[cfg(feature = "android")]
        {
            out.push_str(
                "Shell: /system/bin/sh (mksh) — toybox commands only\n\
                 Available commands: ls, cat, grep, cp, mv, mkdir, rm, rmdir, chmod, chown, \
                 ps, id, wc, sort, head, tail, find, xargs, sed, awk, df, du, mount, \
                 ping, ifconfig, echo, env, date, stat, touch, ln, basename, dirname, \
                 readlink, realpath, md5sum, sha256sum, sleep, kill, pidof, uname, whoami\n\
                 IMPORTANT: Full Linux tools (bash, python, git, curl, apt, pip, npm, wget, \
                 ssh, systemctl) are NOT available. Only generate commands using the tools \
                 listed above.",
            );
        }

        Ok(out)
    }
}

impl PromptSection for IdentitySection {
    fn name(&self) -> &str {
        "identity"
    }

    fn build(&self, ctx: &PromptContext<'_>) -> Result<String> {
        let mut prompt = String::from("## Project Context\n\n");
        let mut has_aieos = false;

        if crate::identity::is_aieos_configured(ctx.identity_config)
            && let Ok(Some(aieos)) =
                crate::identity::load_aieos_identity(ctx.identity_config, ctx.workspace_dir)
        {
            let rendered = crate::identity::aieos_to_system_prompt(&aieos);
            if !rendered.is_empty() {
                prompt.push_str(&rendered);
                prompt.push_str("\n\n");
                has_aieos = true;
            }
        }

        if !has_aieos {
            prompt.push_str(
                "The following workspace files define your identity, behavior, and context.\n\n",
            );
        }

        let profile = crate::personality::load_personality(ctx.workspace_dir);
        let rendered = profile.render();
        if !rendered.trim().is_empty() {
            prompt.push_str(&rendered);
        } else if !has_aieos {
            return Ok(String::new());
        }

        Ok(prompt)
    }
}

impl PromptSection for WorkspaceSection {
    fn name(&self) -> &str {
        "workspace"
    }

    fn build(&self, ctx: &PromptContext<'_>) -> Result<String> {
        Ok(format!(
            "## Workspace\n\nWorking directory: `{}`",
            ctx.workspace_dir.display()
        ))
    }
}

impl PromptSection for ToolsSection {
    fn name(&self) -> &str {
        "tools"
    }

    fn build(&self, ctx: &PromptContext<'_>) -> Result<String> {
        let mut out = String::from("## Tools\n\n");
        for spec in ctx.tool_specs {
            let _ = writeln!(
                out,
                "- **{}**: {}\n  Parameters: `{}`",
                spec.name, spec.description, spec.parameters
            );
        }
        if !ctx.dispatcher_instructions.is_empty() {
            out.push('\n');
            out.push_str(ctx.dispatcher_instructions);
        }
        Ok(out)
    }
}

impl PromptSection for SafetySection {
    fn name(&self) -> &str {
        "safety"
    }

    fn build(&self, ctx: &PromptContext<'_>) -> Result<String> {
        let mut out = String::from("## Safety\n\n- Do not exfiltrate private data.\n");
        if ctx.autonomy_level != AutonomyLevel::Full {
            out.push_str(
                "- Do not run destructive commands without asking.\n\
                 - Do not bypass oversight or approval mechanisms.\n",
            );
        }
        out.push_str("- Prefer `trash` over `rm`.\n");
        Ok(out)
    }
}

impl PromptSection for ToolHonestySection {
    fn name(&self) -> &str {
        "tool_honesty"
    }

    fn build(&self, _ctx: &PromptContext<'_>) -> Result<String> {
        Ok("## CRITICAL: Tool Honesty\n\n\
             - NEVER fabricate, invent, or guess tool results.\n\
             - If a tool call fails, report the error — never make up data.\n\
             - When unsure, ask the user rather than guessing."
            .into())
    }
}

impl PromptSection for StableMemorySection {
    fn name(&self) -> &str {
        "stable_memory"
    }

    fn build(&self, ctx: &PromptContext<'_>) -> Result<String> {
        if ctx.stable_core_memories.is_empty() {
            return Ok(String::new());
        }
        let mut out = String::from("## Core Memories\n\n");
        out.push_str(
            "The following are your most important long-term memories. \
             These are always available to you.\n\n",
        );
        for entry in ctx.stable_core_memories {
            let _ = writeln!(out, "- **{}**: {}", entry.key, entry.content);
        }
        Ok(out)
    }
}

impl PromptSection for MemorySection {
    fn name(&self) -> &str {
        "memory"
    }

    fn build(&self, _ctx: &PromptContext<'_>) -> Result<String> {
        Ok("## Memory\n\n\
             You have a long-term memory system. Relevant memories are automatically recalled \
             and provided as context at the start of each turn.\n\
             - Use `memory_recall` to search for additional or more specific memories when the \
             auto-recalled context is insufficient.\n\
             - Use `memory_store` to save important facts, preferences, or context that the user \
             mentions or that seem important for future interactions."
            .into())
    }
}

impl PromptSection for SkillsIndexSection {
    fn name(&self) -> &str {
        "skills_index"
    }

    fn build(&self, ctx: &PromptContext<'_>) -> Result<String> {
        Ok(crate::skills::render_skill_index(ctx.skill_index))
    }
}

impl PromptSection for ActiveSkillsSection {
    fn name(&self) -> &str {
        "active_skills"
    }

    fn build(&self, ctx: &PromptContext<'_>) -> Result<String> {
        Ok(crate::skills::render_active_skills(ctx.active_skills))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builder_with_defaults_produces_all_sections() {
        let builder = SystemPromptBuilder::with_defaults();
        let identity_config = IdentityConfig::default();
        let ctx = PromptContext {
            workspace_dir: Path::new("/tmp/test"),
            model_name: "test-model",
            tool_specs: &[],
            dispatcher_instructions: "",
            identity_config: &identity_config,
            autonomy_level: AutonomyLevel::Full,
            skill_index: &[],
            active_skills: &[],
            stable_core_memories: &[],
        };

        let prompt = builder.build(&ctx).unwrap();
        // DateTimeSection is no longer in the system prompt — time comes from
        // the user message timestamp prefix instead.
        assert!(!prompt.contains("## CRITICAL CONTEXT"));
        assert!(!prompt.contains("Time:"));
        assert!(prompt.contains("## Platform Environment"));
        assert!(prompt.contains("## Workspace"));
        assert!(prompt.contains("## Tools"));
        assert!(prompt.contains("## Safety"));
        assert!(prompt.contains("## CRITICAL: Tool Honesty"));
    }

    #[test]
    fn safety_section_supervised_mode() {
        let section = SafetySection;
        let identity_config = IdentityConfig::default();
        let ctx = PromptContext {
            workspace_dir: Path::new("/tmp"),
            model_name: "test",
            tool_specs: &[],
            dispatcher_instructions: "",
            identity_config: &identity_config,
            autonomy_level: AutonomyLevel::Supervised,
            skill_index: &[],
            active_skills: &[],
            stable_core_memories: &[],
        };

        let text = section.build(&ctx).unwrap();
        assert!(text.contains("Do not run destructive commands without asking"));
    }

    #[test]
    fn safety_section_full_mode_omits_approval() {
        let section = SafetySection;
        let identity_config = IdentityConfig::default();
        let ctx = PromptContext {
            workspace_dir: Path::new("/tmp"),
            model_name: "test",
            tool_specs: &[],
            dispatcher_instructions: "",
            identity_config: &identity_config,
            autonomy_level: AutonomyLevel::Full,
            skill_index: &[],
            active_skills: &[],
            stable_core_memories: &[],
        };

        let text = section.build(&ctx).unwrap();
        assert!(!text.contains("Do not run destructive commands without asking"));
    }

    #[test]
    fn stable_memory_section_empty_returns_nothing() {
        let section = StableMemorySection;
        let identity_config = IdentityConfig::default();
        let ctx = PromptContext {
            workspace_dir: Path::new("/tmp"),
            model_name: "test",
            tool_specs: &[],
            dispatcher_instructions: "",
            identity_config: &identity_config,
            autonomy_level: AutonomyLevel::Full,
            skill_index: &[],
            active_skills: &[],
            stable_core_memories: &[],
        };

        let text = section.build(&ctx).unwrap();
        assert!(text.is_empty());
    }

    #[test]
    fn stable_memory_section_with_entries() {
        use clawseed_api::memory_traits::{MemoryCategory, MemoryEntry};
        let section = StableMemorySection;
        let identity_config = IdentityConfig::default();
        let entries = vec![MemoryEntry {
            id: "1".into(),
            key: "user_name".into(),
            content: "User prefers Rust".into(),
            category: MemoryCategory::Core,
            timestamp: "now".into(),
            session_id: None,
            score: None,
            namespace: "default".into(),
            importance: Some(0.9),
            superseded_by: None,
            embedding: None,
        }];
        let ctx = PromptContext {
            workspace_dir: Path::new("/tmp"),
            model_name: "test",
            tool_specs: &[],
            dispatcher_instructions: "",
            identity_config: &identity_config,
            autonomy_level: AutonomyLevel::Full,
            skill_index: &[],
            active_skills: &[],
            stable_core_memories: &entries,
        };

        let text = section.build(&ctx).unwrap();
        assert!(text.contains("## Core Memories"));
        assert!(text.contains("user_name"));
        assert!(text.contains("User prefers Rust"));
    }

    #[test]
    fn cache_class_all_sections_are_stable() {
        // All current sections default to Stable
        assert_eq!(IdentitySection.cache_class(), CacheClass::Stable);
        assert_eq!(PlatformSection.cache_class(), CacheClass::Stable);
        assert_eq!(WorkspaceSection.cache_class(), CacheClass::Stable);
        assert_eq!(StableMemorySection.cache_class(), CacheClass::Stable);
        assert_eq!(ToolsSection.cache_class(), CacheClass::Stable);
        assert_eq!(MemorySection.cache_class(), CacheClass::Stable);
        assert_eq!(SafetySection.cache_class(), CacheClass::Stable);
        assert_eq!(ToolHonestySection.cache_class(), CacheClass::Stable);
        assert_eq!(SkillsIndexSection.cache_class(), CacheClass::Stable);
        assert_eq!(ActiveSkillsSection.cache_class(), CacheClass::Stable);
    }

    fn make_ctx() -> PromptContext<'static> {
        // Use a leaked box to get static references for test purposes
        static IDENTITY: std::sync::OnceLock<IdentityConfig> = std::sync::OnceLock::new();
        let identity_config = IDENTITY.get_or_init(IdentityConfig::default);
        PromptContext {
            workspace_dir: Path::new("/tmp/test"),
            model_name: "test-model",
            tool_specs: &[],
            dispatcher_instructions: "",
            identity_config,
            autonomy_level: AutonomyLevel::Full,
            skill_index: &[],
            active_skills: &[],
            stable_core_memories: &[],
        }
    }

    #[test]
    fn build_partitioned_all_stable_no_dynamic() {
        // With DateTimeSection removed, the entire prompt is stable (no dynamic).
        let builder = SystemPromptBuilder::with_defaults();
        let ctx = make_ctx();
        let result = builder.build_partitioned(&ctx).unwrap();
        // stable should be non-empty, dynamic should be empty
        assert!(!result.stable.is_empty());
        assert!(result.dynamic.is_empty());
        // When dynamic is empty, full == stable (no preamble appended)
        assert_eq!(result.full, result.stable);
    }

    #[test]
    fn build_partitioned_stable_contains_all_sections() {
        // With no dynamic sections, the entire prompt is stable.
        let builder = SystemPromptBuilder::with_defaults();
        let ctx = make_ctx();
        let partitioned = builder.build_partitioned(&ctx).unwrap();
        // All sections should be in stable (no datetime section anymore)
        assert!(partitioned.stable.contains("## Platform Environment"));
        assert!(partitioned.stable.contains("## Workspace"));
        // No dynamic content
        assert!(partitioned.dynamic.is_empty());
        // full == stable exactly
        assert!(partitioned.full.starts_with(&partitioned.stable));
        assert_eq!(partitioned.full.len(), partitioned.stable.len());
    }

    #[test]
    fn build_partitioned_no_preamble_when_dynamic_empty() {
        // When there are no dynamic sections, no preamble should be appended.
        struct OnlyStable;
        impl PromptSection for OnlyStable {
            fn name(&self) -> &str {
                "only_stable"
            }
            fn build(&self, _ctx: &PromptContext<'_>) -> Result<String> {
                Ok("STABLE CONTENT".into())
            }
        }
        let builder = SystemPromptBuilder::default().add_section(Box::new(OnlyStable));
        let ctx = make_ctx();
        let p = builder.build_partitioned(&ctx).unwrap();
        assert_eq!(p.stable, "STABLE CONTENT");
        assert_eq!(p.dynamic, "");
        assert_eq!(p.full, "STABLE CONTENT");
    }
}
