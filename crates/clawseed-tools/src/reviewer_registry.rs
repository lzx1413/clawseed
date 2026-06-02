//! Reviewer tool registry — filters shared builtin tools and adds reviewer-specific tools.
//!
//! The reviewer's tool set is NOT a separate full registration system. It filters
//! `shared_builtin_tools` by name for read-only tools (file_read, memory_recall),
//! reusing the same Arc references, then appends one freshly constructed
//! `ReviewerMemoryStoreTool` with the namespaced council memory.

use crate::reviewer_memory_store::ReviewerMemoryStoreTool;
use clawseed_api::tool::Tool;
use clawseed_memory::namespaced::NamespacedMemory;
use std::sync::Arc;

/// Allowed tool names for reviewers (hardcoded whitelist).
const REVIEWER_READ_TOOLS: &[&str] = &["file_read", "memory_recall"];

/// Construct the reviewer tool set from shared builtin tools + reviewer-local tool.
///
/// - Filters `shared_builtin_tools` by name for `file_read` and `memory_recall`,
///   reusing their Arc references directly (zero construction, zero duplication).
/// - Constructs one `ReviewerMemoryStoreTool` with the namespaced council memory.
/// - Returns the combined set as `Vec<Arc<dyn Tool>>`.
pub fn reviewer_tools(
    role: &str,
    council_memory: Arc<NamespacedMemory>,
    shared_builtin_tools: Arc<[Arc<dyn Tool>]>,
) -> Vec<Arc<dyn Tool>> {
    let mut tools: Vec<Arc<dyn Tool>> = Vec::new();

    // Filter shared builtin tools by name — reuse Arc references
    for tool in shared_builtin_tools.iter() {
        if REVIEWER_READ_TOOLS.contains(&tool.name()) {
            tools.push(tool.clone());
        }
    }

    // Construct reviewer-specific tool
    tools.push(Arc::new(ReviewerMemoryStoreTool::new(
        role.to_string(),
        council_memory,
    )));

    tools
}
