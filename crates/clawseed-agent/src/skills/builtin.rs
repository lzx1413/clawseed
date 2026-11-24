//! Built-in skills — hardcoded skill definitions that are seeded into every
//! workspace on first use, so they're available immediately after installation.

use sha2::{Digest, Sha256};
use std::path::Path;

/// A built-in skill definition: manifest.toml + SKILL.md content.
struct BuiltinSkill {
    name: &'static str,
    version: &'static str,
    manifest: &'static str,
    skill_md: &'static str,
    legacy_hashes: &'static [&'static str],
}

const BUILTIN_SKILLS: &[BuiltinSkill] = &[
    BuiltinSkill {
        name: "skill-creator",
        version: "0.1.0",
        manifest: include_str!("builtin_skill-creator_manifest.toml"),
        skill_md: include_str!("builtin_skill-creator_SKILL.md"),
        legacy_hashes: &[],
    },
    BuiltinSkill {
        name: "plugin-authoring",
        version: "0.4.0",
        manifest: include_str!("builtin_plugin-authoring_manifest.toml"),
        skill_md: include_str!("builtin_plugin-authoring_SKILL.md"),
        legacy_hashes: &[
            "1cb6f708ce97191caa323570c606e4bff36336d8cc36856efa26ee1781390ef5",
            "65f442d23161ba8a1d63169c1f56510e55967cfc293c75adb7a610b06dcf9c5a",
            "79100be99121d0dfdd1484bc907bf1c1b5bf84b485a742c8f299a9bdeaa0c4f4",
        ],
    },
    BuiltinSkill {
        name: "media-presentation",
        version: "0.5.0",
        manifest: include_str!("builtin_media-presentation_manifest.toml"),
        skill_md: include_str!("builtin_media-presentation_SKILL.md"),
        legacy_hashes: &[
            "80323bb48a8144f509b99d552c83700bf376a09f39b85301e617ab569c93d7ea",
            "69b38bb3be978cf6c8a82bcd63b9cbae818c2b287450ce860980fcb0c8e59744",
        ],
    },
];

/// Ensure built-in skills exist and upgrade files that still contain system content.
/// User-customized SKILL.md files are preserved.
///
/// Call this before `load_skill_index_with_roots()` so newly seeded skills
/// are discovered by the normal scan.
pub fn ensure_builtin_skills(workspace_dir: &Path) {
    let skills_dir = workspace_dir.join(".clawseed").join("skills");
    for skill in BUILTIN_SKILLS {
        let skill_dir = skills_dir.join(skill.name);
        if !skill_dir.exists()
            && let Err(e) = std::fs::create_dir_all(&skill_dir)
        {
            tracing::warn!(
                "Failed to create built-in skill dir {}: {}",
                skill_dir.display(),
                e
            );
            continue;
        }
        let manifest_path = skill_dir.join("manifest.toml");
        let skill_md_path = skill_dir.join("SKILL.md");
        let marker_path = skill_dir.join(".clawseed-builtin.sha256");
        let current_hash = std::fs::read(&skill_md_path)
            .ok()
            .map(|bytes| sha256_hex(&bytes));
        let marker_hash = std::fs::read_to_string(&marker_path)
            .ok()
            .map(|value| value.trim().to_owned());
        let is_new = !skill_md_path.exists();
        let is_known_system_content = current_hash.as_deref().is_some_and(|hash| {
            marker_hash.as_deref() == Some(hash) || skill.legacy_hashes.contains(&hash)
        });
        if is_new || is_known_system_content {
            if let Err(e) = std::fs::write(&manifest_path, skill.manifest) {
                tracing::warn!("Failed to write {}: {}", manifest_path.display(), e);
            }
            if let Err(e) = std::fs::write(&skill_md_path, skill.skill_md) {
                tracing::warn!("Failed to write {}: {}", skill_md_path.display(), e);
                continue;
            }
            let hash = sha256_hex(skill.skill_md.as_bytes());
            if let Err(e) = std::fs::write(&marker_path, format!("{hash}\n")) {
                tracing::warn!("Failed to write {}: {}", marker_path.display(), e);
            }
            tracing::info!(
                "Seeded or upgraded built-in skill {} to {}",
                skill.name,
                skill.version
            );
        } else if !manifest_path.exists() {
            tracing::debug!(
                "Preserving customized built-in skill {}; manifest is missing",
                skill.name
            );
        }
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut digest = Sha256::new();
    digest.update(bytes);
    hex::encode(digest.finalize())
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
        let plugin_skill_dir = dir.path().join(".clawseed/skills/plugin-authoring");
        assert!(plugin_skill_dir.join("manifest.toml").exists());
        assert!(plugin_skill_dir.join("SKILL.md").exists());
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

    #[test]
    fn ensure_records_builtin_hash_for_future_upgrades() {
        let dir = tempfile::tempdir().unwrap();
        ensure_builtin_skills(dir.path());
        let marker = dir
            .path()
            .join(".clawseed/skills/plugin-authoring/.clawseed-builtin.sha256");
        assert_eq!(
            std::fs::read_to_string(marker).unwrap().trim(),
            sha256_hex(BUILTIN_SKILLS[1].skill_md.as_bytes())
        );
    }
}
