//! Agent loop, tool dispatch, cost control, and cron engine for ClawSeed.
//!
//! The Agent is a registry — it holds `Vec<Box<dyn Tool>>`, `Vec<Box<dyn Hook>>`,
//! and `Vec<Box<dyn ContextProvider>>`. Adding features only requires adding
//! entries to these vectors; the core code never changes.

pub mod agent;
pub mod agent_loop;
pub mod context;
pub mod dispatcher;
pub mod tool_execution;
pub mod history;
pub mod health;
pub mod prompt;
pub mod cost;
pub mod hooks;
pub mod observer;
pub mod observability;
pub mod approval;
pub mod security;
pub mod cron;
pub mod tools;
