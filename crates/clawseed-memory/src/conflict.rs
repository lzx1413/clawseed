//! Conflict resolution for memory entries.
//!
//! Before storing Core memories, performs a text similarity check against
//! existing entries. If Jaccard similarity exceeds a threshold but content
//! differs, the old entry is marked as superseded by appending a marker.

use super::traits::{Memory, MemoryCategory, MemoryEntry};

/// Check for conflicting Core memories and supersede old ones.
///
/// Uses `memory.recall()` to find similar entries, then applies Jaccard
/// similarity to detect conflicts. Only Core memories are checked.
/// Conflicting entries are superseded by re-storing with a prefix marker.
///
/// Returns the list of entry keys that were superseded.
pub async fn check_and_resolve_conflicts(
    memory: &dyn Memory,
    key: &str,
    content: &str,
    category: &MemoryCategory,
    threshold: f64,
) -> anyhow::Result<Vec<String>> {
    if !matches!(category, MemoryCategory::Core) {
        return Ok(Vec::new());
    }

    let candidates = memory.recall(content, 10, None, None, None).await?;
    let conflicts = find_text_conflicts(&candidates, key, content, threshold);

    // Supersede conflicting entries by re-storing with a prefix marker
    let conflict_ids: Vec<String> = conflicts.iter().map(|e| e.id.clone()).collect();

    for entry in &candidates {
        if conflict_ids.contains(&entry.id) {
            let superseded_content = format!("[SUPERSEDED by '{}'] {}", key, entry.content);
            memory
                .store_with_metadata(
                    &entry.key,
                    &superseded_content,
                    entry.category.clone(),
                    entry.session_id.as_deref(),
                    Some(&entry.namespace),
                    entry.importance,
                )
                .await?;
        }
    }

    Ok(conflict_ids)
}

/// Find potentially conflicting entries using text similarity.
///
/// Returns entries where Jaccard similarity exceeds the threshold,
/// the content differs, and the entry hasn't already been superseded.
pub fn find_text_conflicts(
    entries: &[MemoryEntry],
    own_key: &str,
    new_content: &str,
    threshold: f64,
) -> Vec<MemoryEntry> {
    entries
        .iter()
        .filter(|e| {
            matches!(e.category, MemoryCategory::Core)
                && e.superseded_by.is_none()
                && e.key != own_key
                && !e.content.starts_with("[SUPERSEDED")
                && jaccard_similarity(&e.content, new_content) > threshold
                && e.content != new_content
        })
        .cloned()
        .collect()
}

/// Compute Jaccard similarity between two strings based on word overlap.
///
/// Returns a value between 0.0 (no overlap) and 1.0 (identical word sets).
pub fn jaccard_similarity(a: &str, b: &str) -> f64 {
    let words_a: std::collections::HashSet<&str> = a.split_whitespace().collect();
    let words_b: std::collections::HashSet<&str> = b.split_whitespace().collect();

    if words_a.is_empty() && words_b.is_empty() {
        return 1.0;
    }
    if words_a.is_empty() || words_b.is_empty() {
        return 0.0;
    }

    let intersection = words_a.intersection(&words_b).count();
    let union = words_a.union(&words_b).count();

    if union == 0 {
        0.0
    } else {
        intersection as f64 / union as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jaccard_identical_strings() {
        let sim = jaccard_similarity("hello world", "hello world");
        assert!((sim - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn jaccard_disjoint_strings() {
        let sim = jaccard_similarity("hello world", "foo bar");
        assert!(sim.abs() < f64::EPSILON);
    }

    #[test]
    fn jaccard_partial_overlap() {
        let sim = jaccard_similarity("the quick brown fox", "the slow brown dog");
        assert!((sim - 2.0 / 6.0).abs() < 0.01);
    }

    #[test]
    fn jaccard_empty_strings() {
        assert!((jaccard_similarity("", "") - 1.0).abs() < f64::EPSILON);
        assert!(jaccard_similarity("hello", "").abs() < f64::EPSILON);
    }

    fn make_entry(id: &str, key: &str, content: &str, category: MemoryCategory) -> MemoryEntry {
        MemoryEntry {
            id: id.into(),
            key: key.into(),
            content: content.into(),
            category,
            timestamp: "now".into(),
            session_id: None,
            score: None,
            namespace: "default".into(),
            importance: Some(0.7),
            superseded_by: None,
        }
    }

    #[test]
    fn find_text_conflicts_filters_correctly() {
        let entries = vec![
            make_entry(
                "1",
                "pref",
                "User prefers Rust for systems work",
                MemoryCategory::Core,
            ),
            make_entry(
                "2",
                "daily1",
                "User prefers Rust for systems work",
                MemoryCategory::Daily,
            ),
            make_entry(
                "3",
                "pref2",
                "User prefers Rust for systems work",
                MemoryCategory::Core,
            ),
        ];

        // Entry 3: superseded
        let mut entries = entries;
        entries[2].superseded_by = Some("other".into());

        let conflicts = find_text_conflicts(
            &entries,
            "new_key",
            "User now prefers Go for systems work",
            0.3,
        );
        assert_eq!(conflicts.len(), 1);
        assert_eq!(conflicts[0].id, "1");
    }

    #[test]
    fn find_text_conflicts_skips_own_key() {
        let entries = vec![make_entry(
            "1",
            "pref",
            "User prefers Rust for systems work",
            MemoryCategory::Core,
        )];

        let conflicts = find_text_conflicts(&entries, "pref", "User prefers Go", 0.0);
        assert!(conflicts.is_empty());
    }

    #[test]
    fn find_text_conflicts_skips_already_superseded() {
        let mut entry = make_entry(
            "1",
            "pref",
            "[SUPERSEDED by 'new'] Old preference",
            MemoryCategory::Core,
        );
        entry.superseded_by = None; // content prefix is the marker

        let entries = vec![entry];
        let conflicts = find_text_conflicts(&entries, "new_key", "Old preference updated", 0.3);
        assert!(
            conflicts.is_empty(),
            "Should skip entries with SUPERSEDED prefix"
        );
    }
}
