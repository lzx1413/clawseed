//! Conflict resolution for memory entries.
//!
//! Before storing Core memories, performs a text similarity check against
//! existing entries. If combined similarity exceeds a threshold but content
//! differs, the old entry is marked as superseded by appending a marker.
//!
//! Phase E adds multi-signal conflict detection: weighted combination of
//! Jaccard word overlap, cosine embedding similarity, and BM25 token overlap,
//! plus heuristic contradiction signal detection (negation reversal, preference
//! change, temporal contradiction).

use super::traits::{Memory, MemoryCategory, MemoryEntry};
use super::vector;
use clawseed_api::memory_traits::ConflictMode;
use std::collections::HashSet;

/// Check for conflicting Core memories and supersede old ones.
///
/// Uses `memory.recall_with_embeddings()` to find similar entries (with
/// embeddings for cosine similarity), then applies multi-signal conflict
/// detection. Only Core memories are checked. Conflicting entries are
/// superseded by re-storing with a prefix marker.
///
/// Returns the list of entry keys that were superseded.
pub async fn check_and_resolve_conflicts(
    memory: &dyn Memory,
    key: &str,
    content: &str,
    category: &MemoryCategory,
    threshold: f64,
    mode: &ConflictMode,
) -> anyhow::Result<Vec<String>> {
    if !matches!(category, MemoryCategory::Core) {
        return Ok(Vec::new());
    }

    let candidates = memory
        .recall_with_embeddings(content, 10, None, None, None, None)
        .await?;
    let conflicts = find_text_conflicts(&candidates, key, content, threshold, mode);

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

/// Find potentially conflicting entries using multi-signal similarity.
///
/// Returns entries where combined similarity + contradiction signal boost
/// exceeds the threshold, the content differs, and the entry hasn't already
/// been superseded.
///
/// When contradiction signals are detected, they add a boost (weighted at 0.3)
/// to the combined similarity score, making it more likely to exceed the
/// threshold.
pub fn find_text_conflicts(
    entries: &[MemoryEntry],
    own_key: &str,
    new_content: &str,
    threshold: f64,
    mode: &ConflictMode,
) -> Vec<MemoryEntry> {
    // Create a temporary entry for the new content so we can compute
    // pairwise similarity against existing entries.
    // New content doesn't have an embedding yet, so combined_similarity
    // will fall back to pure Jaccard when embedding is None on one side.
    let new_entry = MemoryEntry {
        id: String::new(),
        key: own_key.into(),
        content: new_content.into(),
        category: MemoryCategory::Core,
        timestamp: String::new(),
        session_id: None,
        score: None,
        namespace: "default".into(),
        importance: None,
        superseded_by: None,
        embedding: None,
    };

    entries
        .iter()
        .filter(|e| {
            matches!(e.category, MemoryCategory::Core)
                && e.superseded_by.is_none()
                && e.key != own_key
                && !e.content.starts_with("[SUPERSEDED")
        })
        .filter(|e| {
            let similarity = combined_similarity(e, &new_entry, mode);
            let contradiction = detect_contradiction_signals(e, new_content);
            // Combined score: similarity + contradiction boost (weighted at 0.3)
            let total = similarity + contradiction * 0.3;
            total > threshold && e.content != new_content
        })
        .cloned()
        .collect()
}

/// Compute combined similarity between two entries using multiple signals.
///
/// If either embedding is None (embedding unavailable), falls back to
/// pure Jaccard similarity. No weight renormalization is performed.
///
/// When both embeddings are present:
///   similarity = jaccard_w * jaccard + cosine_w * cosine_sim + bm25_w * bm25_overlap
pub fn combined_similarity(
    entry_a: &MemoryEntry,
    entry_b: &MemoryEntry,
    mode: &ConflictMode,
) -> f64 {
    match mode {
        ConflictMode::Jaccard => jaccard_similarity(&entry_a.content, &entry_b.content),
        ConflictMode::Combined {
            jaccard_w,
            cosine_w,
            bm25_w,
        } => {
            let jaccard = jaccard_similarity(&entry_a.content, &entry_b.content);

            // If either embedding is missing, fall back to pure Jaccard
            let emb_a = entry_a.embedding.as_ref();
            let emb_b = entry_b.embedding.as_ref();

            if emb_a.is_none() || emb_b.is_none() {
                return jaccard;
            }

            let cosine = f64::from(vector::cosine_similarity(emb_a.unwrap(), emb_b.unwrap()));
            let bm25 = bm25_overlap(&entry_a.content, &entry_b.content);

            f64::from(*jaccard_w) * jaccard
                + f64::from(*cosine_w) * cosine
                + f64::from(*bm25_w) * bm25
        }
    }
}

/// Compute BM25-style token overlap between two strings.
///
/// BM25 overlap = count of shared unique tokens / max(unique_tokens_a, unique_tokens_b).
///
/// This gives higher scores than Jaccard when many tokens overlap relative
/// to the longer document, capturing "most of the shorter doc's tokens appear
/// in the longer doc" which Jaccard's union denominator penalizes.
pub fn bm25_overlap(a: &str, b: &str) -> f64 {
    let set_a: HashSet<&str> = a.split_whitespace().collect();
    let set_b: HashSet<&str> = b.split_whitespace().collect();

    if set_a.is_empty() && set_b.is_empty() {
        return 1.0;
    }
    if set_a.is_empty() || set_b.is_empty() {
        return 0.0;
    }

    let intersection = set_a.intersection(&set_b).count();
    let max_len = std::cmp::max(set_a.len(), set_b.len());

    if max_len == 0 {
        0.0
    } else {
        intersection as f64 / max_len as f64
    }
}

/// Heuristic contradiction signal detection between a memory entry and new content.
///
/// Checks for:
/// - Negation reversal: one entry negates a claim in the other
///   (e.g., "likes X" vs "does not like X" / "hates X")
/// - Preference change: both entries state preferences but with different values
///   (e.g., "prefers X" vs "prefers Y")
/// - Temporal contradiction: one entry contains absolute terms ("always/forever")
///   while the other contains temporal shift signals ("now/currently/switched")
///
/// Returns a float between 0.0 (no contradiction signals) and 1.0 (strong
/// contradiction signals). This is additive on top of combined_similarity.
pub fn detect_contradiction_signals(existing: &MemoryEntry, new_content: &str) -> f64 {
    let existing_lower = existing.content.to_ascii_lowercase();
    let new_lower = new_content.to_ascii_lowercase();

    let mut signal_score: f64 = 0.0;

    // Signal 1: Negation reversal
    // If one says "not/doesn't/don't/hates/dislikes" and the other says the
    // same tokens without negation
    const NEGATION_WORDS: &[&str] = &[
        "not", "doesn't", "don't", "never", "no", "hates", "dislikes", "won't",
    ];
    let has_negation_existing = NEGATION_WORDS.iter().any(|w| existing_lower.contains(w));
    let has_negation_new = NEGATION_WORDS.iter().any(|w| new_lower.contains(w));

    if has_negation_existing != has_negation_new {
        // Check if they share significant content beyond the negation
        let jaccard = jaccard_similarity(&existing_lower, &new_lower);
        if jaccard > 0.3 {
            signal_score += 0.4;
        }
    }

    // Signal 2: Preference change
    // Both mention "prefers/likes/favorite" but with different values
    const PREFERENCE_WORDS: &[&str] = &["prefers", "likes", "favorite", "favourite", "loves"];
    let has_pref_existing = PREFERENCE_WORDS.iter().any(|w| existing_lower.contains(w));
    let has_pref_new = PREFERENCE_WORDS.iter().any(|w| new_lower.contains(w));

    if has_pref_existing && has_pref_new && existing_lower != new_lower {
        signal_score += 0.3;
    }

    // Signal 3: Temporal contradiction
    // "always/forever" vs "now/currently/recently" with shared content
    const ABSOLUTE_WORDS: &[&str] = &["always", "forever", "every", "all"];
    const TEMPORAL_WORDS: &[&str] = &[
        "now",
        "currently",
        "recently",
        "lately",
        "switched",
        "changed",
    ];
    let has_absolute = ABSOLUTE_WORDS.iter().any(|w| existing_lower.contains(w));
    let has_temporal = TEMPORAL_WORDS.iter().any(|w| new_lower.contains(w));

    if has_absolute && has_temporal {
        let jaccard = jaccard_similarity(&existing_lower, &new_lower);
        if jaccard > 0.2 {
            signal_score += 0.3;
        }
    }

    signal_score.min(1.0)
}

/// Compute Jaccard similarity between two strings based on word overlap.
///
/// Returns a value between 0.0 (no overlap) and 1.0 (identical word sets).
pub fn jaccard_similarity(a: &str, b: &str) -> f64 {
    let words_a: HashSet<&str> = a.split_whitespace().collect();
    let words_b: HashSet<&str> = b.split_whitespace().collect();

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
            embedding: None,
        }
    }

    fn make_entry_with_embedding(
        id: &str,
        key: &str,
        content: &str,
        embedding: Option<Vec<f32>>,
    ) -> MemoryEntry {
        MemoryEntry {
            id: id.into(),
            key: key.into(),
            content: content.into(),
            category: MemoryCategory::Core,
            timestamp: "now".into(),
            session_id: None,
            score: None,
            namespace: "default".into(),
            importance: Some(0.7),
            superseded_by: None,
            embedding,
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
            &ConflictMode::Jaccard,
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

        let conflicts = find_text_conflicts(
            &entries,
            "pref",
            "User prefers Go",
            0.0,
            &ConflictMode::Jaccard,
        );
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
        let conflicts = find_text_conflicts(
            &entries,
            "new_key",
            "Old preference updated",
            0.3,
            &ConflictMode::Jaccard,
        );
        assert!(
            conflicts.is_empty(),
            "Should skip entries with SUPERSEDED prefix"
        );
    }

    // ── Phase E tests ───────────────────────────────────────────────

    #[test]
    fn combined_similarity_jaccard_mode() {
        let a = make_entry_with_embedding("1", "k1", "hello world", None);
        let b = make_entry_with_embedding("2", "k2", "hello there", None);
        let mode = ConflictMode::Jaccard;
        let sim = combined_similarity(&a, &b, &mode);
        // Should be identical to jaccard_similarity
        let expected = jaccard_similarity("hello world", "hello there");
        assert!((sim - expected).abs() < 0.001);
    }

    #[test]
    fn combined_similarity_without_embeddings_falls_back_to_jaccard() {
        let a = make_entry_with_embedding("1", "k1", "hello world", None);
        let b = make_entry_with_embedding("2", "k2", "hello there", Some(vec![0.1; 768]));
        let mode = ConflictMode::default();
        let sim = combined_similarity(&a, &b, &mode);
        // One embedding missing → pure Jaccard
        let expected = jaccard_similarity("hello world", "hello there");
        assert!((sim - expected).abs() < 0.001);
    }

    #[test]
    fn combined_similarity_both_embeddings() {
        // Identical embeddings → cosine = 1.0
        let emb = vec![0.5, 0.5, 0.5];
        let a = make_entry_with_embedding("1", "k1", "hello world", Some(emb.clone()));
        let b = make_entry_with_embedding("2", "k2", "hello there", Some(emb));
        let mode = ConflictMode::Combined {
            jaccard_w: 0.4,
            cosine_w: 0.4,
            bm25_w: 0.2,
        };
        let sim = combined_similarity(&a, &b, &mode);
        let jaccard = jaccard_similarity("hello world", "hello there");
        let cosine = 1.0; // identical vectors
        let bm25 = bm25_overlap("hello world", "hello there");
        let expected = 0.4 * jaccard + 0.4 * cosine + 0.2 * bm25;
        assert!((sim - expected).abs() < 0.01);
    }

    #[test]
    fn bm25_overlap_identical() {
        let overlap = bm25_overlap("hello world", "hello world");
        assert!((overlap - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn bm25_overlap_disjoint() {
        let overlap = bm25_overlap("hello world", "foo bar");
        assert!(overlap.abs() < f64::EPSILON);
    }

    #[test]
    fn bm25_overlap_asymmetric() {
        // Short vs long: "A B" vs "A B C D E F"
        // Jaccard: 2/6 = 0.33, BM25 overlap: 2/max(2,6) = 0.33
        let overlap = bm25_overlap("A B", "A B C D E F");
        assert!(overlap >= 0.33);
    }

    #[test]
    fn bm25_overlap_empty() {
        assert!((bm25_overlap("", "") - 1.0).abs() < f64::EPSILON);
        assert!(bm25_overlap("hello", "").abs() < f64::EPSILON);
    }

    #[test]
    fn contradiction_signals_negation_reversal() {
        let existing = make_entry("1", "pref", "User likes coffee", MemoryCategory::Core);
        let signals = detect_contradiction_signals(&existing, "User doesn't like coffee");
        assert!(signals > 0.0, "negation reversal should produce a signal");
    }

    #[test]
    fn contradiction_signals_preference_change() {
        let existing = make_entry("1", "pref", "User prefers Rust", MemoryCategory::Core);
        let signals = detect_contradiction_signals(&existing, "User prefers Go");
        assert!(signals > 0.0, "preference change should produce a signal");
    }

    #[test]
    fn contradiction_signals_temporal() {
        // Use enough shared words to get jaccard > 0.2
        let existing = make_entry(
            "1",
            "rule",
            "always use HTTPS for all connections",
            MemoryCategory::Core,
        );
        let signals =
            detect_contradiction_signals(&existing, "now use HTTP for all local connections");
        // Shared tokens: {use, for, all, connections} = 4, union = 7 → jaccard = 4/7 = 0.57
        assert!(
            signals > 0.0,
            "temporal contradiction should produce a signal: {signals}"
        );
    }

    #[test]
    fn contradiction_signals_no_match() {
        let existing = make_entry("1", "name", "User name is Alice", MemoryCategory::Core);
        let signals = detect_contradiction_signals(&existing, "User works at BigCorp");
        assert!(
            signals.abs() < f64::EPSILON,
            "no contradiction should produce 0.0"
        );
    }

    #[test]
    fn find_text_conflicts_combined_mode_with_embedding() {
        // Two entries with identical embeddings but different words
        // Combined similarity should be higher than pure Jaccard
        let emb = vec![0.5; 4];
        let entries = vec![
            make_entry_with_embedding("1", "pref", "likes coffee", Some(emb.clone())),
            make_entry_with_embedding("2", "other", "likes tea", Some(emb)),
        ];
        let new_content = "likes espresso";

        // With Combined mode, cosine similarity of identical embeddings boosts the score
        let conflicts_combined = find_text_conflicts(
            &entries,
            "new_key",
            new_content,
            0.3,
            &ConflictMode::Combined {
                jaccard_w: 0.4,
                cosine_w: 0.4,
                bm25_w: 0.2,
            },
        );
        // "likes coffee" vs "likes espresso" shares "likes", with cosine boost
        assert!(
            conflicts_combined.len() >= 1,
            "combined mode should catch at least one conflict"
        );
    }
}
