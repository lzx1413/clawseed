//! AgentToolContext — the execution context for tool runs.

use std::path::{Path, PathBuf};

/// The real tool execution context — holds workspace dir.
pub struct AgentToolContext {
    workspace_dir: PathBuf,
}

impl AgentToolContext {
    pub fn new(workspace_dir: PathBuf) -> Self {
        Self { workspace_dir }
    }
}

impl clawseed_api::tool_context::ToolContext for AgentToolContext {
    fn workspace_dir(&self) -> &Path {
        &self.workspace_dir
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clawseed_api::tool_context::ToolContext;

    #[test]
    fn workspace_dir_returns_correct_path() {
        let ctx = AgentToolContext::new(PathBuf::from("/workspace"));
        assert_eq!(ctx.workspace_dir(), Path::new("/workspace"));
    }
}
