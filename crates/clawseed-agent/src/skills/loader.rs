//! Skill discovery and loading — scan skill roots, load index and full skills.

use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use super::manifest::{SkillManifest, parse_manifest, parse_skill_md};
use super::{Skill, SkillIndexEntry};

/// Return the ordered list of skill root directories.
///
/// Priority (highest first):
/// 1. `<workspace>/.clawseed/skills/`
/// 2. `<workspace>/.claude/skills/`   (Claude Code compat)
/// 3. `~/.clawseed/skills/`
/// 4. `~/.claude/skills/`              (Claude Code compat)
pub fn skill_roots(workspace_dir: &Path) -> Vec<PathBuf> {
    let mut roots = Vec::new();

    // Workspace-level
    roots.push(workspace_dir.join(".clawseed").join("skills"));
    roots.push(workspace_dir.join(".claude").join("skills"));

    // User-level
    if let Some(home) = dirs_home() {
        roots.push(home.join(".clawseed").join("skills"));
        roots.push(home.join(".claude").join("skills"));
    }

    roots
}

/// Get the user's home directory.
fn dirs_home() -> Option<PathBuf> {
    std::env::var("HOME")
        .ok()
        .map(PathBuf::from)
        .or_else(|| std::env::var("USERPROFILE").ok().map(PathBuf::from))
}

/// Scan skill roots in priority order, read manifest.toml from each,
/// return `Vec<SkillIndexEntry>`. Does NOT read SKILL.md content.
///
/// On name collision (by effective skill name, not directory name),
/// higher-priority root wins (earlier in `skill_roots()`).
pub fn load_skill_index(workspace_dir: &Path) -> Vec<SkillIndexEntry> {
    load_skill_index_with_roots(workspace_dir, &[])
}

/// Scan skill roots plus extra roots, read manifest.toml from each,
/// return `Vec<SkillIndexEntry>`. Does NOT read SKILL.md content.
///
/// Extra roots are appended after the default roots (lowest priority).
pub fn load_skill_index_with_roots(
    workspace_dir: &Path,
    extra_roots: &[String],
) -> Vec<SkillIndexEntry> {
    let mut seen: HashMap<String, SkillIndexEntry> = HashMap::new();

    let mut all_roots = skill_roots(workspace_dir);
    for extra in extra_roots {
        all_roots.push(PathBuf::from(extra));
    }

    for root in &all_roots {
        if !root.is_dir() {
            continue;
        }

        let entries = match std::fs::read_dir(root) {
            Ok(e) => e,
            Err(_) => continue,
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }

            let dir_name = match path.file_name().and_then(|n| n.to_str()) {
                Some(n) => n.to_string(),
                None => continue,
            };

            if let Some(index_entry) = build_index_entry(&path, &dir_name) {
                // Dedup by effective skill name (from manifest), not directory name
                if seen.contains_key(&index_entry.name) {
                    continue;
                }
                seen.insert(index_entry.name.clone(), index_entry);
            }
        }
    }

    let mut result: Vec<SkillIndexEntry> = seen.into_values().collect();
    // Sort by name for deterministic output
    result.sort_by(|a, b| a.name.cmp(&b.name));
    result
}

/// Build a SkillIndexEntry from a skill directory by reading manifest.toml
/// (falling back to SKILL.md frontmatter).
fn build_index_entry(skill_dir: &Path, fallback_name: &str) -> Option<SkillIndexEntry> {
    let manifest_path = skill_dir.join("manifest.toml");
    let skill_md_path = skill_dir.join("SKILL.md");

    // Try manifest.toml first
    if manifest_path.exists()
        && let Ok(manifest) = parse_manifest(&manifest_path)
    {
        let name = if manifest.name.is_empty() {
            fallback_name.to_string()
        } else {
            manifest.name
        };
        return Some(SkillIndexEntry {
            name,
            description: manifest.description,
            version: manifest.version,
            trigger_phrases: manifest.triggers,
            permissions: manifest.permissions,
        });
    }

    // Fall back to SKILL.md frontmatter
    if skill_md_path.exists()
        && let Ok((Some(manifest), _)) = parse_skill_md(&skill_md_path)
    {
        let name = if manifest.name.is_empty() {
            fallback_name.to_string()
        } else {
            manifest.name
        };
        return Some(SkillIndexEntry {
            name,
            description: manifest.description,
            version: manifest.version,
            trigger_phrases: manifest.triggers,
            permissions: manifest.permissions,
        });
    }

    // SKILL.md exists but has no frontmatter — use directory name
    if skill_md_path.exists() {
        return Some(SkillIndexEntry {
            name: fallback_name.to_string(),
            description: String::new(),
            version: String::new(),
            trigger_phrases: Vec::new(),
            permissions: Vec::new(),
        });
    }

    None
}

/// Load a full skill by name. Searches roots in priority order.
///
/// The `name` parameter is the effective skill name (from manifest or
/// frontmatter), not necessarily the directory name. This function scans
/// each root's subdirectories, reads their manifests, and matches by
/// effective name.
///
/// Reads manifest.toml + SKILL.md, merges metadata (manifest takes precedence),
/// and returns the full `Skill`.
pub fn load_skill_by_name(name: &str, workspace_dir: &Path) -> Result<Skill> {
    load_skill_by_name_with_roots(name, workspace_dir, &[])
}

/// Load a full skill by name with extra roots.
pub fn load_skill_by_name_with_roots(
    name: &str,
    workspace_dir: &Path,
    extra_roots: &[String],
) -> Result<Skill> {
    let mut all_roots = skill_roots(workspace_dir);
    for extra in extra_roots {
        all_roots.push(PathBuf::from(extra));
    }

    for root in &all_roots {
        if !root.is_dir() {
            continue;
        }

        // First try direct directory match (common case: dir name == skill name)
        let direct_dir = root.join(name);
        if direct_dir.is_dir()
            && let Ok(skill) = load_skill_from_dir(&direct_dir, name)
            && skill.name == name
        {
            return Ok(skill);
        }

        // Fallback: scan all subdirectories and match by effective name
        let entries = match std::fs::read_dir(root) {
            Ok(e) => e,
            Err(_) => continue,
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }

            if path == direct_dir {
                continue; // Already tried above
            }

            if let Ok(skill) = load_skill_from_dir(&path, "")
                && skill.name == name
            {
                return Ok(skill);
            }
        }
    }

    anyhow::bail!("Skill '{}' not found in any skill root", name)
}

/// Load a full skill from its directory.
///
/// Returns an error if the directory contains neither manifest.toml nor SKILL.md.
fn load_skill_from_dir(skill_dir: &Path, fallback_name: &str) -> Result<Skill> {
    let manifest_path = skill_dir.join("manifest.toml");
    let skill_md_path = skill_dir.join("SKILL.md");

    if !manifest_path.exists() && !skill_md_path.exists() {
        anyhow::bail!(
            "Skill directory {} contains neither manifest.toml nor SKILL.md",
            skill_dir.display()
        );
    }

    // Parse manifest.toml if it exists
    let manifest = if manifest_path.exists() {
        Some(
            parse_manifest(&manifest_path)
                .with_context(|| format!("Failed to parse manifest in {}", skill_dir.display()))?,
        )
    } else {
        None
    };

    // Parse SKILL.md if it exists
    let (md_manifest, content) = if skill_md_path.exists() {
        parse_skill_md(&skill_md_path)
            .with_context(|| format!("Failed to parse SKILL.md in {}", skill_dir.display()))?
    } else {
        (None, String::new())
    };

    // Merge: manifest.toml takes precedence, fall back to SKILL.md frontmatter
    let effective = merge_manifests(manifest.as_ref(), md_manifest.as_ref(), fallback_name);

    Ok(Skill {
        name: effective.name,
        description: effective.description,
        version: effective.version,
        author: effective.author,
        tags: effective.tags,
        permissions: effective.permissions,
        content,
        location: skill_dir.to_path_buf(),
    })
}

/// Merge manifest data: manifest.toml takes precedence over SKILL.md frontmatter.
fn merge_manifests(
    toml_manifest: Option<&SkillManifest>,
    md_manifest: Option<&SkillManifest>,
    fallback_name: &str,
) -> SkillManifest {
    let mut result = SkillManifest {
        name: fallback_name.to_string(),
        ..Default::default()
    };

    // Apply SKILL.md frontmatter first (lower priority)
    if let Some(md) = md_manifest {
        if !md.name.is_empty() {
            result.name = md.name.clone();
        }
        if !md.description.is_empty() {
            result.description = md.description.clone();
        }
        if md.version != "0.1.0" {
            result.version = md.version.clone();
        }
        if md.author.is_some() {
            result.author = md.author.clone();
        }
        if !md.tags.is_empty() {
            result.tags = md.tags.clone();
        }
        if !md.permissions.is_empty() {
            result.permissions = md.permissions.clone();
        }
        if !md.triggers.is_empty() {
            result.triggers = md.triggers.clone();
        }
    }

    // Apply manifest.toml (higher priority, overwrites)
    if let Some(toml) = toml_manifest {
        if !toml.name.is_empty() {
            result.name = toml.name.clone();
        }
        if !toml.description.is_empty() {
            result.description = toml.description.clone();
        }
        if toml.version != "0.1.0" {
            result.version = toml.version.clone();
        }
        if toml.author.is_some() {
            result.author = toml.author.clone();
        }
        if !toml.tags.is_empty() {
            result.tags = toml.tags.clone();
        }
        if !toml.permissions.is_empty() {
            result.permissions = toml.permissions.clone();
        }
        if !toml.triggers.is_empty() {
            result.triggers = toml.triggers.clone();
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_skill_index_from_temp_dir() {
        let dir = tempfile::tempdir().unwrap();
        let workspace = dir.path();

        // Create a skill directory with manifest.toml
        let skill_dir = workspace
            .join(".clawseed")
            .join("skills")
            .join("auto-coder");
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(
            skill_dir.join("manifest.toml"),
            r#"[skill]
name = "auto-coder"
description = "Autonomous code generation."
permissions = ["file_read", "shell_exec"]
triggers = ["write code"]
"#,
        )
        .unwrap();

        let index = load_skill_index(workspace);
        assert_eq!(index.len(), 1);
        assert_eq!(index[0].name, "auto-coder");
        assert_eq!(index[0].description, "Autonomous code generation.");
        assert_eq!(index[0].trigger_phrases, vec!["write code"]);
    }

    #[test]
    fn load_skill_by_name_from_temp_dir() {
        let dir = tempfile::tempdir().unwrap();
        let workspace = dir.path();

        let skill_dir = workspace
            .join(".clawseed")
            .join("skills")
            .join("auto-coder");
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(
            skill_dir.join("manifest.toml"),
            r#"[skill]
name = "auto-coder"
version = "0.3.0"
description = "Autonomous code generation."
permissions = ["file_read", "shell_exec"]
"#,
        )
        .unwrap();
        std::fs::write(
            skill_dir.join("SKILL.md"),
            r#"# Auto Coder

Follow these steps.
"#,
        )
        .unwrap();

        let skill = load_skill_by_name("auto-coder", workspace).unwrap();
        assert_eq!(skill.name, "auto-coder");
        assert_eq!(skill.version, "0.3.0");
        assert!(skill.content.contains("# Auto Coder"));
    }

    #[test]
    fn load_skill_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let result = load_skill_by_name("nonexistent", dir.path());
        assert!(result.is_err());
    }

    #[test]
    fn priority_workspace_over_home() {
        let dir = tempfile::tempdir().unwrap();
        let workspace = dir.path();

        // Workspace-level skill
        let ws_skill = workspace.join(".clawseed").join("skills").join("my-skill");
        std::fs::create_dir_all(&ws_skill).unwrap();
        std::fs::write(
            ws_skill.join("manifest.toml"),
            r#"[skill]
name = "my-skill"
description = "Workspace version"
"#,
        )
        .unwrap();

        let index = load_skill_index(workspace);
        assert_eq!(index.len(), 1);
        assert_eq!(index[0].description, "Workspace version");
    }

    #[test]
    fn manifest_name_differs_from_directory_name() {
        let dir = tempfile::tempdir().unwrap();
        let workspace = dir.path();

        // Directory is "my-skill-dir" but manifest says name = "auto-coder"
        let skill_dir = workspace
            .join(".clawseed")
            .join("skills")
            .join("my-skill-dir");
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(
            skill_dir.join("manifest.toml"),
            r#"[skill]
name = "auto-coder"
description = "Skill with different name"
"#,
        )
        .unwrap();
        std::fs::write(skill_dir.join("SKILL.md"), "# Auto Coder\n").unwrap();

        // Index should use manifest name, not directory name
        let index = load_skill_index(workspace);
        assert_eq!(index.len(), 1);
        assert_eq!(index[0].name, "auto-coder");

        // Activation by effective name should work
        let skill = load_skill_by_name("auto-coder", workspace).unwrap();
        assert_eq!(skill.name, "auto-coder");
    }

    #[test]
    fn duplicate_effective_name_deduped() {
        let dir = tempfile::tempdir().unwrap();
        let workspace = dir.path();

        // Two directories, both declare name = "same-name"
        let dir1 = workspace.join(".clawseed").join("skills").join("skill-a");
        std::fs::create_dir_all(&dir1).unwrap();
        std::fs::write(
            dir1.join("manifest.toml"),
            r#"[skill]
name = "same-name"
description = "Version A"
"#,
        )
        .unwrap();

        let dir2 = workspace.join(".clawseed").join("skills").join("skill-b");
        std::fs::create_dir_all(&dir2).unwrap();
        std::fs::write(
            dir2.join("manifest.toml"),
            r#"[skill]
name = "same-name"
description = "Version B"
"#,
        )
        .unwrap();

        // Should only appear once — first directory found wins
        let index = load_skill_index(workspace);
        let matching: Vec<_> = index.iter().filter(|e| e.name == "same-name").collect();
        assert_eq!(matching.len(), 1);
    }

    #[test]
    fn extra_roots_appended() {
        let dir = tempfile::tempdir().unwrap();
        let extra = tempfile::tempdir().unwrap();

        // Skill in extra root
        let skill_dir = extra.path().join("extra-skill");
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(
            skill_dir.join("manifest.toml"),
            r#"[skill]
name = "extra-skill"
description = "From extra root"
"#,
        )
        .unwrap();

        let index =
            load_skill_index_with_roots(dir.path(), &[extra.path().to_string_lossy().to_string()]);
        assert_eq!(index.len(), 1);
        assert_eq!(index[0].name, "extra-skill");
    }
}
