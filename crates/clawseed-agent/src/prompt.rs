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
}

/// Trait for a prompt section.
pub trait PromptSection: Send + Sync {
    fn name(&self) -> &str;
    fn build(&self, ctx: &PromptContext<'_>) -> Result<String>;
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
                Box::new(ToolsSection),
                Box::new(SafetySection),
                Box::new(ToolHonestySection),
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
}

// ── Prompt sections ────────────────────────────────────────────────

pub struct DateTimeSection;
pub struct IdentitySection;
pub struct PlatformSection;
pub struct WorkspaceSection;
pub struct ToolsSection;
pub struct SafetySection;
pub struct ToolHonestySection;

impl PromptSection for DateTimeSection {
    fn name(&self) -> &str {
        "datetime"
    }

    fn build(&self, _ctx: &PromptContext<'_>) -> Result<String> {
        let now = chrono::Local::now();
        let (year, month, day) = (now.year(), now.month(), now.day());
        let (hour, minute, second) = (now.hour(), now.minute(), now.second());
        let tz = now.format("%Z");

        Ok(format!(
            "## CRITICAL CONTEXT: CURRENT DATE & TIME\n\n\
             The following is the ABSOLUTE TRUTH regarding the current date and time. \
             Use this for all relative time calculations.\n\n\
             Date: {year:04}-{month:02}-{day:02}\n\
             Time: {hour:02}:{minute:02}:{second:02} ({tz})"
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

        if crate::identity::is_aieos_configured(ctx.identity_config) {
            if let Ok(Some(aieos)) =
                crate::identity::load_aieos_identity(ctx.identity_config, ctx.workspace_dir)
            {
                let rendered = crate::identity::aieos_to_system_prompt(&aieos);
                if !rendered.is_empty() {
                    prompt.push_str(&rendered);
                    prompt.push_str("\n\n");
                    has_aieos = true;
                }
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
        };

        let prompt = builder.build(&ctx).unwrap();
        assert!(prompt.contains("## CRITICAL CONTEXT"));
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
        };

        let text = section.build(&ctx).unwrap();
        assert!(!text.contains("Do not run destructive commands without asking"));
    }
}
