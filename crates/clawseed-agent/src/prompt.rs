//! Prompt building for the agent.
//!
//! The system prompt is assembled from sections (identity, tools, safety, etc.).
//! In clawseed-agent the prompt builder is minimal — no skills, identity,
//! or security policy injection.

use anyhow::Result;
use std::fmt::Write;
use std::path::Path;
use clawseed_api::tool::Tool;
use chrono::{Datelike, Timelike};

/// Context for building the system prompt.
pub struct PromptContext<'a> {
    pub workspace_dir: &'a Path,
    pub model_name: &'a str,
    pub tools: &'a [Box<dyn Tool>],
    pub dispatcher_instructions: &'a str,
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
                Box::new(ToolsSection),
                Box::new(SafetySection),
                Box::new(WorkspaceSection),
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

// Prompt sections

pub struct ToolsSection;
pub struct SafetySection;
pub struct WorkspaceSection;
pub struct DateTimeSection;

impl PromptSection for ToolsSection {
    fn name(&self) -> &str { "tools" }

    fn build(&self, ctx: &PromptContext<'_>) -> Result<String> {
        let mut out = String::from("## Tools\n\n");
        for tool in ctx.tools {
            let _ = writeln!(
                out,
                "- **{}**: {}\n  Parameters: `{}`",
                tool.name(),
                tool.description(),
                tool.parameters_schema()
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
    fn name(&self) -> &str { "safety" }

    fn build(&self, _ctx: &PromptContext<'_>) -> Result<String> {
        Ok(
            "## Safety\n\n- Do not exfiltrate private data.\n\
             - Do not run destructive commands without asking.\n\
             - Prefer `trash` over `rm`.\n"
                .into(),
        )
    }
}

impl PromptSection for WorkspaceSection {
    fn name(&self) -> &str { "workspace" }

    fn build(&self, ctx: &PromptContext<'_>) -> Result<String> {
        Ok(format!(
            "## Workspace\n\nWorking directory: `{}`",
            ctx.workspace_dir.display()
        ))
    }
}

impl PromptSection for DateTimeSection {
    fn name(&self) -> &str { "datetime" }

    fn build(&self, _ctx: &PromptContext<'_>) -> Result<String> {
        let now = chrono::Local::now();
        let (year, month, day) = (now.year(), now.month(), now.day());
        let (hour, minute, second) = (now.hour(), now.minute(), now.second());
        let tz = now.format("%Z");

        Ok(format!(
            "## CRITICAL CONTEXT: CURRENT DATE & TIME\n\n\
             The following is the ABSOLUTE TRUTH regarding the current date and time. \
             Use this for all relative time calculations (e.g. \"last 7 days\").\n\n\
             Date: {year:04}-{month:02}-{day:02}\n\
             Time: {hour:02}:{minute:02}:{second:02} ({tz})\n\
             ISO 8601: {year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}{}",
            now.format("%:z")
        ))
    }
}
