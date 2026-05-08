//! Skill manifest parsing — manifest.toml and SKILL.md formats.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Parsed manifest.toml [skill] section.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillManifest {
    pub name: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub license: Option<String>,
    #[serde(default)]
    pub permissions: Vec<String>,
    #[serde(default)]
    pub triggers: Vec<String>,
}

impl Default for SkillManifest {
    fn default() -> Self {
        Self {
            name: String::new(),
            version: "0.1.0".into(),
            author: None,
            description: String::new(),
            category: None,
            tags: Vec::new(),
            license: None,
            permissions: Vec::new(),
            triggers: Vec::new(),
        }
    }
}

/// Parse a manifest.toml file and extract the [skill] section.
pub fn parse_manifest(path: &Path) -> Result<SkillManifest> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read manifest: {}", path.display()))?;

    let raw: toml::Value = toml::from_str(&content)
        .with_context(|| format!("Failed to parse manifest.toml: {}", path.display()))?;

    let skill_table = raw
        .get("skill")
        .cloned()
        .unwrap_or_else(|| toml::Value::Table(toml::map::Map::new()));

    let manifest: SkillManifest = skill_table.try_into().with_context(|| {
        format!(
            "Failed to deserialize [skill] section in {}",
            path.display()
        )
    })?;

    // Warn about deprecated [[tools]] sections
    if raw.get("tools").is_some() || raw.get("tools").is_some() {
        tracing::warn!(
            "manifest.toml at {} contains [[tools]] section which is deprecated and ignored. \
             Skills should declare permissions instead of defining tools.",
            path.display()
        );
    }

    Ok(manifest)
}

/// Parse a SKILL.md file: extract YAML frontmatter (if present) and body content.
///
/// Returns (optional manifest from frontmatter, body text).
pub fn parse_skill_md(path: &Path) -> Result<(Option<SkillManifest>, String)> {
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read SKILL.md: {}", path.display()))?;

    let (frontmatter, body) = extract_frontmatter(&content);
    let manifest = frontmatter.and_then(parse_frontmatter_as_manifest);

    Ok((manifest, body))
}

/// Extract YAML frontmatter from markdown content.
///
/// Returns (frontmatter_str, body_str). Frontmatter is the content between
/// the first two `---` delimiters.
fn extract_frontmatter(content: &str) -> (Option<&str>, String) {
    let trimmed = content.trim_start();
    if !trimmed.starts_with("---") {
        return (None, content.to_string());
    }

    // Find the closing ---
    let after_first = &trimmed[3..];
    if let Some(end_offset) = after_first.find("\n---") {
        let frontmatter = after_first[..end_offset].trim();
        let body_start = 3 + end_offset + 4; // skip opening --- + content + \n---
        let body = trimmed[body_start..].trim().to_string();
        return (Some(frontmatter), body);
    }

    (None, content.to_string())
}

/// Parse frontmatter string as a SkillManifest using simple key-value parsing.
///
/// Handles: name, description, version, author, tags, permissions, triggers.
/// Lists can be inline (`tags: [a, b]`) or block-style (`tags:\n  - a\n  - b`).
/// Returns None if no `name` is found (caller should fall back to manifest.toml).
fn parse_frontmatter_as_manifest(frontmatter: &str) -> Option<SkillManifest> {
    let mut manifest = SkillManifest::default();

    for line in frontmatter.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with('-') {
            continue;
        }

        if let Some((key, value)) = line.split_once(':') {
            let key = key.trim();
            let value = value.trim();

            // Skip keys whose value is empty (block list follows on next lines)
            if value.is_empty() {
                continue;
            }

            match key {
                "name" => manifest.name = unquote(value),
                "description" => manifest.description = unquote(value),
                "version" => manifest.version = unquote(value),
                "author" => manifest.author = Some(unquote(value)),
                _ => {}
            }
        }
    }

    // Parse list fields (inline and block-style)
    if let Some(tags) = parse_yaml_list(frontmatter, "tags") {
        manifest.tags = tags;
    }
    if let Some(permissions) = parse_yaml_list(frontmatter, "permissions") {
        manifest.permissions = permissions;
    }
    if let Some(triggers) = parse_yaml_list(frontmatter, "triggers") {
        manifest.triggers = triggers;
    }

    if manifest.name.is_empty() {
        return None;
    }

    Some(manifest)
}

/// Parse a YAML-style list under a key.
///
/// Supports two formats:
/// - Inline: `tags: [a, b, c]`
/// - Block: `tags:\n  - a\n  - b\n  - c`
fn parse_yaml_list(text: &str, key: &str) -> Option<Vec<String>> {
    let mut found_inline = None;
    let mut block_start_line = None;
    let key_prefix = format!("{key}:");

    for (i, line) in text.lines().enumerate() {
        let trimmed = line.trim();

        // Inline list: key: [a, b, c]
        if let Some(rest) = trimmed.strip_prefix(&key_prefix) {
            let rest = rest.trim();
            if rest.starts_with('[') && rest.ends_with(']') {
                let inner = &rest[1..rest.len() - 1];
                let items = inner
                    .split(',')
                    .map(|s| unquote(s.trim()))
                    .filter(|s| !s.is_empty())
                    .collect();
                found_inline = Some(items);
            } else if rest.is_empty() {
                // Block list header: `key:` with no inline value
                block_start_line = Some(i);
            }
        }
    }

    if let Some(items) = found_inline {
        return Some(items);
    }

    // Parse block-style list
    let start = block_start_line?;
    let mut items = Vec::new();
    for line in text.lines().skip(start + 1) {
        let trimmed = line.trim();
        if let Some(item) = trimmed.strip_prefix("- ") {
            items.push(unquote(item.trim()));
        } else if trimmed.is_empty() || trimmed.contains(':') {
            // Empty line or new key — block list ended
            break;
        }
    }

    if items.is_empty() { None } else { Some(items) }
}

/// Remove surrounding quotes from a value.
fn unquote(s: &str) -> String {
    let s = s.trim();
    if (s.starts_with('"') && s.ends_with('"')) || (s.starts_with('\'') && s.ends_with('\'')) {
        s[1..s.len() - 1].to_string()
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_manifest_toml() {
        let dir = tempfile::tempdir().unwrap();
        let manifest_path = dir.path().join("manifest.toml");
        std::fs::write(
            &manifest_path,
            r#"[skill]
name = "auto-coder"
version = "0.3.0"
description = "Autonomous code generation agent."
permissions = ["file_read", "file_write", "shell_exec"]
triggers = ["write code", "implement feature"]
tags = ["coding"]
"#,
        )
        .unwrap();

        let manifest = parse_manifest(&manifest_path).unwrap();
        assert_eq!(manifest.name, "auto-coder");
        assert_eq!(manifest.version, "0.3.0");
        assert_eq!(manifest.description, "Autonomous code generation agent.");
        assert_eq!(
            manifest.permissions,
            vec!["file_read", "file_write", "shell_exec"]
        );
        assert_eq!(manifest.triggers, vec!["write code", "implement feature"]);
        assert_eq!(manifest.tags, vec!["coding"]);
    }

    #[test]
    fn parse_skill_md_with_frontmatter() {
        let dir = tempfile::tempdir().unwrap();
        let skill_path = dir.path().join("SKILL.md");
        std::fs::write(
            &skill_path,
            r#"---
name: auto-coder
description: "Autonomous code generation agent."
version: 0.1.0
tags: [coding, official]
---

# Auto Coder

You are an autonomous coding agent.

## Workflow
1. Read code
2. Write code
3. Run tests
"#,
        )
        .unwrap();

        let (manifest, body) = parse_skill_md(&skill_path).unwrap();
        let manifest = manifest.unwrap();
        assert_eq!(manifest.name, "auto-coder");
        assert_eq!(manifest.description, "Autonomous code generation agent.");
        assert_eq!(manifest.tags, vec!["coding", "official"]);

        assert!(body.contains("# Auto Coder"));
        assert!(body.contains("## Workflow"));
        assert!(!body.contains("---"));
    }

    #[test]
    fn parse_skill_md_without_frontmatter() {
        let dir = tempfile::tempdir().unwrap();
        let skill_path = dir.path().join("SKILL.md");
        std::fs::write(
            &skill_path,
            r#"# Simple Skill

Just instructions, no frontmatter.
"#,
        )
        .unwrap();

        let (manifest, body) = parse_skill_md(&skill_path).unwrap();
        assert!(manifest.is_none());
        assert!(body.contains("# Simple Skill"));
    }

    #[test]
    fn extract_frontmatter_no_delimiters() {
        let (fm, body) = extract_frontmatter("Hello world");
        assert!(fm.is_none());
        assert_eq!(body, "Hello world");
    }

    #[test]
    fn extract_frontmatter_with_delimiters() {
        let content = "---\nname: test\n---\nBody text";
        let (fm, body) = extract_frontmatter(content);
        assert_eq!(fm.unwrap(), "name: test");
        assert_eq!(body, "Body text");
    }

    #[test]
    fn unquote_strings() {
        assert_eq!(unquote(r#""hello""#), "hello");
        assert_eq!(unquote("'hello'"), "hello");
        assert_eq!(unquote("hello"), "hello");
    }

    #[test]
    fn parse_frontmatter_permissions_and_triggers() {
        let dir = tempfile::tempdir().unwrap();
        let skill_path = dir.path().join("SKILL.md");
        std::fs::write(
            &skill_path,
            r#"---
name: auto-coder
description: "Code generation agent."
permissions: [file_read, shell_exec]
triggers: [write code, implement feature]
---

# Auto Coder
"#,
        )
        .unwrap();

        let (manifest, _) = parse_skill_md(&skill_path).unwrap();
        let manifest = manifest.unwrap();
        assert_eq!(manifest.permissions, vec!["file_read", "shell_exec"]);
        assert_eq!(manifest.triggers, vec!["write code", "implement feature"]);
    }

    #[test]
    fn parse_frontmatter_block_list() {
        let dir = tempfile::tempdir().unwrap();
        let skill_path = dir.path().join("SKILL.md");
        std::fs::write(
            &skill_path,
            r#"---
name: auto-coder
description: Code agent.
permissions:
  - file_read
  - shell_exec
triggers:
  - write code
  - implement feature
---

# Auto Coder
"#,
        )
        .unwrap();

        let (manifest, _) = parse_skill_md(&skill_path).unwrap();
        let manifest = manifest.unwrap();
        assert_eq!(manifest.permissions, vec!["file_read", "shell_exec"]);
        assert_eq!(manifest.triggers, vec!["write code", "implement feature"]);
    }
}
