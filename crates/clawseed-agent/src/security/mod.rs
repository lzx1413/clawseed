//! Minimal security policy for the clawseed-agent crate.
//!
//! Provides basic command allowlist and autonomy enforcement.
//! SecurityPolicy also implements the Hook trait so it can intercept
//! tool calls before execution without per-tool checks.

pub mod pairing;
pub mod secrets;

use clawseed_api::hook::{Hook, HookResult, ToolCall, ToolExecutionResult};
use clawseed_config::schema::AutonomyConfig;
use std::path::Path;
use std::time::{Duration, Instant};

pub use clawseed_config::schema::AutonomyLevel;
pub use pairing::PairingGuard;
pub use secrets::{SecretStore, WebAuthnConfig, WebAuthnManager};

/// Minimal security policy based on autonomy config.
///
/// Enforces:
/// - Autonomy level (read-only blocks all actions)
/// - Command allowlist
/// - Action rate limiting
/// - Forbidden path arguments
pub struct SecurityPolicy {
    autonomy_level: AutonomyLevel,
    allowed_commands: Vec<String>,
    max_actions_per_hour: u32,
    rate_window: std::sync::Mutex<RateWindow>,
}

struct RateWindow {
    started_at: Instant,
    action_count: u32,
}

impl SecurityPolicy {
    const RATE_WINDOW: Duration = Duration::from_secs(60 * 60);

    /// Build a SecurityPolicy from the autonomy config and workspace dir.
    pub fn from_config(autonomy: &AutonomyConfig, _workspace_dir: &Path) -> Self {
        Self {
            autonomy_level: autonomy.level,
            allowed_commands: autonomy.allowed_commands.clone(),
            max_actions_per_hour: autonomy.max_actions_per_hour,
            rate_window: std::sync::Mutex::new(RateWindow {
                started_at: Instant::now(),
                action_count: 0,
            }),
        }
    }

    fn rate_window(&self) -> std::sync::MutexGuard<'_, RateWindow> {
        self.rate_window.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn reset_window_if_expired(window: &mut RateWindow) {
        if window.started_at.elapsed() >= Self::RATE_WINDOW {
            window.started_at = Instant::now();
            window.action_count = 0;
        }
    }

    /// Whether the agent is allowed to take actions at all.
    pub fn can_act(&self) -> bool {
        self.autonomy_level != AutonomyLevel::ReadOnly
    }

    /// Whether the action rate limit has been exceeded.
    ///
    /// When `max_actions_per_hour` is 0, no actions are allowed at all.
    pub fn is_rate_limited(&self) -> bool {
        if self.max_actions_per_hour == 0 {
            return true;
        }
        let mut window = self.rate_window();
        Self::reset_window_if_expired(&mut window);
        window.action_count >= self.max_actions_per_hour
    }

    /// Record an action and return whether the budget allows it.
    ///
    /// When `max_actions_per_hour` is 0, no actions are allowed.
    pub fn record_action(&self) -> bool {
        if self.max_actions_per_hour == 0 {
            return false;
        }
        let mut window = self.rate_window();
        Self::reset_window_if_expired(&mut window);
        if window.action_count >= self.max_actions_per_hour {
            return false;
        }
        window.action_count += 1;
        true
    }

    /// Validate a command for execution.
    ///
    /// Checks the allowlist when one is defined. If no allowlist is set,
    /// all commands are allowed (supervised/full autonomy).
    pub fn validate_command_execution(&self, command: &str, approved: bool) -> Result<(), String> {
        if self.autonomy_level == AutonomyLevel::ReadOnly {
            return Err("autonomy is read-only".to_string());
        }

        if self.allowed_commands.is_empty() {
            return Ok(());
        }

        let cmd_binary = command.split_whitespace().next().unwrap_or("");
        if self
            .allowed_commands
            .iter()
            .any(|allowed| cmd_binary == allowed.as_str())
        {
            // Allowed by command allowlist, but medium-risk commands
            // (like touch, rm, cp, mv) still need explicit approval.
            let medium_risk = ["touch", "rm", "cp", "mv", "mkdir", "chmod", "chown", "kill"];
            if medium_risk.contains(&cmd_binary) && !approved {
                return Err(format!(
                    "command '{cmd_binary}' requires explicit approval (medium risk)"
                ));
            }
            return Ok(());
        }

        Err(format!("command '{cmd_binary}' is not allowed"))
    }

    /// Check if a command contains a forbidden path argument.
    ///
    /// Returns `Some(path)` if a forbidden path is found, `None` otherwise.
    /// Also checks for input redirection to sensitive paths (e.g. `cat </etc/passwd`).
    pub fn forbidden_path_argument(&self, command: &str) -> Option<String> {
        let sensitive_paths = ["/etc/passwd", "/etc/shadow", "/etc/ssh", "/root/.ssh"];

        // Check for paths in the command string (including after redirection operators)
        for sensitive in &sensitive_paths {
            if command.contains(sensitive) {
                return Some((*sensitive).to_string());
            }
        }

        // Check for tilde-user paths (e.g. ~root/.ssh)
        regex_tilde_user_path(command)
    }

    /// Check if a command is allowed by the policy.
    pub fn is_command_allowed(&self, command: &str) -> bool {
        self.validate_command_execution(command, false).is_ok()
    }
}

#[cfg(test)]
impl SecurityPolicy {
    fn force_rate_window_started_at(&self, started_at: Instant) {
        let mut window = self.rate_window();
        window.started_at = started_at;
    }
}

/// Simple check for tilde-user paths like `~root/...`.
fn regex_tilde_user_path(command: &str) -> Option<String> {
    for part in command.split_whitespace() {
        if part.starts_with('~') && part.len() > 1 && !part.starts_with("~/") {
            // Tilde with a username (e.g. ~root) — potentially sensitive
            return Some(part.to_string());
        }
    }
    None
}

impl Hook for SecurityPolicy {
    fn before_tool_call(&self, call: &mut ToolCall) -> HookResult {
        // 1. Check autonomy level
        if !self.can_act() {
            return HookResult::Cancel("Autonomy level is read-only".into());
        }
        if self.is_rate_limited() {
            return HookResult::Cancel("Action rate limit exceeded".into());
        }

        // 2. For shell/exec tools: validate command
        if (call.name == "shell" || call.name == "exec")
            && let Some(cmd) = call.arguments.get("command").and_then(|v| v.as_str())
        {
            if let Some(forbidden) = self.forbidden_path_argument(cmd) {
                return HookResult::Cancel(format!("Forbidden path in command: {forbidden}"));
            }
            if !self.is_command_allowed(cmd) {
                return HookResult::Cancel(format!("Command not allowed by policy: {cmd}"));
            }
        }

        HookResult::Continue
    }

    fn after_tool_call(&self, _result: &ToolExecutionResult) -> HookResult {
        self.record_action();
        HookResult::Continue
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rate_limit_resets_after_one_hour_window() {
        let autonomy = AutonomyConfig {
            max_actions_per_hour: 1,
            ..AutonomyConfig::default()
        };
        let policy = SecurityPolicy::from_config(&autonomy, Path::new("."));

        assert!(!policy.is_rate_limited());
        assert!(policy.record_action());
        assert!(policy.is_rate_limited());

        policy.force_rate_window_started_at(Instant::now() - SecurityPolicy::RATE_WINDOW);

        assert!(!policy.is_rate_limited());
        assert!(policy.record_action());
        assert!(policy.is_rate_limited());
    }

    #[test]
    fn zero_action_budget_is_always_limited() {
        let autonomy = AutonomyConfig {
            max_actions_per_hour: 0,
            ..AutonomyConfig::default()
        };
        let policy = SecurityPolicy::from_config(&autonomy, Path::new("."));

        assert!(policy.is_rate_limited());
        assert!(!policy.record_action());
    }
}
