//! Memory hygiene — periodic pruning of stale entries.
//!
//! Cadence-gated (12h interval) cleanup that deletes Conversation and Daily
//! rows older than the configured retention period. Core memories are never
//! pruned. State is tracked in `memory_hygiene_state.json`.

use anyhow::Result;
use chrono::{DateTime, Duration, Local, Utc};
use clawseed_config::schema::MemoryConfig;
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

const HYGIENE_INTERVAL_HOURS: i64 = 12;
const STATE_FILE: &str = "memory_hygiene_state.json";

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct HygieneReport {
    pruned_conversation_rows: u64,
    pruned_daily_rows: u64,
}

impl HygieneReport {
    fn total_actions(&self) -> u64 {
        self.pruned_conversation_rows + self.pruned_daily_rows
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct HygieneState {
    last_run_at: Option<String>,
    last_report: HygieneReport,
}

/// Run memory hygiene if the cadence window has elapsed.
///
/// This function is intentionally best-effort: callers should log and continue on failure.
pub fn run_if_due(config: &MemoryConfig, workspace_dir: &Path) -> Result<()> {
    if !config.hygiene_enabled {
        return Ok(());
    }

    if !should_run_now(workspace_dir)? {
        return Ok(());
    }

    let report = HygieneReport {
        pruned_conversation_rows: prune_category_rows(
            workspace_dir,
            "conversation",
            config.conversation_retention_days,
        )?,
        pruned_daily_rows: prune_category_rows(
            workspace_dir,
            "daily",
            config.conversation_retention_days,
        )?,
    };

    write_state(workspace_dir, &report)?;

    if report.total_actions() > 0 {
        tracing::info!(
            "memory hygiene complete: pruned_conversation={} pruned_daily={}",
            report.pruned_conversation_rows,
            report.pruned_daily_rows,
        );
    }

    Ok(())
}

fn should_run_now(workspace_dir: &Path) -> Result<bool> {
    let path = state_path(workspace_dir);
    if !path.exists() {
        return Ok(true);
    }

    let raw = fs::read_to_string(&path)?;
    let state: HygieneState = match serde_json::from_str(&raw) {
        Ok(s) => s,
        Err(_) => return Ok(true),
    };

    let Some(last_run_at) = state.last_run_at else {
        return Ok(true);
    };

    let last = match DateTime::parse_from_rfc3339(&last_run_at) {
        Ok(ts) => ts.with_timezone(&Utc),
        Err(_) => return Ok(true),
    };

    Ok(Utc::now().signed_duration_since(last) >= Duration::hours(HYGIENE_INTERVAL_HOURS))
}

fn write_state(workspace_dir: &Path, report: &HygieneReport) -> Result<()> {
    let path = state_path(workspace_dir);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let state = HygieneState {
        last_run_at: Some(Utc::now().to_rfc3339()),
        last_report: report.clone(),
    };
    let json = serde_json::to_vec_pretty(&state)?;
    fs::write(path, json)?;
    Ok(())
}

fn state_path(workspace_dir: &Path) -> PathBuf {
    workspace_dir.join("state").join(STATE_FILE)
}

/// Delete rows of a given category older than the retention period.
///
/// Opens brain.db directly (WAL mode) and runs a DELETE query.
/// Returns the number of rows deleted.
fn prune_category_rows(workspace_dir: &Path, category: &str, retention_days: u32) -> Result<u64> {
    if retention_days == 0 {
        return Ok(0);
    }

    let db_path = workspace_dir.join("memory").join("brain.db");
    if !db_path.exists() {
        return Ok(0);
    }

    let conn = Connection::open(db_path)?;
    conn.execute_batch("PRAGMA journal_mode = WAL; PRAGMA synchronous = NORMAL;")?;
    let cutoff = (Local::now() - Duration::days(i64::from(retention_days))).to_rfc3339();

    let affected = conn.execute(
        "DELETE FROM memories WHERE category = ?1 AND updated_at < ?2",
        params![category, cutoff],
    )?;

    Ok(u64::try_from(affected).unwrap_or(0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sqlite::SqliteMemory;
    use crate::traits::{Memory, MemoryCategory};
    use tempfile::TempDir;

    fn hygiene_cfg() -> MemoryConfig {
        MemoryConfig {
            hygiene_enabled: true,
            conversation_retention_days: 30,
            ..Default::default()
        }
    }

    #[test]
    fn skips_when_disabled() {
        let tmp = TempDir::new().unwrap();
        let mut cfg = hygiene_cfg();
        cfg.hygiene_enabled = false;
        run_if_due(&cfg, tmp.path()).unwrap();
        // No state file written when disabled
        assert!(!state_path(tmp.path()).exists());
    }

    #[test]
    fn skips_second_run_within_cadence() {
        let tmp = TempDir::new().unwrap();
        let cfg = hygiene_cfg();

        run_if_due(&cfg, tmp.path()).unwrap();
        assert!(state_path(tmp.path()).exists());

        // Second run should be skipped (within 12h)
        run_if_due(&cfg, tmp.path()).unwrap();
    }

    #[tokio::test]
    async fn prunes_old_conversation_rows() {
        let tmp = TempDir::new().unwrap();
        let workspace = tmp.path();

        let mem = SqliteMemory::new(workspace).unwrap();
        mem.store("conv_old", "outdated", MemoryCategory::Conversation, None)
            .await
            .unwrap();
        mem.store("core_keep", "durable", MemoryCategory::Core, None)
            .await
            .unwrap();
        mem.store("daily_old", "old daily", MemoryCategory::Daily, None)
            .await
            .unwrap();
        drop(mem);

        // Backdate the old entries
        let db_path = workspace.join("memory").join("brain.db");
        let conn = Connection::open(&db_path).unwrap();
        let old_cutoff = (Local::now() - Duration::days(60)).to_rfc3339();
        conn.execute(
            "UPDATE memories SET created_at = ?1, updated_at = ?1 WHERE key IN ('conv_old', 'daily_old')",
            params![old_cutoff],
        )
        .unwrap();
        drop(conn);

        let mut cfg = hygiene_cfg();
        cfg.conversation_retention_days = 30;

        run_if_due(&cfg, workspace).unwrap();

        let mem2 = SqliteMemory::new(workspace).unwrap();
        assert!(
            mem2.get("conv_old").await.unwrap().is_none(),
            "old conversation should be pruned"
        );
        assert!(
            mem2.get("daily_old").await.unwrap().is_none(),
            "old daily should be pruned"
        );
        assert!(
            mem2.get("core_keep").await.unwrap().is_some(),
            "core memory should remain"
        );
    }

    #[test]
    fn prune_category_rows_no_db() {
        let tmp = TempDir::new().unwrap();
        let result = prune_category_rows(tmp.path(), "conversation", 30).unwrap();
        assert_eq!(result, 0);
    }

    #[test]
    fn prune_category_rows_zero_retention() {
        let tmp = TempDir::new().unwrap();
        let result = prune_category_rows(tmp.path(), "conversation", 0).unwrap();
        assert_eq!(result, 0);
    }
}
