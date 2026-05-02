//! Minimal security policy for the clawseed-agent crate.
//!
//! Provides basic command allowlist and autonomy enforcement.
//! The full SecurityPolicy with risk scoring, path restrictions, etc.
//! is injected as a Hook at the binary level.

pub mod pairing;
pub mod secrets;

use clawseed_config::schema::AutonomyConfig;
use std::path::Path;

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
    action_count: std::sync::atomic::AtomicU32,
}

impl SecurityPolicy {
    /// Build a SecurityPolicy from the autonomy config and workspace dir.
    pub fn from_config(autonomy: &AutonomyConfig, _workspace_dir: &Path) -> Self {
        Self {
            autonomy_level: autonomy.level,
            allowed_commands: autonomy.allowed_commands.clone(),
            max_actions_per_hour: autonomy.max_actions_per_hour,
            action_count: std::sync::atomic::AtomicU32::new(0),
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
        use std::sync::atomic::Ordering;
        self.action_count.load(Ordering::Relaxed) >= self.max_actions_per_hour
    }

    /// Record an action and return whether the budget allows it.
    ///
    /// When `max_actions_per_hour` is 0, no actions are allowed.
    pub fn record_action(&self) -> bool {
        if self.max_actions_per_hour == 0 {
            return false;
        }
        use std::sync::atomic::Ordering;
        let current = self.action_count.fetch_add(1, Ordering::Relaxed);
        current < self.max_actions_per_hour
    }

    /// Validate a command for execution.
    ///
    /// Checks the allowlist when one is defined. If no allowlist is set,
    /// all commands are allowed (supervised/full autonomy).
    pub fn validate_command_execution(
        &self,
        command: &str,
        approved: bool,
    ) -> Result<(), String> {
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
        let sensitive_paths = [
            "/etc/passwd",
            "/etc/shadow",
            "/etc/ssh",
            "/root/.ssh",
        ];

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
