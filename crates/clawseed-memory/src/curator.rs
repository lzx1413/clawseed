//! Memory curator — LLM-driven nightly cleanup.
//!
//! Analyzes all memories for duplicates, conflicts, and low-importance entries,
//! then produces and executes a cleanup plan: delete, merge, update.
//!
//! Designed to run as a scheduled cron job (e.g. every night at 9 PM).
//! Uses the configured Provider to analyze memories intelligently.

use super::traits::{Memory, MemoryCategory};
use anyhow::Result;
use clawseed_api::provider::{ChatMessage, ChatRequest, Provider};
use serde::{Deserialize, Serialize};
use std::fmt::Write;

/// Maximum character length for curated summaries.
const MAX_CURATED_LENGTH: usize = 50;

/// Report returned after a curate pass.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CurateReport {
    pub deleted: Vec<String>,
    pub merged: Vec<MergeGroup>,
    pub total_before: usize,
    pub total_after: usize,
}

/// A group of memories to merge into a single summary.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergeGroup {
    pub keys: Vec<String>,
    pub merged_content: String,
}

/// LLM curate plan output format.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct CuratePlan {
    delete: Vec<String>,
    merge: Vec<MergeGroup>,
}

/// Run the LLM-driven memory curator.
///
/// 1. Collects all Core + Daily memories
/// 2. Asks the LLM to analyze and produce a cleanup plan (delete, merge)
/// 3. Executes the plan: deletes low-value entries, merges duplicates
///
/// Returns a report summarizing what was done.
pub async fn curate_memories(
    provider: &dyn Provider,
    model: &str,
    memory: &dyn Memory,
) -> Result<CurateReport> {
    // 1. Collect all non-Conversation memories
    let entries = memory.list(None, None).await?;
    let target_entries: Vec<_> = entries
        .iter()
        .filter(|e| {
            matches!(e.category, MemoryCategory::Core | MemoryCategory::Daily)
                && !e.content.starts_with("[SUPERSEDED")
        })
        .collect();

    if target_entries.is_empty() {
        return Ok(CurateReport {
            total_before: 0,
            total_after: 0,
            ..Default::default()
        });
    }

    let total_before = target_entries.len();

    // 2. Build prompt with all entries and ask LLM for a curate plan
    let prompt = build_curate_prompt(&target_entries);
    let plan = get_curate_plan(provider, model, &prompt).await?;

    // 3. Execute the plan
    let mut deleted = Vec::new();
    let mut merged = Vec::new();

    // Delete entries marked as low-value
    for key in &plan.delete {
        if memory.forget(key).await.is_ok() {
            deleted.push(key.clone());
        }
    }

    // Merge groups of duplicate/conflicting entries
    for group in &plan.merge {
        // Store the merged summary as a new Core entry
        let merged_key = format!("core_{}", uuid::Uuid::new_v4());
        let truncated = truncate_content(&group.merged_content, MAX_CURATED_LENGTH);
        if memory
            .store(&merged_key, &truncated, MemoryCategory::Core, None)
            .await
            .is_ok()
        {
            // Delete the original entries that were merged
            for key in &group.keys {
                if memory.forget(key).await.is_ok() {
                    deleted.push(key.clone());
                }
            }
            merged.push(MergeGroup {
                keys: group.keys.clone(),
                merged_content: truncated,
            });
        }
    }

    // Count remaining entries
    let remaining = memory.list(None, None).await?;
    let total_after = remaining
        .iter()
        .filter(|e| {
            matches!(e.category, MemoryCategory::Core | MemoryCategory::Daily)
                && !e.content.starts_with("[SUPERSEDED")
        })
        .count();

    Ok(CurateReport {
        deleted,
        merged,
        total_before,
        total_after,
    })
}

/// Build the LLM prompt listing all memory entries.
fn build_curate_prompt(entries: &[&clawseed_api::memory_traits::MemoryEntry]) -> String {
    let mut prompt = String::from(
        "你是一个记忆整理助手。以下是所有记忆条目，请分析并输出整理方案。\n\n\
         记忆条目列表：\n",
    );

    for (i, entry) in entries.iter().enumerate() {
        let _ = writeln!(
            prompt,
            "{}. [{}] {}: {}",
            i + 1,
            match entry.category {
                MemoryCategory::Core => "Core",
                MemoryCategory::Daily => "Daily",
                MemoryCategory::Conversation => "Conv",
                MemoryCategory::Custom(ref s) => s,
            },
            entry.key,
            entry.content
        );
    }

    prompt.push_str(
        "\n请分析以上记忆，输出 JSON 格式的整理方案：\n\
         ```json\n\
         {\n\
           \"delete\": [\"要删除的key列表，不重要或过时的信息\"],\n\
           \"merge\": [{\"keys\": [\"要合并的key列表\"], \"merged_content\": \"合并后的新摘要(≤50字)\"}]\n\
         }\n\
         ```\n\n\
         整理原则：\n\
         - 只保留真正重要的事实、偏好、决策\n\
         - 重复内容合并为一条更准确的摘要\n\
         - 冲突内容保留最新版本\n\
         - 每条摘要不超过50字\n\
         - Daily 条目如果不重要就删除\n\
         - 不要删除所有记忆，至少保留最关键的几条",
    );

    prompt
}

/// Call the provider and parse the curate plan from the response.
async fn get_curate_plan(provider: &dyn Provider, model: &str, prompt: &str) -> Result<CuratePlan> {
    let messages = vec![
        ChatMessage::system(
            "You are a memory curator assistant. Output ONLY valid JSON, no other text.",
        ),
        ChatMessage::user(prompt),
    ];

    let request = ChatRequest {
        messages: &messages,
        tools: None,
    };

    let response = provider.chat(request, model, None).await?;
    let text = response.text.unwrap_or_default();
    let trimmed = text.trim();

    // Strip markdown code block markers if present
    let json_text = trimmed
        .strip_prefix("```json")
        .or_else(|| trimmed.strip_prefix("```"))
        .unwrap_or(trimmed)
        .strip_suffix("```")
        .unwrap_or(trimmed)
        .trim();

    // Parse the JSON plan
    let plan: CuratePlan = serde_json::from_str(json_text).map_err(|e| {
        anyhow::anyhow!("Failed to parse curate plan JSON: {e}\nRaw response: {text}")
    })?;

    Ok(plan)
}

/// Truncate content to max length at word boundary.
fn truncate_content(content: &str, max_len: usize) -> String {
    if content.chars().count() <= max_len {
        return content.to_string();
    }

    let end = content
        .char_indices()
        .take_while(|(i, _)| *i < max_len)
        .last()
        .map(|(i, c)| i + c.len_utf8())
        .unwrap_or(max_len);

    let truncated = &content[..end];
    if let Some(last_space) = truncated.rfind(' ') {
        format!("{}…", &truncated[..last_space])
    } else {
        format!("{}…", truncated)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_short_content_unchanged() {
        let result = truncate_content("Hello world", 50);
        assert_eq!(result, "Hello world");
    }

    #[test]
    fn truncate_long_content_at_word_boundary() {
        let long = "The quick brown fox jumps over the lazy dog and keeps going";
        let result = truncate_content(long, 20);
        assert!(result.ends_with('…'));
        assert!(result.len() <= 25);
    }

    #[test]
    fn parse_curate_plan_json() {
        let json = r#"{"delete": ["daily_2024_01_01_abcd"], "merge": [{"keys": ["core_1", "core_2"], "merged_content": "User prefers Rust"}]}"#;
        let plan: CuratePlan = serde_json::from_str(json).unwrap();
        assert_eq!(plan.delete.len(), 1);
        assert_eq!(plan.merge.len(), 1);
        assert_eq!(plan.merge[0].keys.len(), 2);
    }

    #[test]
    fn parse_curate_plan_with_code_block() {
        let text = "```json\n{\"delete\": [], \"merge\": []}\n```";
        let stripped = text
            .strip_prefix("```json")
            .unwrap_or(text)
            .strip_suffix("```")
            .unwrap_or(text)
            .trim();
        let plan: CuratePlan = serde_json::from_str(stripped).unwrap();
        assert!(plan.delete.is_empty());
    }

    #[test]
    fn build_curate_prompt_contains_entries() {
        use clawseed_api::memory_traits::MemoryEntry;
        let entries = vec![MemoryEntry {
            id: "1".into(),
            key: "core_pref".into(),
            content: "User prefers Rust".into(),
            category: MemoryCategory::Core,
            timestamp: "2024-01-01".into(),
            session_id: None,
            score: None,
            namespace: "default".into(),
            importance: Some(0.8),
            superseded_by: None,
            embedding: None,
        }];
        let refs: Vec<_> = entries.iter().collect();
        let prompt = build_curate_prompt(&refs);
        assert!(prompt.contains("core_pref"));
        assert!(prompt.contains("User prefers Rust"));
        assert!(prompt.contains("[Core]"));
    }
}
