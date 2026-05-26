//! Built-in skills — hardcoded skill definitions that are seeded into every
//! workspace on first use, so they're available immediately after installation.

use std::path::Path;

/// A built-in skill definition: manifest.toml + SKILL.md content.
struct BuiltinSkill {
    name: &'static str,
    manifest: &'static str,
    skill_md: &'static str,
}

const BUILTIN_SKILLS: &[BuiltinSkill] = &[BuiltinSkill {
    name: "skill-creator",
    manifest: include_str!("builtin_skill-creator_manifest.toml"),
    skill_md: include_str!("builtin_skill-creator_SKILL.md"),
}];

/// Ensure all built-in skills exist in the workspace's skill directory.
///
/// Writes manifest.toml and SKILL.md for each built-in skill only if the
/// skill directory doesn't already exist — this is safe to call on every
/// startup. If a user has deleted or customized a built-in skill, we don't
/// overwrite it.
///
/// Call this before `load_skill_index_with_roots()` so newly seeded skills
/// are discovered by the normal scan.
pub fn ensure_builtin_skills(workspace_dir: &Path) {
    let skills_dir = workspace_dir.join(".clawseed").join("skills");
    for skill in BUILTIN_SKILLS {
        let skill_dir = skills_dir.join(skill.name);
        if skill_dir.is_dir() {
            continue; // Don't overwrite user-modified skills
        }
        if let Err(e) = std::fs::create_dir_all(&skill_dir) {
            tracing::warn!(
                "Failed to create built-in skill dir {}: {}",
                skill_dir.display(),
                e
            );
            continue;
        }
        let manifest_path = skill_dir.join("manifest.toml");
        if let Err(e) = std::fs::write(&manifest_path, skill.manifest) {
            tracing::warn!("Failed to write {}: {}", manifest_path.display(), e);
        }
        let skill_md_path = skill_dir.join("SKILL.md");
        if let Err(e) = std::fs::write(&skill_md_path, skill.skill_md) {
            tracing::warn!("Failed to write {}: {}", skill_md_path.display(), e);
        }
        tracing::info!("Seeded built-in skill: {}", skill.name);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ensure_creates_skill_files() {
        let dir = tempfile::tempdir().unwrap();
        ensure_builtin_skills(dir.path());

        let skill_dir = dir.path().join(".clawseed/skills/skill-creator");
        assert!(skill_dir.is_dir());
        assert!(skill_dir.join("manifest.toml").exists());
        assert!(skill_dir.join("SKILL.md").exists());
    }

    #[test]
    fn ensure_does_not_overwrite_existing() {
        let dir = tempfile::tempdir().unwrap();
        let skill_dir = dir.path().join(".clawseed/skills/skill-creator");
        std::fs::create_dir_all(&skill_dir).unwrap();
        std::fs::write(skill_dir.join("SKILL.md"), "custom content").unwrap();

        ensure_builtin_skills(dir.path());

        let content = std::fs::read_to_string(skill_dir.join("SKILL.md")).unwrap();
        assert_eq!(content, "custom content");
    }
}
