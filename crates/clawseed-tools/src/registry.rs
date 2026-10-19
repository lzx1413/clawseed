//! Tool registry — convenience function for the binary to register all built-in tools.

use clawseed_api::tool::Tool;
use clawseed_config::schema::Config;
use std::path::PathBuf;
use std::sync::Arc;

fn is_deepseek_provider_name(value: &str) -> bool {
    let normalized = value.trim().to_ascii_lowercase();
    normalized == "deepseek"
        || normalized == "deepseek-search"
        || normalized == "deepseek_search"
        || normalized
            .strip_prefix("custom:")
            .and_then(|url| reqwest::Url::parse(url).ok())
            .and_then(|url| url.host_str().map(|host| host == "api.deepseek.com"))
            .unwrap_or(false)
}

fn is_deepseek_base_url(value: &str) -> bool {
    reqwest::Url::parse(value)
        .ok()
        .and_then(|url| url.host_str().map(|host| host == "api.deepseek.com"))
        .unwrap_or(false)
}

fn is_deepseek_model_provider(
    key: &str,
    profile: &clawseed_config::schema::ModelProviderConfig,
) -> bool {
    is_deepseek_provider_name(key)
        || profile
            .name
            .as_deref()
            .is_some_and(is_deepseek_provider_name)
        || profile
            .base_url
            .as_deref()
            .is_some_and(is_deepseek_base_url)
}

fn resolve_deepseek_search_key(config: &Config) -> Option<String> {
    if let Some(key) = config
        .web_search
        .deepseek_api_key
        .as_deref()
        .filter(|key| !key.trim().is_empty())
    {
        return Some(key.trim().to_owned());
    }

    if let Some(profile) = config.providers.fallback_provider()
        && (config
            .providers
            .fallback
            .as_deref()
            .is_some_and(is_deepseek_provider_name)
            || profile
                .name
                .as_deref()
                .is_some_and(is_deepseek_provider_name)
            || profile
                .base_url
                .as_deref()
                .is_some_and(is_deepseek_base_url))
        && let Some(key) = profile
            .api_key
            .as_deref()
            .filter(|key| !key.trim().is_empty())
    {
        return Some(key.trim().to_owned());
    }

    // The Android settings UI stores the active LLM credential in a named
    // provider table such as `[providers.models.deepseek]`. It may not also
    // populate `providers.default_api_key`, so inspect all named providers.
    if let Some(key) = config.providers.models.iter().find_map(|(name, profile)| {
        is_deepseek_model_provider(name, profile)
            .then_some(profile.api_key.as_deref())
            .flatten()
            .filter(|key| !key.trim().is_empty())
            .map(|key| key.trim().to_owned())
    }) {
        return Some(key);
    }

    if config
        .providers
        .default
        .as_deref()
        .is_some_and(is_deepseek_provider_name)
        && let Some(key) = config
            .providers
            .default_api_key
            .as_deref()
            .filter(|key| !key.trim().is_empty())
    {
        return Some(key.trim().to_owned());
    }

    std::env::var("DEEPSEEK_API_KEY")
        .ok()
        .filter(|key| !key.trim().is_empty())
        .map(|key| key.trim().to_owned())
}

use crate::ask_user::{AskUserHandler, AskUserTool, CliAskUserHandler};
use crate::background::{
    BackgroundCancelTool, BackgroundEventSink, BackgroundJobManager, BackgroundRunTool,
    BackgroundStatusTool,
};
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
use crate::javascript::JavaScriptTool;
use crate::knowledge_tool::KnowledgeTool;
use crate::llm_task::LlmTaskTool;
use crate::memory_export::MemoryExportTool;
use crate::memory_forget::MemoryForgetTool;
use crate::memory_purge::MemoryPurgeTool;
use crate::memory_recall::MemoryRecallTool;
use crate::memory_store::MemoryStoreTool;
use crate::memory_update::MemoryUpdateTool;
use crate::model_routing_config::ModelRoutingConfigTool;
use crate::pdf_read::PdfReadTool;
use crate::shell::ShellTool;
use crate::skill_create::SkillCreateTool;
use crate::skill_tool::SkillTool;
use crate::web_fetch::WebFetchTool;
use crate::web_search_tool::WebSearchTool;

/// Return all built-in tools as boxed trait objects.
///
/// The binary calls this once during startup to populate the Agent's tool registry.
/// Tools that need runtime dependencies receive them via constructor injection.
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
    all_tools_with_runtime(workspace_dir, config, memory, None, None)
}

/// Return all built-in tools and optionally publish terminal background-job events.
pub fn all_tools_with_background_events(
    #[cfg_attr(feature = "android", allow(unused))] workspace_dir: PathBuf,
    config: &Config,
    memory: Arc<dyn clawseed_api::memory_traits::Memory>,
    background_event_sink: Option<BackgroundEventSink>,
) -> Vec<Box<dyn Tool>> {
    all_tools_with_runtime(workspace_dir, config, memory, background_event_sink, None)
}

/// Return all built-ins with transport-specific runtime handlers.
pub fn all_tools_with_runtime(
    #[cfg_attr(feature = "android", allow(unused))] workspace_dir: PathBuf,
    config: &Config,
    memory: Arc<dyn clawseed_api::memory_traits::Memory>,
    background_event_sink: Option<BackgroundEventSink>,
    ask_user_handler: Option<Arc<dyn AskUserHandler>>,
) -> Vec<Box<dyn Tool>> {
    let background_manager = BackgroundJobManager::new(background_event_sink);
    let mut tools: Vec<Box<dyn Tool>> = vec![
        Box::new(AskUserTool::new(
            ask_user_handler.unwrap_or_else(|| Arc::new(CliAskUserHandler)),
        )),
        Box::new(BackgroundCancelTool::new(background_manager.clone())),
        Box::new(BackgroundRunTool::new(background_manager.clone())),
        Box::new(BackgroundStatusTool::new(background_manager)),
        Box::new(CalculatorTool::new()),
        Box::new(ContentSearchTool::new()),
        Box::new(FileEditTool::new()),
        Box::new(FileReadTool::new()),
        Box::new(FileWriteTool::new()),
        Box::new(GlobSearchTool::new()),
        Box::new(JavaScriptTool::new()),
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
    tools.push(Box::new(MemoryStoreTool::new(memory.clone())));
    tools.push(Box::new(MemoryUpdateTool::new(memory)));
    tools.push(Box::new(ModelRoutingConfigTool::new()));
    tools.push(Box::new(PdfReadTool::new()));
    tools.push(Box::new(ShellTool::new()));

    // Skill tool: only include when skill system is enabled
    if config.skills.enabled {
        tools.push(Box::new(SkillTool::new()));
        tools.push(Box::new(SkillCreateTool::new()));
    }

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
        let search_tool = WebSearchTool::new_with_config(
            provider,
            config.web_search.brave_api_key.clone(),
            config.web_search.searxng_instance_url.clone(),
            config.web_search.tavily_api_key.clone(),
            5,  // max_results
            15, // timeout_secs
        )
        .with_deepseek_api_key(resolve_deepseek_search_key(config));
        tools.push(Box::new(search_tool));
    }

    tools
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deepseek_search_key_prefers_explicit_override() {
        let mut config = Config::default();
        config.web_search.deepseek_api_key = Some("search-key".into());
        config.providers.default = Some("deepseek".into());
        config.providers.default_api_key = Some("llm-key".into());

        assert_eq!(
            resolve_deepseek_search_key(&config).as_deref(),
            Some("search-key")
        );
    }

    #[test]
    fn deepseek_search_key_reuses_default_provider_key() {
        let mut config = Config::default();
        config.providers.default = Some("deepseek".into());
        config.providers.default_api_key = Some("llm-key".into());

        assert_eq!(
            resolve_deepseek_search_key(&config).as_deref(),
            Some("llm-key")
        );
    }

    #[test]
    fn deepseek_search_key_reuses_named_provider_key() {
        let mut config = Config::default();
        config.providers.models.insert(
            "deepseek".into(),
            clawseed_config::schema::ModelProviderConfig {
                api_key: Some("llm-key".into()),
                base_url: Some("https://api.deepseek.com/v1".into()),
                ..Default::default()
            },
        );

        assert_eq!(
            resolve_deepseek_search_key(&config).as_deref(),
            Some("llm-key")
        );
    }
}
