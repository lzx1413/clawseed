//! Interactive approval workflow for supervised mode.
//!
//! Provides a pre-execution hook that prompts the user before tool calls,
//! with session-scoped "Always" allowlists and audit logging.

use chrono::Utc;
use clawseed_config::schema::AutonomyLevel;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::io::{self, BufRead, Write};

/// A request to approve a tool call before execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalRequest {
    pub tool_name: String,
    pub arguments: serde_json::Value,
}

/// The user's response to an approval request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ApprovalResponse {
    Yes,
    No,
    Always,
}

/// A single audit log entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalLogEntry {
    pub timestamp: String,
    pub tool_name: String,
    pub arguments_summary: String,
    pub decision: ApprovalResponse,
    pub channel: String,
}

/// Manages the approval workflow for tool calls.
pub struct ApprovalManager {
    auto_approve: HashSet<String>,
    always_ask: HashSet<String>,
    autonomy_level: AutonomyLevel,
    non_interactive: bool,
    session_allowlist: Mutex<HashSet<String>>,
    audit_log: Mutex<Vec<ApprovalLogEntry>>,
}

impl ApprovalManager {
    /// Create an interactive (CLI) approval manager.
    pub fn new(
        auto_approve: Vec<String>,
        always_ask: Vec<String>,
        autonomy_level: AutonomyLevel,
    ) -> Self {
        Self {
            auto_approve: auto_approve.into_iter().collect(),
            always_ask: always_ask.into_iter().collect(),
            autonomy_level,
            non_interactive: false,
            session_allowlist: Mutex::new(HashSet::new()),
            audit_log: Mutex::new(Vec::new()),
        }
    }

    /// Create a non-interactive approval manager for channel-driven runs.
    pub fn for_non_interactive(
        auto_approve: Vec<String>,
        always_ask: Vec<String>,
        autonomy_level: AutonomyLevel,
    ) -> Self {
        Self {
            auto_approve: auto_approve.into_iter().collect(),
            always_ask: always_ask.into_iter().collect(),
            autonomy_level,
            non_interactive: true,
            session_allowlist: Mutex::new(HashSet::new()),
            audit_log: Mutex::new(Vec::new()),
        }
    }

    pub fn is_non_interactive(&self) -> bool {
        self.non_interactive
    }

    /// Check whether a tool call requires interactive approval.
    pub fn needs_approval(&self, tool_name: &str) -> bool {
        if self.autonomy_level == AutonomyLevel::Full {
            return false;
        }
        if self.autonomy_level == AutonomyLevel::ReadOnly {
            return false;
        }
        if self.always_ask.contains("*") || self.always_ask.contains(tool_name) {
            return true;
        }
        if self.non_interactive && tool_name == "shell" {
            return false;
        }
        if self.auto_approve.contains("*") || self.auto_approve.contains(tool_name) {
            return false;
        }
        let allowlist = self.session_allowlist.lock();
        if allowlist.contains(tool_name) {
            return false;
        }
        true
    }

    /// Record an approval decision and update session state.
    pub fn record_decision(
        &self,
        tool_name: &str,
        args: &serde_json::Value,
        decision: ApprovalResponse,
        channel: &str,
    ) {
        if decision == ApprovalResponse::Always {
            let mut allowlist = self.session_allowlist.lock();
            allowlist.insert(tool_name.to_string());
        }

        let summary = summarize_args(args);
        let entry = ApprovalLogEntry {
            timestamp: Utc::now().to_rfc3339(),
            tool_name: tool_name.to_string(),
            arguments_summary: summary,
            decision,
            channel: channel.to_string(),
        };
        let mut log = self.audit_log.lock();
        log.push(entry);
    }

    pub fn audit_log(&self) -> Vec<ApprovalLogEntry> {
        self.audit_log.lock().clone()
    }

    pub fn session_allowlist(&self) -> HashSet<String> {
        self.session_allowlist.lock().clone()
    }

    pub fn prompt_cli(&self, request: &ApprovalRequest) -> ApprovalResponse {
        prompt_cli_interactive(request)
    }
}

fn prompt_cli_interactive(request: &ApprovalRequest) -> ApprovalResponse {
    let summary = summarize_args(&request.arguments);
    eprintln!();
    eprintln!("Agent wants to execute: {}", request.tool_name);
    eprintln!("   {summary}");
    eprint!("   [Y]es / [N]o / [A]lways for {}: ", request.tool_name);
    let _ = io::stderr().flush();

    let stdin = io::stdin();
    let mut line = String::new();
    if stdin.lock().read_line(&mut line).is_err() {
        return ApprovalResponse::No;
    }

    match line.trim().to_ascii_lowercase().as_str() {
        "y" | "yes" => ApprovalResponse::Yes,
        "a" | "always" => ApprovalResponse::Always,
        _ => ApprovalResponse::No,
    }
}

pub fn summarize_args(args: &serde_json::Value) -> String {
    match args {
        serde_json::Value::Object(map) => {
            let parts: Vec<String> = map
                .iter()
                .map(|(k, v)| {
                    let val = match v {
                        serde_json::Value::String(s) => truncate_for_summary(s, 80),
                        other => truncate_for_summary(&other.to_string(), 80),
                    };
                    format!("{k}: {val}")
                })
                .collect();
            parts.join(", ")
        }
        other => truncate_for_summary(&other.to_string(), 120),
    }
}

fn truncate_for_summary(input: &str, max_chars: usize) -> String {
    let mut chars = input.chars();
    let truncated: String = chars.by_ref().take(max_chars).collect();
    if chars.next().is_some() {
        format!("{truncated}...")
    } else {
        input.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn supervised_config() -> (Vec<String>, Vec<String>, AutonomyLevel) {
        (
            vec!["file_read".into(), "memory_recall".into()],
            vec!["shell".into()],
            AutonomyLevel::Supervised,
        )
    }

    #[test]
    fn auto_approve_tools_skip_prompt() {
        let (auto, ask, level) = supervised_config();
        let mgr = ApprovalManager::new(auto, ask, level);
        assert!(!mgr.needs_approval("file_read"));
        assert!(!mgr.needs_approval("memory_recall"));
    }

    #[test]
    fn always_ask_tools_always_prompt() {
        let (auto, ask, level) = supervised_config();
        let mgr = ApprovalManager::new(auto, ask, level);
        assert!(mgr.needs_approval("shell"));
    }

    #[test]
    fn unknown_tool_needs_approval_in_supervised() {
        let (auto, ask, level) = supervised_config();
        let mgr = ApprovalManager::new(auto, ask, level);
        assert!(mgr.needs_approval("file_write"));
    }

    #[test]
    fn full_autonomy_never_prompts() {
        let mgr = ApprovalManager::new(vec![], vec![], AutonomyLevel::Full);
        assert!(!mgr.needs_approval("shell"));
    }

    #[test]
    fn always_response_adds_to_session_allowlist() {
        let (auto, ask, level) = supervised_config();
        let mgr = ApprovalManager::new(auto, ask, level);
        assert!(mgr.needs_approval("file_write"));
        mgr.record_decision(
            "file_write",
            &serde_json::json!({}),
            ApprovalResponse::Always,
            "cli",
        );
        assert!(!mgr.needs_approval("file_write"));
    }

    #[test]
    fn always_ask_overrides_session_allowlist() {
        let (auto, ask, level) = supervised_config();
        let mgr = ApprovalManager::new(auto, ask, level);
        mgr.record_decision(
            "shell",
            &serde_json::json!({}),
            ApprovalResponse::Always,
            "cli",
        );
        assert!(mgr.needs_approval("shell"));
    }

    #[test]
    fn approval_response_serde_roundtrip() {
        let json = serde_json::to_string(&ApprovalResponse::Always).unwrap();
        assert_eq!(json, "\"always\"");
        let parsed: ApprovalResponse = serde_json::from_str("\"no\"").unwrap();
        assert_eq!(parsed, ApprovalResponse::No);
    }
}
