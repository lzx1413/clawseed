//! Skill system — data structures, rendering, and permission checking.
//!
//! Skills are workflow orchestrators (not tools). The skill index is always
//! visible in the system prompt; full skill content is loaded on demand via
//! the `Skill` tool and injected into the system prompt.

pub mod builtin;
pub mod loader;
pub mod manifest;

pub use loader::*;
pub use manifest::*;

use std::fmt::Write;
use std::path::PathBuf;

/// Compact entry for the skill index (~30-50 tokens per skill).
#[derive(Debug, Clone)]
pub struct SkillIndexEntry {
    pub name: String,
    pub description: String,
    pub version: String,
    pub trigger_phrases: Vec<String>,
    pub permissions: Vec<String>,
}

/// Full skill definition (loaded on demand).
#[derive(Debug, Clone)]
pub struct Skill {
    pub name: String,
    pub description: String,
    pub version: String,
    pub author: Option<String>,
    pub tags: Vec<String>,
    pub permissions: Vec<String>,
    /// SKILL.md body content (after frontmatter).
    pub content: String,
    /// Directory where the skill was found.
    pub location: PathBuf,
}

/// An active skill in the agent's context.
#[derive(Debug, Clone)]
pub struct ActiveSkill {
    pub skill: Skill,
}

/// Map permission identifiers to clawseed tool names.
const PERMISSION_MAP: &[(&str, &[&str])] = &[
    ("file_read", &["file_read"]),
    ("file_write", &["file_write", "file_edit"]),
    ("shell_exec", &["shell"]),
    ("web_search", &["web_search"]),
    ("web_fetch", &["web_fetch"]),
    ("http_request", &["http_request"]),
    (
        "memory",
        &[
            "memory_store",
            "memory_recall",
            "memory_export",
            "memory_forget",
            "memory_purge",
        ],
    ),
    ("knowledge", &["knowledge"]),
    ("calculator", &["calculator"]),
    ("git", &["git"]),
    (
        "cron",
        &[
            "cron_add",
            "cron_list",
            "cron_remove",
            "cron_run",
            "cron_runs",
            "cron_update",
        ],
    ),
    ("backup", &["backup"]),
    ("glob_search", &["glob_search"]),
    ("content_search", &["content_search"]),
    ("llm_task", &["llm_task"]),
    ("pdf_read", &["pdf_read"]),
];

/// Check skill permissions against available tool names.
pub fn check_permissions(skill: &Skill, available_tool_names: &[String]) -> Result<(), String> {
    for perm in &skill.permissions {
        let fallback = [perm.as_str()];
        let allowed_names = PERMISSION_MAP
            .iter()
            .find(|(p, _)| p == perm)
            .map(|(_, names)| *names)
            .unwrap_or(&fallback);

        let satisfied = available_tool_names
            .iter()
            .any(|t| allowed_names.contains(&t.as_str()));

        if !satisfied {
            return Err(format!(
                "Skill '{}' requires permission '{}' but no matching tool is available.",
                skill.name, perm
            ));
        }
    }
    Ok(())
}

/// Render the skill index as XML for the system prompt.
fn index_summary(description: &str) -> String {
    // The index is for discovery. Full instructions and revision history belong
    // in the on-demand skill body, not in every model request.
    let first = description
        .trim()
        .split_inclusive(['。', '！', '？', '\n'])
        .next()
        .unwrap_or("");
    let first = first
        .split_once(". ")
        .map_or(first, |(sentence, _)| sentence);
    let mut summary = first.chars().take(160).collect::<String>();
    if first.chars().count() > 160 {
        summary.push('…');
    }
    summary
}

fn index_xml(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

pub fn render_skill_index(entries: &[SkillIndexEntry]) -> String {
    if entries.is_empty() {
        return String::new();
    }

    let mut s = String::from(
        "## Available Skills\n\n\
         Skill summaries are listed below. To use a skill, call `Skill({\"skill\": \"<name>\"})` \
         to load its full instructions into your system prompt. Summaries and historical memories are not the complete rules; load the relevant skill before applying its procedure.\n\n\
         <available_skills>\n",
    );

    let mut entries = entries.iter().collect::<Vec<_>>();
    entries.sort_by(|a, b| a.name.cmp(&b.name));
    for entry in entries {
        let triggers = entry.trigger_phrases.join(", ");
        let _ = writeln!(
            s,
            "  <skill name=\"{}\" triggers=\"{}\">",
            index_xml(&entry.name),
            index_xml(&triggers)
        );
        let _ = writeln!(s, "    {}", index_xml(&index_summary(&entry.description)));
        let _ = writeln!(s, "  </skill>");
    }

    s.push_str("</available_skills>");
    s
}

/// Render active skill content for the system prompt.
pub fn render_active_skills(skills: &[ActiveSkill]) -> String {
    if skills.is_empty() {
        return String::new();
    }

    let mut s = String::new();
    for skill in skills {
        let _ = writeln!(s, "<active_skill name=\"{}\">", skill.skill.name);
        s.push_str(&skill.skill.content);
        s.push('\n');
        let _ = writeln!(s, "</active_skill>");
        s.push('\n');
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn index_keeps_discovery_summary_without_revision_history() {
        let entry = SkillIndexEntry {
            name: "finance".into(),
            description: "行星系统 v1.8.7：仓位与风险管理。v1.8.6修订；旧预警ID123。".into(),
            version: "1.8.7".into(),
            trigger_phrases: vec!["持仓分析".into()],
            permissions: vec![],
        };
        let index = render_skill_index(&[entry]);
        assert!(index.contains("仓位与风险管理。"));
        assert!(index.contains("持仓分析"));
        assert!(!index.contains("旧预警"));
        assert!(!index.contains("v1.8.6"));
        assert!(index_summary(&"长".repeat(1000)).chars().count() <= 161);
    }

    #[test]
    fn render_empty_index() {
        assert!(render_skill_index(&[]).is_empty());
    }

    #[test]
    fn render_index_entries() {
        let entries = vec![SkillIndexEntry {
            name: "auto-coder".into(),
            description: "Autonomous code generation.".into(),
            version: "0.3.0".into(),
            trigger_phrases: vec!["write code".into(), "implement feature".into()],
            permissions: vec!["file_read".into()],
        }];
        let rendered = render_skill_index(&entries);
        assert!(rendered.contains("<skill name=\"auto-coder\""));
        assert!(rendered.contains("triggers=\"write code, implement feature\""));
        assert!(rendered.contains("Autonomous code generation."));
        assert!(rendered.contains("</available_skills>"));
    }

    #[test]
    fn render_empty_active_skills() {
        assert!(render_active_skills(&[]).is_empty());
    }

    #[test]
    fn render_active_skill_content() {
        let skills = vec![ActiveSkill {
            skill: Skill {
                name: "auto-coder".into(),
                description: "test".into(),
                version: "0.1.0".into(),
                author: None,
                tags: vec![],
                permissions: vec![],
                content: "Follow these steps...".into(),
                location: PathBuf::from("/tmp"),
            },
        }];
        let rendered = render_active_skills(&skills);
        assert!(rendered.contains("<active_skill name=\"auto-coder\">"));
        assert!(rendered.contains("Follow these steps..."));
        assert!(rendered.contains("</active_skill>"));
    }

    #[test]
    fn check_permissions_satisfied() {
        let skill = Skill {
            name: "test".into(),
            description: String::new(),
            version: "0.1.0".into(),
            author: None,
            tags: vec![],
            permissions: vec!["file_read".into(), "shell_exec".into()],
            content: String::new(),
            location: PathBuf::new(),
        };
        let tool_names = vec!["file_read".into(), "shell".into()];
        assert!(check_permissions(&skill, &tool_names).is_ok());
    }

    #[test]
    fn check_permissions_missing() {
        let skill = Skill {
            name: "test".into(),
            description: String::new(),
            version: "0.1.0".into(),
            author: None,
            tags: vec![],
            permissions: vec!["file_read".into(), "web_search".into()],
            content: String::new(),
            location: PathBuf::new(),
        };
        let tool_names = vec!["file_read".into()];
        let result = check_permissions(&skill, &tool_names);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("web_search"));
    }

    #[test]
    fn check_permissions_unknown_falls_through() {
        let skill = Skill {
            name: "test".into(),
            description: String::new(),
            version: "0.1.0".into(),
            author: None,
            tags: vec![],
            permissions: vec!["custom_tool".into()],
            content: String::new(),
            location: PathBuf::new(),
        };
        let tool_names = vec!["custom_tool".into()];
        assert!(check_permissions(&skill, &tool_names).is_ok());
    }
}
