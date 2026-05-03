//! Agent loop, tool dispatch, cost control, and cron engine for ClawSeed.
//!
//! The Agent is a registry — it holds `Vec<Box<dyn Tool>>`, `Vec<Box<dyn Hook>>`,
//! and `Vec<Box<dyn ContextProvider>>`. Adding features only requires adding
//! entries to these vectors; the core code never changes.

pub mod agent;
pub mod agent_loop;
pub mod approval;
pub mod context;
pub mod cost;
pub mod cron;
pub mod dispatcher;
pub mod health;
pub mod history;
pub mod hooks;
pub mod identity;
pub mod observability;
pub mod observer;
pub mod parser;
pub mod personality;
pub mod prompt;
pub mod security;
pub mod tool_registry;
pub mod tools;
