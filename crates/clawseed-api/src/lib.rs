//! Core traits and shared types for ClawSeed.
//!
//! This crate defines all fundamental abstractions that other ClawSeed crates
//! depend on. It contains zero implementation — only trait definitions and
//! shared types.
//!
//! ## Key design: Extension imports core, core never imports extension
//!
//! The Agent holds `Vec<Box<dyn Tool>>`, `Vec<Box<dyn Hook>>`.
//! Adding features only requires adding entries to these vectors;
//! the core code never changes.
//!
//! ## Traits
//! - [`provider::Provider`] — LLM inference backends
//! - [`tool::Tool`] — agent-callable capabilities
//! - [`hook::Hook`] — before/after tool call interception
//! - [`memory_traits::Memory`] — conversation memory backends

pub mod channel;
pub mod hook;
pub mod memory_traits;
pub mod provider;
pub mod schema;
pub mod tool;
pub mod tool_context;
pub mod tool_registry;
pub mod user_profile;

tokio::task_local! {
    /// Override for tool choice mode, set by the agent loop.
    pub static TOOL_CHOICE_OVERRIDE: Option<String>;
}
