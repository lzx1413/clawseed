//! Heuristic memory consolidation.
//!
//! After each conversation turn, extracts structured information:
//! - **History entry**: A timestamped summary for the Daily category.
//! - **Memory update**: New facts, preferences, or decisions worth remembering
//!   long-term (stored as Core), when the turn contains high-signal content.
//!
//! This heuristic approach uses importance scoring instead of an LLM call,
//! making it suitable for on-device/mobile use where extra API calls are costly.

use super::conflict;
use super::importance;
use super::traits::{Memory, MemoryCategory};
use clawseed_api::provider::Provider;

/// Importance score threshold above which a turn is promoted to Core memory.
const CORE_PROMOTION_THRESHOLD: f64 = 0.8;

/// Maximum character length for history entries and Core summaries.
const MAX_SUMMARY_LENGTH: usize = 50;

/// Minimum content length to consider for Core promotion.
const MIN_CORE_CONTENT_LENGTH: usize = 10;

/// Consolidate a conversation turn into memory.
///
/// Phase 1: Write a history entry to the Daily category.
/// Phase 2: If the turn contains high-signal content, store as Core
///          with importance metadata and conflict detection.
pub async fn consolidate_turn(
    _provider: &dyn Provider,
    _model: &str,
    memory: &dyn Memory,
    user_message: &str,
    assistant_response: &str,
) -> anyhow::Result<()> {
    let turn_text = format!("User: {user_message}\nAssistant: {assistant_response}");

    // Phase 1: Write history entry to Daily category.
    let date = chrono::Local::now().format("%Y-%m-%d").to_string();
    let history_key = format!("daily_{date}_{}", uuid::Uuid::new_v4());
    let history_summary = truncate_content(&turn_text, MAX_SUMMARY_LENGTH);
    memory
        .store(&history_key, &history_summary, MemoryCategory::Daily, None)
        .await?;

    // Phase 2: Check if the turn contains high-signal content for Core memory.
    let combined = format!("{user_message} {assistant_response}");
    let user_importance = importance::compute_importance(user_message, &MemoryCategory::Core);
    let combined_importance = importance::compute_importance(&combined, &MemoryCategory::Core);

    // Use the higher importance score; also require meaningful length
    let best_importance = user_importance.max(combined_importance);
    let best_content = if user_importance >= combined_importance {
        user_message
    } else {
        &combined
    };

    if best_importance >= CORE_PROMOTION_THRESHOLD && best_content.len() >= MIN_CORE_CONTENT_LENGTH
    {
        let mem_key = format!("core_{}", uuid::Uuid::new_v4());
        let core_summary = truncate_content(best_content, MAX_SUMMARY_LENGTH);

        // Conflict check: find and supersede contradictory Core memories.
        if let Err(e) = conflict::check_and_resolve_conflicts(
            memory,
            &mem_key,
            &core_summary,
            &MemoryCategory::Core,
            0.6,
        )
        .await
        {
            tracing::debug!("conflict check skipped: {e}");
        }

        memory
            .store_with_metadata(
                &mem_key,
                &core_summary,
                MemoryCategory::Core,
                None,
                None,
                Some(best_importance),
            )
            .await?;
    }

    Ok(())
}

/// Truncate text to a concise summary suitable for memory entries.
///
/// Strips common noise patterns and truncates at word boundaries.
fn truncate_content(text: &str, max_len: usize) -> String {
    let cleaned = text
        .lines()
        .filter(|line| {
            let trimmed = line.trim();
            !trimmed.is_empty() && !trimmed.starts_with('[') && !trimmed.starts_with('#')
        })
        .collect::<Vec<_>>()
        .join(" ");

    if cleaned.len() <= max_len {
        return cleaned;
    }

    // Truncate at word boundary
    let end = cleaned
        .char_indices()
        .take_while(|(i, _)| *i < max_len)
        .last()
        .map(|(i, c)| i + c.len_utf8())
        .unwrap_or(max_len);

    // Walk back to the last space within bounds
    let truncated = &cleaned[..end];
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
    fn truncate_short_text_unchanged() {
        let result = truncate_content("Hello world", 50);
        assert_eq!(result, "Hello world");
    }

    #[test]
    fn truncate_long_text_at_word_boundary() {
        let long = "The quick brown fox jumps over the lazy dog and keeps going";
        let result = truncate_content(long, 20);
        // Ellipsis `…` is 3 bytes in UTF-8, so byte length can exceed max_len by a few
        assert!(
            result.len() <= 25,
            "result too long: {result} ({} bytes)",
            result.len()
        );
        assert!(result.ends_with('…'));
    }

    #[test]
    fn truncate_strips_noise_lines() {
        let text = "[IMAGE:/local/path]\n# Heading\nReal content here\n[DOCUMENT:file]";
        let result = truncate_content(text, 50);
        assert!(!result.contains("[IMAGE"));
        assert!(!result.contains("[DOCUMENT"));
        assert!(!result.contains("# Heading"));
        assert!(result.contains("Real content here"));
    }

    #[test]
    fn truncate_empty_lines_filtered() {
        let text = "\n\nHello\n\nWorld\n\n";
        let result = truncate_content(text, 50);
        assert_eq!(result, "Hello World");
    }

    #[test]
    fn core_promotion_threshold_sanity() {
        // "important" and "decision" are high-signal keywords
        let score = importance::compute_importance(
            "This is an important decision about the project",
            &MemoryCategory::Core,
        );
        assert!(
            score >= CORE_PROMOTION_THRESHOLD,
            "High-signal content should exceed promotion threshold, got {score}"
        );
    }

    #[test]
    fn low_signal_stays_below_threshold() {
        let score = importance::compute_importance(
            "Hello, how are you today?",
            &MemoryCategory::Conversation,
        );
        assert!(
            score < CORE_PROMOTION_THRESHOLD,
            "Low-signal content should stay below threshold, got {score}"
        );
    }
}
