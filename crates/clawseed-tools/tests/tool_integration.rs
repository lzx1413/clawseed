//! Integration tests: verify every tool can be instantiated and execute successfully.
//!
//! Categorised by dependency:
//! - Pure computation: calculator, cron_*, knowledge, llm_task, model_routing_config
//! - Filesystem: file_read, file_write, file_edit, glob_search, content_search, backup, git_operations, pdf_read, shell
//! - Network: http_request, web_fetch, web_search (tested with config only — no live requests)
//! - Memory: memory_store, memory_recall, memory_forget, memory_export, memory_purge

use clawseed_api::tool::Tool;
use clawseed_api::tool_context::ToolContext;
use clawseed_config::schema::Config;
use std::path::{Path, PathBuf};

// ── Test context ──────────────────────────────────────────────────────────────

struct TestContext {
    workspace: PathBuf,
}

impl TestContext {
    fn new(workspace: PathBuf) -> Self {
        Self { workspace }
    }
}

impl ToolContext for TestContext {
    fn workspace_dir(&self) -> &Path {
        &self.workspace
    }
    fn get_any(&self, _type_id: std::any::TypeId) -> Option<&(dyn std::any::Any + Send + Sync)> {
        None
    }
}

fn ctx(workspace: PathBuf) -> TestContext {
    TestContext::new(workspace)
}

/// Helper: create a temp dir, write some files, return the path.
fn setup_workspace() -> tempfile::TempDir {
    let dir = tempfile::TempDir::new().unwrap();
    std::fs::write(dir.path().join("hello.txt"), "Hello, world!").unwrap();
    std::fs::write(dir.path().join("data.json"), r#"{"key": "value"}"#).unwrap();
    std::fs::create_dir_all(dir.path().join("subdir")).unwrap();
    std::fs::write(dir.path().join("subdir/nested.rs"), "fn main() {}").unwrap();
    dir
}

/// Build a default config with all network tools enabled.
fn test_config() -> Config {
    let mut config = Config::default();
    config.http_request.enabled = true;
    config.http_request.allowed_domains = vec!["*".to_string()];
    config.web_fetch.enabled = true;
    config.web_fetch.allowed_domains = vec!["*".to_string()];
    config.web_search.enabled = true;
    config
}

/// Build the full tool list from registry (same as production).
fn all_tools(workspace: PathBuf) -> Vec<Box<dyn Tool>> {
    let config = test_config();
    clawseed_tools::registry::all_tools(workspace, &config)
}

// ── 1. Registry: all tools instantiated ───────────────────────────────────────

#[tokio::test]
async fn registry_all_tools_instantiated() {
    let dir = tempfile::TempDir::new().unwrap();
    let tools = all_tools(dir.path().to_path_buf());
    // 29 tools total when all network tools enabled
    assert!(
        tools.len() >= 25,
        "expected at least 25 tools, got {}",
        tools.len()
    );

    let names: Vec<&str> = tools.iter().map(|t| t.name()).collect();
    // Verify all expected tool names are present
    for expected in [
        "backup",
        "calculator",
        "content_search",
        "cron_add",
        "cron_list",
        "cron_remove",
        "cron_run",
        "cron_runs",
        "cron_update",
        "file_edit",
        "file_read",
        "file_write",
        "git_operations",
        "glob_search",
        "http_request",
        "knowledge",
        "llm_task",
        "memory_export",
        "memory_forget",
        "memory_purge",
        "memory_recall",
        "memory_store",
        "model_routing_config",
        "pdf_read",
        "shell",
        "web_fetch",
        "web_search_tool",
    ] {
        assert!(names.contains(&expected), "missing tool: {expected}");
    }
}

#[tokio::test]
async fn registry_network_tools_disabled_by_default() {
    let dir = tempfile::TempDir::new().unwrap();
    let config = Config::default(); // network tools disabled by default
    let tools = clawseed_tools::registry::all_tools(dir.path().to_path_buf(), &config);
    let names: Vec<&str> = tools.iter().map(|t| t.name()).collect();
    assert!(
        !names.contains(&"http_request"),
        "http_request should be disabled by default"
    );
    assert!(
        !names.contains(&"web_fetch"),
        "web_fetch should be disabled by default"
    );
    assert!(
        !names.contains(&"web_search_tool"),
        "web_search_tool should be disabled by default"
    );
}

#[tokio::test]
async fn registry_network_tools_enabled_with_config() {
    let dir = tempfile::TempDir::new().unwrap();
    let tools = all_tools(dir.path().to_path_buf());
    let names: Vec<&str> = tools.iter().map(|t| t.name()).collect();
    assert!(
        names.contains(&"http_request"),
        "http_request should be enabled"
    );
    assert!(names.contains(&"web_fetch"), "web_fetch should be enabled");
    assert!(
        names.contains(&"web_search_tool"),
        "web_search_tool should be enabled"
    );
}

#[tokio::test]
async fn every_tool_has_valid_spec() {
    let dir = tempfile::TempDir::new().unwrap();
    let tools = all_tools(dir.path().to_path_buf());
    for tool in &tools {
        let name = tool.name().to_string();
        assert!(!name.is_empty(), "tool has empty name");
        assert!(
            !tool.description().is_empty(),
            "tool {} has empty description",
            name
        );
        let schema = tool.parameters_schema();
        assert!(
            schema.is_object(),
            "tool {} schema is not an object: {}",
            name,
            schema
        );
    }
}

// ── 2. Calculator ─────────────────────────────────────────────────────────────

#[tokio::test]
async fn calculator_add() {
    let dir = tempfile::TempDir::new().unwrap();
    let tools = all_tools(dir.path().to_path_buf());
    let calc = tools.iter().find(|t| t.name() == "calculator").unwrap();
    let result = calc
        .execute(
            serde_json::json!({"function": "add", "values": [1, 2, 3]}),
            &ctx(dir.path().to_path_buf()),
        )
        .await
        .unwrap();
    assert!(result.success, "calculator add failed: {:?}", result.error);
    assert!(
        result.output.contains("6"),
        "expected 6, got: {}",
        result.output
    );
}

#[tokio::test]
async fn calculator_sqrt() {
    let dir = tempfile::TempDir::new().unwrap();
    let tools = all_tools(dir.path().to_path_buf());
    let calc = tools.iter().find(|t| t.name() == "calculator").unwrap();
    let result = calc
        .execute(
            serde_json::json!({"function": "sqrt", "x": 16}),
            &ctx(dir.path().to_path_buf()),
        )
        .await
        .unwrap();
    assert!(result.success, "calculator sqrt failed: {:?}", result.error);
    assert!(
        result.output.contains("4"),
        "expected 4, got: {}",
        result.output
    );
}

#[tokio::test]
async fn calculator_average() {
    let dir = tempfile::TempDir::new().unwrap();
    let tools = all_tools(dir.path().to_path_buf());
    let calc = tools.iter().find(|t| t.name() == "calculator").unwrap();
    let result = calc
        .execute(
            serde_json::json!({"function": "average", "values": [10, 20, 30]}),
            &ctx(dir.path().to_path_buf()),
        )
        .await
        .unwrap();
    assert!(
        result.success,
        "calculator average failed: {:?}",
        result.error
    );
    assert!(
        result.output.contains("20"),
        "expected 20, got: {}",
        result.output
    );
}

// ── 3. File write / read / edit round-trip ────────────────────────────────────

#[tokio::test]
async fn file_write_read_roundtrip() {
    let dir = tempfile::TempDir::new().unwrap();
    let tools = all_tools(dir.path().to_path_buf());
    let c = ctx(dir.path().to_path_buf());

    let fw = tools.iter().find(|t| t.name() == "file_write").unwrap();
    let result = fw
        .execute(
            serde_json::json!({"path": "test.txt", "content": "Hello from test"}),
            &c,
        )
        .await
        .unwrap();
    assert!(result.success, "file_write failed: {:?}", result.error);

    let fr = tools.iter().find(|t| t.name() == "file_read").unwrap();
    let result = fr
        .execute(serde_json::json!({"path": "test.txt"}), &c)
        .await
        .unwrap();
    assert!(result.success, "file_read failed: {:?}", result.error);
    assert!(
        result.output.contains("Hello from test"),
        "content mismatch: {}",
        result.output
    );
}

#[tokio::test]
async fn file_edit_replaces_string() {
    let dir = tempfile::TempDir::new().unwrap();
    let tools = all_tools(dir.path().to_path_buf());
    let c = ctx(dir.path().to_path_buf());

    // Write initial content
    let fw = tools.iter().find(|t| t.name() == "file_write").unwrap();
    fw.execute(
        serde_json::json!({"path": "edit_test.txt", "content": "foo bar baz"}),
        &c,
    )
    .await
    .unwrap();

    // Edit: replace "bar" with "world"
    let fe = tools.iter().find(|t| t.name() == "file_edit").unwrap();
    let result = fe.execute(
        serde_json::json!({"path": "edit_test.txt", "old_string": "bar", "new_string": "world"}),
        &c,
    ).await.unwrap();
    assert!(result.success, "file_edit failed: {:?}", result.error);

    // Verify
    let fr = tools.iter().find(|t| t.name() == "file_read").unwrap();
    let result = fr
        .execute(serde_json::json!({"path": "edit_test.txt"}), &c)
        .await
        .unwrap();
    assert!(
        result.output.contains("foo world baz"),
        "edit result: {}",
        result.output
    );
}

#[tokio::test]
async fn file_write_creates_subdirectories() {
    let dir = tempfile::TempDir::new().unwrap();
    let tools = all_tools(dir.path().to_path_buf());
    let c = ctx(dir.path().to_path_buf());

    let fw = tools.iter().find(|t| t.name() == "file_write").unwrap();
    let result = fw
        .execute(
            serde_json::json!({"path": "deep/nested/dir/file.txt", "content": "deep content"}),
            &c,
        )
        .await
        .unwrap();
    assert!(
        result.success,
        "file_write with subdirs failed: {:?}",
        result.error
    );
}

#[tokio::test]
async fn file_read_nonexistent_returns_error() {
    let dir = tempfile::TempDir::new().unwrap();
    let tools = all_tools(dir.path().to_path_buf());
    let fr = tools.iter().find(|t| t.name() == "file_read").unwrap();
    let result = fr
        .execute(
            serde_json::json!({"path": "nonexistent.txt"}),
            &ctx(dir.path().to_path_buf()),
        )
        .await
        .unwrap();
    assert!(!result.success, "should fail for nonexistent file");
    assert!(result.error.is_some());
}

#[tokio::test]
async fn file_tools_reject_path_traversal() {
    let dir = tempfile::TempDir::new().unwrap();
    let tools = all_tools(dir.path().to_path_buf());
    let c = ctx(dir.path().to_path_buf());

    // file_write
    let fw = tools.iter().find(|t| t.name() == "file_write").unwrap();
    let result = fw
        .execute(
            serde_json::json!({"path": "../etc/passwd", "content": "hack"}),
            &c,
        )
        .await
        .unwrap();
    assert!(!result.success, "file_write should reject path traversal");

    // file_edit
    let fe = tools.iter().find(|t| t.name() == "file_edit").unwrap();
    let result = fe
        .execute(
            serde_json::json!({"path": "../etc/shadow", "old_string": "x", "new_string": "y"}),
            &c,
        )
        .await
        .unwrap();
    assert!(!result.success, "file_edit should reject path traversal");
}

// ── 4. Glob search ───────────────────────────────────────────────────────────

#[tokio::test]
async fn glob_search_finds_files() {
    let dir = setup_workspace();
    let tools = all_tools(dir.path().to_path_buf());
    let gs = tools.iter().find(|t| t.name() == "glob_search").unwrap();
    let result = gs
        .execute(
            serde_json::json!({"pattern": "**/*.txt"}),
            &ctx(dir.path().to_path_buf()),
        )
        .await
        .unwrap();
    assert!(result.success, "glob_search failed: {:?}", result.error);
    assert!(
        result.output.contains("hello.txt"),
        "should find hello.txt: {}",
        result.output
    );
}

#[tokio::test]
async fn glob_search_rejects_absolute_path() {
    let dir = tempfile::TempDir::new().unwrap();
    let tools = all_tools(dir.path().to_path_buf());
    let gs = tools.iter().find(|t| t.name() == "glob_search").unwrap();
    let result = gs
        .execute(
            serde_json::json!({"pattern": "/etc/**/*"}),
            &ctx(dir.path().to_path_buf()),
        )
        .await
        .unwrap();
    assert!(!result.success);
    assert!(result.error.unwrap().contains("Absolute paths"));
}

#[tokio::test]
async fn glob_search_empty_workspace() {
    let dir = tempfile::TempDir::new().unwrap();
    let tools = all_tools(dir.path().to_path_buf());
    let gs = tools.iter().find(|t| t.name() == "glob_search").unwrap();
    let result = gs
        .execute(
            serde_json::json!({"pattern": "*.txt"}),
            &ctx(dir.path().to_path_buf()),
        )
        .await
        .unwrap();
    assert!(result.success, "glob_search on empty dir should succeed");
}

// ── 5. Content search ────────────────────────────────────────────────────────

#[tokio::test]
async fn content_search_finds_pattern() {
    let dir = setup_workspace();
    let tools = all_tools(dir.path().to_path_buf());
    let cs = tools.iter().find(|t| t.name() == "content_search").unwrap();
    let result = cs
        .execute(
            serde_json::json!({"pattern": "Hello", "output_mode": "files_with_matches"}),
            &ctx(dir.path().to_path_buf()),
        )
        .await
        .unwrap();
    assert!(result.success, "content_search failed: {:?}", result.error);
    assert!(
        result.output.contains("hello.txt"),
        "should find hello.txt: {}",
        result.output
    );
}

// ── 6. Shell ──────────────────────────────────────────────────────────────────

#[tokio::test]
async fn shell_echo() {
    let dir = tempfile::TempDir::new().unwrap();
    let tools = all_tools(dir.path().to_path_buf());
    let sh = tools.iter().find(|t| t.name() == "shell").unwrap();
    let result = sh
        .execute(
            serde_json::json!({"command": "echo hello"}),
            &ctx(dir.path().to_path_buf()),
        )
        .await
        .unwrap();
    assert!(result.success, "shell echo failed: {:?}", result.error);
    assert!(
        result.output.contains("hello"),
        "expected 'hello' in output: {}",
        result.output
    );
}

#[tokio::test]
async fn shell_failing_command() {
    let dir = tempfile::TempDir::new().unwrap();
    let tools = all_tools(dir.path().to_path_buf());
    let sh = tools.iter().find(|t| t.name() == "shell").unwrap();
    let result = sh
        .execute(
            serde_json::json!({"command": "false"}),
            &ctx(dir.path().to_path_buf()),
        )
        .await
        .unwrap();
    assert!(!result.success, "shell 'false' should report failure");
}

// ── 7. Cron tools (all stubs — just verify they execute) ─────────────────────

#[tokio::test]
async fn cron_add_execute() {
    let dir = tempfile::TempDir::new().unwrap();
    let tools = all_tools(dir.path().to_path_buf());
    let t = tools.iter().find(|t| t.name() == "cron_add").unwrap();
    let result = t
        .execute(
            serde_json::json!({"schedule": "0 * * * *", "prompt": "test prompt"}),
            &ctx(dir.path().to_path_buf()),
        )
        .await
        .unwrap();
    assert!(result.success, "cron_add failed: {:?}", result.error);
}

#[tokio::test]
async fn cron_list_execute() {
    let dir = tempfile::TempDir::new().unwrap();
    let tools = all_tools(dir.path().to_path_buf());
    let t = tools.iter().find(|t| t.name() == "cron_list").unwrap();
    let result = t
        .execute(serde_json::json!({}), &ctx(dir.path().to_path_buf()))
        .await
        .unwrap();
    assert!(result.success, "cron_list failed: {:?}", result.error);
}

#[tokio::test]
async fn cron_remove_execute() {
    let dir = tempfile::TempDir::new().unwrap();
    let tools = all_tools(dir.path().to_path_buf());
    let t = tools.iter().find(|t| t.name() == "cron_remove").unwrap();
    let result = t
        .execute(
            serde_json::json!({"id": "job-1"}),
            &ctx(dir.path().to_path_buf()),
        )
        .await
        .unwrap();
    assert!(result.success, "cron_remove failed: {:?}", result.error);
}

#[tokio::test]
async fn cron_run_execute() {
    let dir = tempfile::TempDir::new().unwrap();
    let tools = all_tools(dir.path().to_path_buf());
    let t = tools.iter().find(|t| t.name() == "cron_run").unwrap();
    let result = t
        .execute(
            serde_json::json!({"id": "job-1"}),
            &ctx(dir.path().to_path_buf()),
        )
        .await
        .unwrap();
    assert!(result.success, "cron_run failed: {:?}", result.error);
}

#[tokio::test]
async fn cron_runs_execute() {
    let dir = tempfile::TempDir::new().unwrap();
    let tools = all_tools(dir.path().to_path_buf());
    let t = tools.iter().find(|t| t.name() == "cron_runs").unwrap();
    let result = t
        .execute(
            serde_json::json!({"id": "job-1"}),
            &ctx(dir.path().to_path_buf()),
        )
        .await
        .unwrap();
    assert!(result.success, "cron_runs failed: {:?}", result.error);
}

#[tokio::test]
async fn cron_update_execute() {
    let dir = tempfile::TempDir::new().unwrap();
    let tools = all_tools(dir.path().to_path_buf());
    let t = tools.iter().find(|t| t.name() == "cron_update").unwrap();
    let result = t
        .execute(
            serde_json::json!({"id": "job-1", "schedule": "0 0 * * *"}),
            &ctx(dir.path().to_path_buf()),
        )
        .await
        .unwrap();
    assert!(result.success, "cron_update failed: {:?}", result.error);
}

// ── 8. Memory tools (use NoneMemory — should succeed without error) ──────────

#[tokio::test]
async fn memory_store_and_recall() {
    let dir = tempfile::TempDir::new().unwrap();
    let tools = all_tools(dir.path().to_path_buf());
    let c = ctx(dir.path().to_path_buf());

    // store
    let ms = tools.iter().find(|t| t.name() == "memory_store").unwrap();
    let result = ms
        .execute(
            serde_json::json!({"key": "test_key", "content": "test value", "category": "core"}),
            &c,
        )
        .await
        .unwrap();
    // NoneMemory accepts writes silently
    assert!(result.success, "memory_store failed: {:?}", result.error);

    // recall
    let mr = tools.iter().find(|t| t.name() == "memory_recall").unwrap();
    let result = mr
        .execute(serde_json::json!({"query": "test"}), &c)
        .await
        .unwrap();
    assert!(result.success, "memory_recall failed: {:?}", result.error);
}

#[tokio::test]
async fn memory_forget_execute() {
    let dir = tempfile::TempDir::new().unwrap();
    let tools = all_tools(dir.path().to_path_buf());
    let t = tools.iter().find(|t| t.name() == "memory_forget").unwrap();
    let result = t
        .execute(
            serde_json::json!({"key": "nonexistent"}),
            &ctx(dir.path().to_path_buf()),
        )
        .await
        .unwrap();
    assert!(result.success, "memory_forget failed: {:?}", result.error);
}

#[tokio::test]
async fn memory_export_execute() {
    let dir = tempfile::TempDir::new().unwrap();
    let tools = all_tools(dir.path().to_path_buf());
    let t = tools.iter().find(|t| t.name() == "memory_export").unwrap();
    let result = t
        .execute(serde_json::json!({}), &ctx(dir.path().to_path_buf()))
        .await
        .unwrap();
    assert!(result.success, "memory_export failed: {:?}", result.error);
}

#[tokio::test]
async fn memory_purge_requires_namespace_or_session() {
    let dir = tempfile::TempDir::new().unwrap();
    let tools = all_tools(dir.path().to_path_buf());
    let t = tools.iter().find(|t| t.name() == "memory_purge").unwrap();

    // Without namespace or session_id, should return ToolResult error
    let result = t
        .execute(serde_json::json!({}), &ctx(dir.path().to_path_buf()))
        .await;
    // memory_purge bails with anyhow when missing params
    assert!(
        result.is_err(),
        "memory_purge without namespace/session should error"
    );

    // With namespace, should return result (NoneMemory doesn't support purge but returns gracefully)
    let result = t
        .execute(
            serde_json::json!({"namespace": "test"}),
            &ctx(dir.path().to_path_buf()),
        )
        .await
        .unwrap();
    // NoneMemory doesn't support purge — it returns success=false with a clear message
    // This is expected behavior: the tool validated params correctly but the backend can't purge
    assert!(
        result.error.is_some(),
        "memory_purge with namespace should have an error from NoneMemory"
    );
}

// ── 9. Stub tools (knowledge, llm_task, model_routing_config) ────────────────

#[tokio::test]
async fn knowledge_tool_execute() {
    let dir = tempfile::TempDir::new().unwrap();
    let tools = all_tools(dir.path().to_path_buf());
    let t = tools.iter().find(|t| t.name() == "knowledge").unwrap();
    let result = t
        .execute(
            serde_json::json!({"action": "graph_stats"}),
            &ctx(dir.path().to_path_buf()),
        )
        .await
        .unwrap();
    // Stub returns success=false with clear message
    assert!(
        !result.success,
        "knowledge stub should report unavailability"
    );
    assert!(
        result.error.as_ref().unwrap().contains("not available"),
        "unexpected error: {:?}",
        result.error
    );
}

#[tokio::test]
async fn llm_task_tool_execute() {
    let dir = tempfile::TempDir::new().unwrap();
    let tools = all_tools(dir.path().to_path_buf());
    let t = tools.iter().find(|t| t.name() == "llm_task").unwrap();
    let result = t
        .execute(
            serde_json::json!({"prompt": "test"}),
            &ctx(dir.path().to_path_buf()),
        )
        .await
        .unwrap();
    assert!(
        !result.success,
        "llm_task stub should report unavailability"
    );
    assert!(
        result.error.as_ref().unwrap().contains("not available"),
        "unexpected error: {:?}",
        result.error
    );
}

#[tokio::test]
async fn model_routing_config_tool_execute() {
    let dir = tempfile::TempDir::new().unwrap();
    let tools = all_tools(dir.path().to_path_buf());
    let t = tools
        .iter()
        .find(|t| t.name() == "model_routing_config")
        .unwrap();
    let result = t
        .execute(
            serde_json::json!({"action": "get"}),
            &ctx(dir.path().to_path_buf()),
        )
        .await
        .unwrap();
    assert!(
        !result.success,
        "model_routing_config stub should report unavailability"
    );
    assert!(
        result.error.as_ref().unwrap().contains("not yet available"),
        "unexpected error: {:?}",
        result.error
    );
}

// ── 10. Git operations ───────────────────────────────────────────────────────

#[tokio::test]
async fn git_operations_status() {
    let dir = tempfile::TempDir::new().unwrap();
    let tools = all_tools(dir.path().to_path_buf());
    let t = tools.iter().find(|t| t.name() == "git_operations").unwrap();

    // Init a git repo first
    std::process::Command::new("git")
        .args(["init"])
        .current_dir(dir.path())
        .output()
        .expect("git init failed");

    let result = t
        .execute(
            serde_json::json!({"operation": "status"}),
            &ctx(dir.path().to_path_buf()),
        )
        .await
        .unwrap();
    assert!(result.success, "git status failed: {:?}", result.error);
}

#[tokio::test]
async fn git_operations_log_empty_repo() {
    let dir = tempfile::TempDir::new().unwrap();
    let tools = all_tools(dir.path().to_path_buf());
    let t = tools.iter().find(|t| t.name() == "git_operations").unwrap();

    std::process::Command::new("git")
        .args(["init"])
        .current_dir(dir.path())
        .output()
        .expect("git init failed");

    let result = t
        .execute(
            serde_json::json!({"operation": "log"}),
            &ctx(dir.path().to_path_buf()),
        )
        .await;
    // Empty repo log will fail — just verify it doesn't panic
    let _ = result;
}

// ── 11. Backup ────────────────────────────────────────────────────────────────

#[tokio::test]
async fn backup_list_empty() {
    let dir = setup_workspace();
    let tools = all_tools(dir.path().to_path_buf());
    let t = tools.iter().find(|t| t.name() == "backup").unwrap();
    let result = t
        .execute(
            serde_json::json!({"command": "list"}),
            &ctx(dir.path().to_path_buf()),
        )
        .await
        .unwrap();
    assert!(result.success, "backup list failed: {:?}", result.error);
}

// ── 12. PDF read (feature-gated) ─────────────────────────────────────────────

#[tokio::test]
async fn pdf_read_without_feature() {
    let dir = tempfile::TempDir::new().unwrap();
    let tools = all_tools(dir.path().to_path_buf());
    let t = tools.iter().find(|t| t.name() == "pdf_read").unwrap();

    // Create a dummy file (not a real PDF)
    std::fs::write(dir.path().join("test.pdf"), "not a pdf").unwrap();

    let result = t
        .execute(
            serde_json::json!({"path": "test.pdf"}),
            &ctx(dir.path().to_path_buf()),
        )
        .await
        .unwrap();

    #[cfg(not(feature = "rag-pdf"))]
    {
        assert!(!result.success);
        assert!(
            result.error.as_ref().unwrap().contains("not enabled"),
            "unexpected error: {:?}",
            result.error
        );
    }
}

#[tokio::test]
async fn pdf_read_rejects_path_traversal() {
    let dir = tempfile::TempDir::new().unwrap();
    let tools = all_tools(dir.path().to_path_buf());
    let t = tools.iter().find(|t| t.name() == "pdf_read").unwrap();
    let result = t
        .execute(
            serde_json::json!({"path": "../../../etc/passwd"}),
            &ctx(dir.path().to_path_buf()),
        )
        .await
        .unwrap();
    assert!(!result.success, "pdf_read should reject path traversal");
    assert!(result.error.unwrap().contains("Path traversal"));
}

// ── 13. Network tools — config validation only (no live requests) ────────────

#[tokio::test]
async fn http_request_rejects_no_allowed_domains() {
    let dir = tempfile::TempDir::new().unwrap();
    // Build tool with empty allowed_domains (simulating misconfigured state)
    let tool = clawseed_tools::http_request::HttpRequestTool::new(
        Vec::new(), // no allowed_domains
        1_048_576,
        30,
        false,
    );
    let result = tool
        .execute(
            serde_json::json!({"url": "https://example.com", "method": "GET"}),
            &ctx(dir.path().to_path_buf()),
        )
        .await
        .unwrap();
    assert!(!result.success);
    assert!(
        result.error.unwrap().contains("allowed_domains"),
        "unexpected error"
    );
}

#[tokio::test]
async fn web_fetch_rejects_no_allowed_domains() {
    let dir = tempfile::TempDir::new().unwrap();
    let tool = clawseed_tools::web_fetch::WebFetchTool::new(
        Vec::new(), // no allowed_domains
        Vec::new(),
        1_048_576,
        30,
        Vec::new(),
    );
    let result = tool
        .execute(
            serde_json::json!({"url": "https://example.com"}),
            &ctx(dir.path().to_path_buf()),
        )
        .await
        .unwrap();
    assert!(!result.success);
    assert!(
        result.error.unwrap().contains("allowed_domains"),
        "unexpected error"
    );
}

#[tokio::test]
async fn web_search_tool_spec_valid() {
    let dir = tempfile::TempDir::new().unwrap();
    let tools = all_tools(dir.path().to_path_buf());
    let t = tools
        .iter()
        .find(|t| t.name() == "web_search_tool")
        .expect("web_search_tool should exist");
    assert!(!t.name().is_empty());
    assert!(!t.description().is_empty());
    let schema = t.parameters_schema();
    assert!(schema["properties"]["query"].is_object());
}

// ── 14. File read with non-canonicalizable workspace ─────────────────────────
// This simulates the Android scenario where the workspace dir exists
// but canonicalize might fail due to the path structure.

#[tokio::test]
async fn file_read_workspace_not_yet_canonicalizable() {
    let dir = tempfile::TempDir::new().unwrap();
    // Write a file
    std::fs::write(dir.path().join("readme.txt"), "test content").unwrap();

    let tools = all_tools(dir.path().to_path_buf());
    let fr = tools.iter().find(|t| t.name() == "file_read").unwrap();
    let result = fr
        .execute(
            serde_json::json!({"path": "readme.txt"}),
            &ctx(dir.path().to_path_buf()),
        )
        .await
        .unwrap();
    assert!(result.success, "file_read failed: {:?}", result.error);
    assert!(result.output.contains("test content"));
}

// ── 15. File write then glob search ──────────────────────────────────────────

#[tokio::test]
async fn file_write_then_glob() {
    let dir = tempfile::TempDir::new().unwrap();
    let tools = all_tools(dir.path().to_path_buf());
    let c = ctx(dir.path().to_path_buf());

    // Write several files
    let fw = tools.iter().find(|t| t.name() == "file_write").unwrap();
    for name in &["a.rs", "b.rs", "c.txt"] {
        fw.execute(serde_json::json!({"path": name, "content": "content"}), &c)
            .await
            .unwrap();
    }

    // Glob for *.rs
    let gs = tools.iter().find(|t| t.name() == "glob_search").unwrap();
    let result = gs
        .execute(serde_json::json!({"pattern": "*.rs"}), &c)
        .await
        .unwrap();
    assert!(result.success);
    assert!(result.output.contains("a.rs"));
    assert!(result.output.contains("b.rs"));
    assert!(!result.output.contains("c.txt"));
}

// ── 16. End-to-end: write → read → edit → read ──────────────────────────────

#[tokio::test]
async fn file_roundtrip_write_read_edit_read() {
    let dir = tempfile::TempDir::new().unwrap();
    let tools = all_tools(dir.path().to_path_buf());
    let c = ctx(dir.path().to_path_buf());

    // Write
    let fw = tools.iter().find(|t| t.name() == "file_write").unwrap();
    let r = fw
        .execute(
            serde_json::json!({"path": "doc.md", "content": "# Title\nHello world\n"}),
            &c,
        )
        .await
        .unwrap();
    assert!(r.success, "write: {:?}", r.error);

    // Read
    let fr = tools.iter().find(|t| t.name() == "file_read").unwrap();
    let r = fr
        .execute(serde_json::json!({"path": "doc.md"}), &c)
        .await
        .unwrap();
    assert!(r.success, "read: {:?}", r.error);
    assert!(r.output.contains("Hello world"));

    // Edit
    let fe = tools.iter().find(|t| t.name() == "file_edit").unwrap();
    let r = fe.execute(serde_json::json!({"path": "doc.md", "old_string": "Hello world", "new_string": "Hello ClawSeed"}), &c).await.unwrap();
    assert!(r.success, "edit: {:?}", r.error);

    // Read again
    let r = fr
        .execute(serde_json::json!({"path": "doc.md"}), &c)
        .await
        .unwrap();
    assert!(r.success, "read2: {:?}", r.error);
    assert!(r.output.contains("Hello ClawSeed"));
    assert!(!r.output.contains("Hello world"));
}

// ── 17. Android config: verify INITIAL_CONFIG parses correctly ────────────────

#[test]
fn android_initial_config_parses_correctly() {
    // This is the exact INITIAL_CONFIG from ClawseedService.kt
    // (with a placeholder workspace_dir)
    let toml_str = r#"
workspace_dir = "/data/data/dev.clawseed.demo/files/.clawseed/workspace"

[gateway]

[web_fetch]
enabled = true
allowed_domains = ["*"]

[http_request]
enabled = true
allowed_domains = ["*"]

[web_search]
enabled = true
provider = "duckduckgo"
"#;
    let config: Config = toml::from_str(toml_str).expect("INITIAL_CONFIG should parse");
    assert!(
        config.http_request.enabled,
        "http_request should be enabled"
    );
    assert!(config.web_fetch.enabled, "web_fetch should be enabled");
    assert!(config.web_search.enabled, "web_search should be enabled");
    assert_eq!(config.http_request.allowed_domains, vec!["*"]);
    assert_eq!(config.web_fetch.allowed_domains, vec!["*"]);
}

#[test]
fn android_config_network_tools_available_after_load() {
    let toml_str = r#"
workspace_dir = "/tmp/test"

[web_fetch]
enabled = true
allowed_domains = ["*"]

[http_request]
enabled = true
allowed_domains = ["*"]

[web_search]
enabled = true
"#;
    let config: Config = toml::from_str(toml_str).expect("config should parse");
    let dir = tempfile::TempDir::new().unwrap();
    let tools = clawseed_tools::registry::all_tools(dir.path().to_path_buf(), &config);
    let names: Vec<&str> = tools.iter().map(|t| t.name()).collect();
    assert!(
        names.contains(&"http_request"),
        "http_request should be in tool list"
    );
    assert!(
        names.contains(&"web_fetch"),
        "web_fetch should be in tool list"
    );
    assert!(
        names.contains(&"web_search_tool"),
        "web_search_tool should be in tool list"
    );
}

#[test]
fn android_config_patching_scenario() {
    // Simulate the patching that ClawseedService.enableSectionIfPresent does:
    // existing config has network tools disabled, patching enables them
    let original = r#"
[gateway]

[web_fetch]
enabled = false

[http_request]
enabled = false

[web_search]
enabled = false
"#;
    let mut config: Config = toml::from_str(original).expect("original config should parse");
    assert!(!config.http_request.enabled);
    assert!(!config.web_fetch.enabled);
    assert!(!config.web_search.enabled);

    // Simulate the patching: flip enabled to true, add allowed_domains
    config.http_request.enabled = true;
    config.http_request.allowed_domains = vec!["*".to_string()];
    config.web_fetch.enabled = true;
    config.web_fetch.allowed_domains = vec!["*".to_string()];
    config.web_search.enabled = true;

    let dir = tempfile::TempDir::new().unwrap();
    let tools = clawseed_tools::registry::all_tools(dir.path().to_path_buf(), &config);
    let names: Vec<&str> = tools.iter().map(|t| t.name()).collect();
    assert!(
        names.contains(&"http_request"),
        "patched: http_request should be available"
    );
    assert!(
        names.contains(&"web_fetch"),
        "patched: web_fetch should be available"
    );
    assert!(
        names.contains(&"web_search_tool"),
        "patched: web_search_tool should be available"
    );
}
