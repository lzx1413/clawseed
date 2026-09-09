//! AgentToolContext — the execution context for tool runs.

use std::path::{Path, PathBuf};

/// The real tool execution context — holds workspace dir.
pub struct AgentToolContext {
    workspace_dir: PathBuf,
    user_context: Option<clawseed_api::user_profile::UserContext>,
}

impl AgentToolContext {
    pub fn new(
        workspace_dir: PathBuf,
        user_context: Option<clawseed_api::user_profile::UserContext>,
    ) -> Self {
        Self {
            workspace_dir,
            user_context,
        }
    }
}

impl clawseed_api::tool_context::ToolContext for AgentToolContext {
    fn workspace_dir(&self) -> &Path {
        &self.workspace_dir
    }

    fn user_context(&self) -> Option<&clawseed_api::user_profile::UserContext> {
        self.user_context.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clawseed_api::tool_context::ToolContext;

    #[test]
    fn workspace_dir_returns_correct_path() {
        let ctx = AgentToolContext::new(PathBuf::from("/workspace"), None);
        assert_eq!(ctx.workspace_dir(), Path::new("/workspace"));
    }
}
