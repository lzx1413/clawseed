//! Built-in tool implementations for ClawSeed.
//!
//! All tools depend only on `clawseed-api` traits (Tool, ToolContext).
//! Runtime capabilities (LLM, memory, security) are accessed via
//! `ctx.get::<T>()` from ToolContext.

#[cfg(not(feature = "android"))]
pub mod backup_tool;
pub mod calculator;
pub mod canvas;
pub mod content_search;
#[cfg(not(feature = "android"))]
pub mod cron_add;
#[cfg(not(feature = "android"))]
pub mod cron_list;
#[cfg(not(feature = "android"))]
pub mod cron_remove;
#[cfg(not(feature = "android"))]
pub mod cron_run;
#[cfg(not(feature = "android"))]
pub mod cron_runs;
#[cfg(not(feature = "android"))]
pub mod cron_update;
pub mod file_edit;
pub mod file_read;
pub mod file_write;
#[cfg(not(feature = "android"))]
pub mod git_operations;
pub mod glob_search;
pub mod http_request;
pub mod knowledge_tool;
pub mod llm_task;
pub mod memory_export;
pub mod memory_forget;
pub mod memory_purge;
pub mod memory_recall;
pub mod memory_store;
pub mod model_routing_config;
pub mod pdf_read;
pub mod registry;
pub mod shell;
pub mod skill_tool;
pub mod util_helpers;
pub mod web_fetch;
pub mod web_search_provider_routing;
pub mod web_search_tool;
