//! Tool registry — convenience function for the binary to register all built-in tools.

use clawseed_api::tool::Tool;
use clawseed_config::schema::Config;
use std::path::PathBuf;
use std::sync::Arc;

#[cfg(not(feature = "android"))]
use crate::backup_tool::BackupTool;
use crate::calculator::CalculatorTool;
use crate::content_search::ContentSearchTool;
#[cfg(not(feature = "android"))]
use crate::cron_add::CronAddTool;
#[cfg(not(feature = "android"))]
use crate::cron_list::CronListTool;
#[cfg(not(feature = "android"))]
use crate::cron_remove::CronRemoveTool;
#[cfg(not(feature = "android"))]
use crate::cron_run::CronRunTool;
#[cfg(not(feature = "android"))]
use crate::cron_runs::CronRunsTool;
#[cfg(not(feature = "android"))]
use crate::cron_update::CronUpdateTool;
use crate::file_edit::FileEditTool;
use crate::file_read::FileReadTool;
use crate::file_write::FileWriteTool;
#[cfg(not(feature = "android"))]
use crate::git_operations::GitOperationsTool;
use crate::glob_search::GlobSearchTool;
use crate::http_request::HttpRequestTool;
use crate::knowledge_tool::KnowledgeTool;
use crate::llm_task::LlmTaskTool;
use crate::memory_export::MemoryExportTool;
use crate::memory_forget::MemoryForgetTool;
use crate::memory_purge::MemoryPurgeTool;
use crate::memory_recall::MemoryRecallTool;
use crate::memory_store::MemoryStoreTool;
use crate::model_routing_config::ModelRoutingConfigTool;
use crate::pdf_read::PdfReadTool;
use crate::shell::ShellTool;
use crate::web_fetch::WebFetchTool;
use crate::web_search_tool::WebSearchTool;

/// Return all built-in tools as boxed trait objects.
///
/// The binary calls this once during startup to populate the Agent's tool registry.
/// Tools that need runtime capabilities access them via `ctx.get::<T>()`.
///
/// `workspace_dir` is passed to tools that need it at construction time.
/// Other tools get workspace info from `ctx.workspace_dir()` at execution time.
///
/// Memory-dependent tools use the provided `memory` backend. Pass
/// `NoneMemory` in tests or when persistence is not needed.
///
/// Network tools (http_request, web_fetch, web_search) are included only
/// when their `enabled` flag is set in the config. When `allowed_domains`
/// is empty and the tool is enabled, all domains are permitted.
pub fn all_tools(
    #[cfg_attr(feature = "android", allow(unused))] workspace_dir: PathBuf,
    config: &Config,
    memory: Arc<dyn clawseed_api::memory_traits::Memory>,
) -> Vec<Box<dyn Tool>> {

    let mut tools: Vec<Box<dyn Tool>> = vec![
        Box::new(CalculatorTool::new()),
        Box::new(ContentSearchTool::new()),
        Box::new(FileEditTool::new()),
        Box::new(FileReadTool::new()),
        Box::new(FileWriteTool::new()),
        Box::new(GlobSearchTool::new()),
    ];

    #[cfg(not(feature = "android"))]
    {
        tools.push(Box::new(BackupTool::new(
            workspace_dir.clone(),
            Vec::new(),
            10,
        )));
        tools.push(Box::new(CronAddTool::new()));
        tools.push(Box::new(CronListTool::new()));
        tools.push(Box::new(CronRemoveTool::new()));
        tools.push(Box::new(CronRunTool::new()));
        tools.push(Box::new(CronRunsTool::new()));
        tools.push(Box::new(CronUpdateTool::new()));
        tools.push(Box::new(GitOperationsTool::new(workspace_dir.clone())));
    }

    // http_request: only include when enabled
    if config.http_request.enabled {
        let domains = if config.http_request.allowed_domains.is_empty() {
            vec!["*".to_string()]
        } else {
            config.http_request.allowed_domains.clone()
        };
        tools.push(Box::new(HttpRequestTool::new(
            domains, 1_048_576, // max_response_size: 1 MB
            30,        // timeout_secs
            false,     // allow_private_hosts
        )));
    }

    tools.push(Box::new(KnowledgeTool::new()));
    tools.push(Box::new(LlmTaskTool::new()));
    tools.push(Box::new(MemoryExportTool::new(memory.clone())));
    tools.push(Box::new(MemoryForgetTool::new(memory.clone())));
    tools.push(Box::new(MemoryPurgeTool::new(memory.clone())));
    tools.push(Box::new(MemoryRecallTool::new(memory.clone())));
    tools.push(Box::new(MemoryStoreTool::new(memory)));
    tools.push(Box::new(ModelRoutingConfigTool::new()));
    tools.push(Box::new(PdfReadTool::new()));
    tools.push(Box::new(ShellTool::new()));

    // web_fetch: only include when enabled
    if config.web_fetch.enabled {
        let domains = if config.web_fetch.allowed_domains.is_empty() {
            vec!["*".to_string()]
        } else {
            config.web_fetch.allowed_domains.clone()
        };
        tools.push(Box::new(WebFetchTool::new(
            domains,
            Vec::new(), // blocked_domains
            1_048_576,  // max_response_size: 1 MB
            30,         // timeout_secs
            Vec::new(), // allowed_private_hosts
        )));
    }

    // web_search: only include when enabled
    if config.web_search.enabled {
        let provider = config.web_search.provider.clone().unwrap_or_default();
        tools.push(Box::new(WebSearchTool::new_with_config(
            provider,
            config.web_search.brave_api_key.clone(),
            config.web_search.searxng_instance_url.clone(),
            5,  // max_results
            15, // timeout_secs
        )));
    }

    tools
}
