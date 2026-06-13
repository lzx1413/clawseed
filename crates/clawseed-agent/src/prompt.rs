//! Prompt building for the agent.
//!
//! The system prompt is assembled from modular sections (identity, tools, safety,
//! etc.) via the `PromptSection` trait and `SystemPromptBuilder`.

use anyhow::Result;
use chrono::{Datelike, Timelike};
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
                Box::new(DateTimeSection),
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
        // Append the preamble to the END of the stable buffer when both halves
        // are non-empty. This keeps the preamble inside the cached prefix and
        // makes `full = stable + "\n\n" + dynamic` byte-exact.
        if !stable_buf.is_empty() && !dynamic_buf.is_empty() {
            stable_buf.push_str("\n\n");
            stable_buf.push_str(DYNAMIC_PREAMBLE);
        }
        let full = if stable_buf.is_empty() {
            dynamic_buf.clone()
        } else if dynamic_buf.is_empty() {
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

    /// Build only the dynamic (per-turn) sections. Used by
    /// `refresh_dynamic_system_content` to avoid rebuilding stable sections.
    pub fn build_dynamic(&self, ctx: &PromptContext<'_>) -> Result<String> {
        let mut output = String::new();
        for section in &self.sections {
            if section.cache_class() != CacheClass::Dynamic {
                continue;
            }
            let part = section.build(ctx)?;
            if part.trim().is_empty() {
                continue;
            }
            if !output.is_empty() {
                output.push_str("\n\n");
            }
            output.push_str(part.trim_end());
        }
        Ok(output)
    }
}

/// A system prompt split into cacheable stable prefix and per-turn dynamic suffix.
pub struct PartitionedSystemPrompt {
    /// Sections that rarely change — should be cached as a prefix by providers.
    /// When both halves are non-empty, this includes a trailing preamble that
    /// bridges the semantic gap caused by moving datetime to the end.
    pub stable: String,
    /// Sections that change per-turn — not cached, rebuilt each turn.
    pub dynamic: String,
    /// Full concatenated prompt: `stable + "\n\n" + dynamic` exactly. The
    /// preamble (if any) is already inside `stable`. Providers that support
    /// partitioning split this at `stable.len()` to recover the two halves.
    pub full: String,
}

/// Preamble appended to the stable buffer when dynamic sections (datetime) move
/// to the end of the prompt. Tells the model that the time below applies to all
/// instructions above. Lives inside the stable block so it's part of the
/// cacheable prefix.
pub const DYNAMIC_PREAMBLE: &str =
    "⚠️ THE CURRENT TIME BELOW APPLIES TO ALL ABOVE INSTRUCTIONS.";

// ── Prompt sections ────────────────────────────────────────────────

pub struct DateTimeSection;
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

impl PromptSection for DateTimeSection {
    fn name(&self) -> &str {
        "datetime"
    }

    fn cache_class(&self) -> CacheClass {
        CacheClass::Dynamic
    }

    fn build(&self, _ctx: &PromptContext<'_>) -> Result<String> {
        let now = chrono::Local::now();
        let (year, month, day) = (now.year(), now.month(), now.day());
        let (hour, minute) = (now.hour(), now.minute());
        let tz = now.format("%Z");

        Ok(format!(
            "## CRITICAL CONTEXT: CURRENT DATE & TIME\n\n\
             The following is the ABSOLUTE TRUTH regarding the current date and time. \
             Use this for all relative time calculations.\n\n\
             Date: {year:04}-{month:02}-{day:02}\n\
             Time: {hour:02}:{minute:02} ({tz})"
        ))
    }
}

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
        assert!(prompt.contains("## CRITICAL CONTEXT"));
        assert!(prompt.contains("Time:"));
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
    fn datetime_section_minute_precision() {
        let section = DateTimeSection;
        let ctx = make_ctx();
        let text = section.build(&ctx).unwrap();
        // Should contain "Time:" with HH:MM format (minute precision, no seconds)
        assert!(text.contains("Time:"));
        // Should NOT contain seconds ":SS" pattern after the minute part
        // (the line should be "Time: HH:MM (TZ)" not "Time: HH:MM:SS (TZ)")
        let time_line = text.lines().find(|l| l.starts_with("Time:")).unwrap();
        // After "Time:", expect " HH:MM" then space + timezone, no second colon for seconds
        let after_label = &time_line["Time:".len()..];
        // There should be exactly one ":" in the time portion (HH:MM), not two (HH:MM:SS)
        // But timezone may contain colons like "UTC+08:00", so count colons before the timezone
        let time_part = after_label.split('(').next().unwrap().trim();
        assert_eq!(time_part.matches(':').count(), 1); // HH:MM only
        // Should contain "Date:" with YYYY-MM-DD format
        assert!(text.contains("Date:"));
    }

    #[test]
    fn cache_class_default_is_stable() {
        // All sections except DateTimeSection should default to Stable
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

    #[test]
    fn datetime_cache_class_is_dynamic() {
        assert_eq!(DateTimeSection.cache_class(), CacheClass::Dynamic);
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
    fn build_partitioned_nonempty() {
        let builder = SystemPromptBuilder::with_defaults();
        let ctx = make_ctx();
        let result = builder.build_partitioned(&ctx).unwrap();
        // Both stable and dynamic should be non-empty (datetime is dynamic, rest is stable)
        assert!(!result.stable.is_empty());
        assert!(!result.dynamic.is_empty());
        // Preamble lives at the END of stable, so full = stable + "\n\n" + dynamic exactly.
        assert!(result.stable.ends_with(DYNAMIC_PREAMBLE));
        assert_eq!(result.full, format!("{}\n\n{}", result.stable, result.dynamic));
    }

    #[test]
    fn build_partitioned_stable_before_dynamic() {
        // build_partitioned() places stable sections before dynamic sections
        // so Anthropic's prefix caching can cache the stable block.
        let builder = SystemPromptBuilder::with_defaults();
        let ctx = make_ctx();
        let partitioned = builder.build_partitioned(&ctx).unwrap();
        // Stable content should contain identity/platform/workspace (not datetime)
        assert!(partitioned.stable.contains("## Platform Environment"));
        assert!(partitioned.stable.contains("## Workspace"));
        // Stable ends with the preamble that bridges to the dynamic section.
        assert!(partitioned.stable.contains(DYNAMIC_PREAMBLE));
        // Dynamic content should contain datetime
        assert!(partitioned.dynamic.contains("## CRITICAL CONTEXT"));
        // Providers can split full at exactly stable.len() to recover the two halves.
        assert!(partitioned.full.starts_with(&partitioned.stable));
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
        assert!(!p.stable.contains(DYNAMIC_PREAMBLE));
    }

    #[test]
    fn build_dynamic_only_dynamic_sections() {
        let builder = SystemPromptBuilder::with_defaults();
        let ctx = make_ctx();
        let dynamic = builder.build_dynamic(&ctx).unwrap();
        // Only DateTimeSection is Dynamic, so dynamic should contain datetime only
        assert!(dynamic.contains("## CRITICAL CONTEXT"));
        assert!(!dynamic.contains("## Platform Environment"));
        assert!(!dynamic.contains("## Workspace"));
    }
}
