//! Core traits and shared types for ClawSeed.
//!
//! This crate defines all fundamental abstractions that other ClawSeed crates
//! depend on. It contains zero implementation — only trait definitions and
//! shared types.
//!
//! ## Key design: Extension imports core, core never imports extension
//!
//! The Agent holds `Vec<Box<dyn Tool>>`, `Vec<Box<dyn Hook>>`, and
//! `Vec<Box<dyn ContextProvider>>`. Adding features only requires adding
//! entries to these vectors; the core code never changes.
//!
//! ## Traits
//! - [`provider::Provider`] — LLM inference backends
//! - [`tool::Tool`] — agent-callable capabilities
//! - [`tool_context::ToolContext`] — capability bag for tool execution
//! - [`hook::Hook`] — before/after tool call interception
//! - [`context_provider::ContextProvider`] — extension mechanism for ToolContext
//! - [`memory_traits::Memory`] — conversation memory backends
//! - [`observer::Observer`] — metrics and tracing

pub mod channel;
pub mod context_provider;
pub mod hook;
pub mod memory_traits;
pub mod observer;
pub mod provider;
pub mod schema;
pub mod tool;
pub mod tool_context;
pub mod tool_registry;

tokio::task_local! {
    /// Current thread/sender ID for per-sender rate limiting.
    pub static TOOL_LOOP_THREAD_ID: Option<String>;

    /// Override for tool choice mode, set by the agent loop.
    pub static TOOL_CHOICE_OVERRIDE: Option<String>;

    /// Session key for the currently active session.
    pub static TOOL_LOOP_SESSION_KEY: Option<String>;
}
