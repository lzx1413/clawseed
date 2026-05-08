//! Personality system — loads workspace identity files (SOUL.md, IDENTITY.md,
//! USER.md, etc.) and injects them into the system prompt pipeline.

use std::fmt::Write;
use std::path::{Path, PathBuf};

const MAX_FILE_CHARS: usize = 20_000;

const PERSONALITY_FILES: &[&str] = &[
    "SOUL.md",
    "IDENTITY.md",
    "USER.md",
    "AGENTS.md",
    "TOOLS.md",
    "HEARTBEAT.md",
    "BOOTSTRAP.md",
    "MEMORY.md",
];

/// Returns the list of recognized personality file names.
/// Used by the gateway API to validate write targets.
pub fn allowed_personality_files() -> &'static [&'static str] {
    PERSONALITY_FILES
}

#[derive(Debug, Clone)]
pub struct PersonalityFile {
    pub name: String,
    pub content: String,
    pub truncated: bool,
    pub path: PathBuf,
}

#[derive(Debug, Clone, Default)]
pub struct PersonalityProfile {
    pub files: Vec<PersonalityFile>,
    pub missing: Vec<String>,
}

impl PersonalityProfile {
    pub fn get(&self, name: &str) -> Option<&str> {
        self.files
            .iter()
            .find(|f| f.name == name)
            .map(|f| f.content.as_str())
    }

    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    pub fn render(&self) -> String {
        let mut out = String::new();
        for file in &self.files {
            let _ = writeln!(out, "### {}\n", file.name);
            out.push_str(&file.content);
            if file.truncated {
                let _ = writeln!(
                    out,
                    "\n\n[... truncated at {MAX_FILE_CHARS} chars — use `read` for full file]\n"
                );
            } else {
                out.push_str("\n\n");
            }
        }
        out
    }
}

pub fn load_personality(workspace_dir: &Path) -> PersonalityProfile {
    load_personality_files(workspace_dir, PERSONALITY_FILES)
}

pub fn load_personality_files(workspace_dir: &Path, filenames: &[&str]) -> PersonalityProfile {
    let mut profile = PersonalityProfile::default();

    for &filename in filenames {
        let path = workspace_dir.join(filename);
        match std::fs::read_to_string(&path) {
            Ok(raw) => {
                let trimmed = raw.trim();
                if trimmed.is_empty() {
                    profile.missing.push(filename.to_string());
                    continue;
                }
                let (content, truncated) = truncate_content(trimmed);
                profile.files.push(PersonalityFile {
                    name: filename.to_string(),
                    content,
                    truncated,
                    path,
                });
            }
            Err(_) => {
                profile.missing.push(filename.to_string());
            }
        }
    }

    profile
}

fn truncate_content(content: &str) -> (String, bool) {
    if content.chars().count() <= MAX_FILE_CHARS {
        return (content.to_string(), false);
    }
    let truncated = content
        .char_indices()
        .nth(MAX_FILE_CHARS)
        .map(|(idx, _)| &content[..idx])
        .unwrap_or(content);
    (truncated.to_string(), true)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_workspace(files: &[(&str, &str)]) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "clawseed_personality_test_{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        for (name, content) in files {
            std::fs::write(dir.join(name), content).unwrap();
        }
        dir
    }

    #[test]
    fn load_personality_reads_existing_files() {
        let ws = setup_workspace(&[
            ("SOUL.md", "I am a helpful assistant."),
            ("IDENTITY.md", "Name: Nova"),
        ]);

        let profile = load_personality(&ws);
        assert_eq!(profile.files.len(), 2);
        assert_eq!(profile.get("SOUL.md").unwrap(), "I am a helpful assistant.");
        assert_eq!(profile.get("IDENTITY.md").unwrap(), "Name: Nova");
        assert!(!profile.is_empty());

        let _ = std::fs::remove_dir_all(ws);
    }

    #[test]
    fn load_personality_records_missing_files() {
        let ws = setup_workspace(&[("SOUL.md", "soul content")]);

        let profile = load_personality(&ws);
        assert_eq!(profile.files.len(), 1);
        assert!(profile.missing.contains(&"IDENTITY.md".to_string()));
        assert!(profile.missing.contains(&"USER.md".to_string()));

        let _ = std::fs::remove_dir_all(ws);
    }

    #[test]
    fn load_personality_treats_empty_files_as_missing() {
        let ws = setup_workspace(&[("SOUL.md", "   \n  ")]);

        let profile = load_personality(&ws);
        assert!(profile.is_empty());
        assert!(profile.missing.contains(&"SOUL.md".to_string()));

        let _ = std::fs::remove_dir_all(ws);
    }

    #[test]
    fn load_personality_truncates_large_files() {
        let large = "x".repeat(MAX_FILE_CHARS + 500);
        let ws = setup_workspace(&[("SOUL.md", &large)]);

        let profile = load_personality(&ws);
        let soul = profile.files.iter().find(|f| f.name == "SOUL.md").unwrap();
        assert!(soul.truncated);
        assert_eq!(soul.content.chars().count(), MAX_FILE_CHARS);

        let _ = std::fs::remove_dir_all(ws);
    }

    #[test]
    fn render_produces_markdown_sections() {
        let ws = setup_workspace(&[("SOUL.md", "Be kind."), ("IDENTITY.md", "Name: Nova")]);

        let profile = load_personality(&ws);
        let rendered = profile.render();
        assert!(rendered.contains("### SOUL.md"));
        assert!(rendered.contains("Be kind."));
        assert!(rendered.contains("### IDENTITY.md"));
        assert!(rendered.contains("Name: Nova"));

        let _ = std::fs::remove_dir_all(ws);
    }

    #[test]
    fn render_truncated_file_shows_notice() {
        let large = "y".repeat(MAX_FILE_CHARS + 100);
        let ws = setup_workspace(&[("SOUL.md", &large)]);

        let profile = load_personality(&ws);
        let rendered = profile.render();
        assert!(rendered.contains("[... truncated at"));

        let _ = std::fs::remove_dir_all(ws);
    }

    #[test]
    fn empty_workspace_yields_empty_profile() {
        let ws = setup_workspace(&[]);
        let profile = load_personality(&ws);
        assert!(profile.is_empty());
        assert!(!profile.missing.is_empty());
        let _ = std::fs::remove_dir_all(ws);
    }
}
