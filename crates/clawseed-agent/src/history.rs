//! History management utilities for conversation trimming.

use clawseed_api::provider::ChatMessage;

/// Default trigger for auto-compaction.
pub const DEFAULT_MAX_HISTORY_MESSAGES: usize = 50;

/// Find the largest byte index <= i that is a valid char boundary.
pub fn floor_char_boundary(s: &str, i: usize) -> usize {
    if i >= s.len() {
        return s.len();
    }
    let mut pos = i;
    while pos > 0 && !s.is_char_boundary(pos) {
        pos -= 1;
    }
    pos
}

/// Truncate a tool result to max_chars, keeping head (2/3) + tail (1/3).
pub fn truncate_tool_result(output: &str, max_chars: usize) -> String {
    if max_chars == 0 || output.len() <= max_chars {
        return output.to_string();
    }
    let head_len = max_chars * 2 / 3;
    let tail_len = max_chars.saturating_sub(head_len);
    let head_end = floor_char_boundary(output, head_len);
    let tail_start_raw = output.len().saturating_sub(tail_len);
    let tail_start = if tail_start_raw >= output.len() {
        output.len()
    } else {
        let mut pos = tail_start_raw;
        while pos < output.len() && !output.is_char_boundary(pos) {
            pos += 1;
        }
        pos
    };
    if head_end >= tail_start {
        return output[..floor_char_boundary(output, max_chars)].to_string();
    }
    let truncated_chars = tail_start - head_end;
    format!(
        "{}\n\n[... {} characters truncated ...]\n\n{}",
        &output[..head_end],
        truncated_chars,
        &output[tail_start..]
    )
}

/// Trim conversation history to prevent unbounded growth.
pub fn trim_history(history: &mut Vec<ChatMessage>, max_history: usize) {
    let has_system = history.first().is_some_and(|m| m.role == "system");
    let non_system_count = if has_system {
        history.len() - 1
    } else {
        history.len()
    };

    if non_system_count <= max_history {
        return;
    }

    let start = if has_system { 1 } else { 0 };
    let to_remove = non_system_count - max_history;
    history.drain(start..start + to_remove);
}

/// Estimate token count for a message history.
pub fn estimate_history_tokens(history: &[ChatMessage]) -> usize {
    history
        .iter()
        .map(|m| m.content.len().div_ceil(4) + 4)
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use clawseed_api::provider::ChatMessage;

    // ── floor_char_boundary ──────────────────────────────────────────────

    #[test]
    fn floor_char_boundary_ascii_within_bounds() {
        assert_eq!(floor_char_boundary("hello", 3), 3);
    }

    #[test]
    fn floor_char_boundary_ascii_at_end() {
        assert_eq!(floor_char_boundary("hello", 5), 5);
    }

    #[test]
    fn floor_char_boundary_ascii_past_end() {
        assert_eq!(floor_char_boundary("hi", 10), 2);
    }

    #[test]
    fn floor_char_boundary_empty_string() {
        assert_eq!(floor_char_boundary("", 0), 0);
        assert_eq!(floor_char_boundary("", 5), 0);
    }

    #[test]
    fn floor_char_boundary_multibyte_at_boundary() {
        // "你好" = 6 bytes, char boundaries at 0, 3, 6
        let s = "你好";
        assert_eq!(floor_char_boundary(s, 3), 3);
        assert_eq!(floor_char_boundary(s, 6), 6);
    }

    #[test]
    fn floor_char_boundary_multibyte_mid_char() {
        // "你好" = 6 bytes; byte 4 is inside second char, floor to 3
        let s = "你好";
        assert_eq!(floor_char_boundary(s, 4), 3);
        assert_eq!(floor_char_boundary(s, 1), 0);
    }

    // ── truncate_tool_result ─────────────────────────────────────────────

    #[test]
    fn truncate_tool_result_short_output_unchanged() {
        assert_eq!(truncate_tool_result("hello", 100), "hello");
    }

    #[test]
    fn truncate_tool_result_max_chars_zero() {
        assert_eq!(truncate_tool_result("hello", 0), "hello");
    }

    #[test]
    fn truncate_tool_result_exact_length() {
        assert_eq!(truncate_tool_result("abcde", 5), "abcde");
    }

    #[test]
    fn truncate_tool_result_truncates_long_output() {
        let output = "a".repeat(300);
        let result = truncate_tool_result(&output, 100);
        assert!(result.contains("[... 200 characters truncated ...]"));
        // Head (2/3 of 100 = 66 chars) + tail (1/3 = 34 chars) = 100 + marker
        assert!(result.starts_with(&"a".repeat(66)));
        assert!(result.ends_with(&"a".repeat(34)));
    }

    #[test]
    fn truncate_tool_result_preserves_head_and_tail() {
        let output = "ABCDEFGHIJ".repeat(30); // 300 chars
        let result = truncate_tool_result(&output, 30);
        // Head = 20 chars, tail = 10 chars
        assert!(result.starts_with("ABCDEFGHIJABCDEFGHIJ"));
        assert!(result.ends_with("ABCDEFGHIJ"));
    }

    #[test]
    fn truncate_tool_result_multibyte_safe() {
        // Each 你 is 3 bytes; 100 你 = 300 bytes
        let output = "你".repeat(100);
        let result = truncate_tool_result(&output, 99);
        // Should not panic; the truncation boundaries must be char-aligned
        assert!(!result.is_empty());
    }

    #[test]
    fn truncate_tool_result_small_max_chars() {
        let result = truncate_tool_result("abcdefghij", 6);
        // head = 4, tail = 2 → head_end=4 < tail_start=8, no overlap
        assert!(result.contains("[... 4 characters truncated ...]"));
        assert!(result.starts_with("abcd"));
        assert!(result.ends_with("ij"));
    }

    #[test]
    fn truncate_tool_result_overlap_falls_back() {
        // Very small max where head overlaps tail — verifies no panic
        let _result = truncate_tool_result("abcdefghij", 3);
        let _result2 = truncate_tool_result("abc", 2);
        let result3 = truncate_tool_result("ab", 1);
        assert!(!result3.is_empty());
    }

    // ── trim_history ─────────────────────────────────────────────────────

    #[test]
    fn trim_history_no_system_message_under_limit() {
        let mut history = vec![ChatMessage::user("msg1"), ChatMessage::user("msg2")];
        trim_history(&mut history, 50);
        assert_eq!(history.len(), 2);
    }

    #[test]
    fn trim_history_with_system_preserves_system() {
        let mut history = vec![
            ChatMessage::system("system prompt"),
            ChatMessage::user("msg1"),
            ChatMessage::user("msg2"),
        ];
        trim_history(&mut history, 1);
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].role, "system");
        assert_eq!(history[1].content, "msg2");
    }

    #[test]
    fn trim_history_removes_oldest_non_system() {
        let mut history = vec![
            ChatMessage::system("sys"),
            ChatMessage::user("old"),
            ChatMessage::user("mid"),
            ChatMessage::user("new"),
        ];
        trim_history(&mut history, 2);
        assert_eq!(history.len(), 3); // system + 2 remaining
        assert_eq!(history[1].content, "mid");
        assert_eq!(history[2].content, "new");
    }

    #[test]
    fn trim_history_exact_limit_no_removal() {
        let mut history = vec![
            ChatMessage::system("sys"),
            ChatMessage::user("msg1"),
            ChatMessage::user("msg2"),
        ];
        trim_history(&mut history, 2);
        assert_eq!(history.len(), 3);
    }

    #[test]
    fn trim_history_empty_history() {
        let mut history: Vec<ChatMessage> = vec![];
        trim_history(&mut history, 10);
        assert!(history.is_empty());
    }

    #[test]
    fn trim_history_no_system_over_limit() {
        let mut history = vec![
            ChatMessage::user("a"),
            ChatMessage::user("b"),
            ChatMessage::user("c"),
        ];
        trim_history(&mut history, 2);
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].content, "b");
        assert_eq!(history[1].content, "c");
    }

    // ── estimate_history_tokens ──────────────────────────────────────────

    #[test]
    fn estimate_tokens_empty_history() {
        let history: Vec<ChatMessage> = vec![];
        assert_eq!(estimate_history_tokens(&history), 0);
    }

    #[test]
    fn estimate_tokens_single_message() {
        let history = vec![ChatMessage::user("hello")]; // 5 chars → 5/4 ceil = 2 + 4 = 6
        assert_eq!(estimate_history_tokens(&history), 6);
    }

    #[test]
    fn estimate_tokens_multiple_messages() {
        let history = vec![
            ChatMessage::system("abc"),    // 3/4 ceil = 1 + 4 = 5
            ChatMessage::user("12345678"), // 8/4 = 2 + 4 = 6
        ];
        assert_eq!(estimate_history_tokens(&history), 11);
    }
}
