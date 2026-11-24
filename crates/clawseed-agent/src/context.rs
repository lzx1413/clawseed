//! AgentToolContext — the execution context for tool runs.

use std::path::{Path, PathBuf};

/// The real tool execution context — holds workspace dir.
pub struct AgentToolContext {
    workspace_dir: PathBuf,
    image_attachments: Vec<clawseed_api::provider::ImageAttachment>,
    user_context: Option<clawseed_api::user_profile::UserContext>,
    turn_id: Option<String>,
    tool_call_id: Option<String>,
}

impl AgentToolContext {
    pub fn new(
        workspace_dir: PathBuf,
        user_context: Option<clawseed_api::user_profile::UserContext>,
        turn_id: Option<String>,
        tool_call_id: Option<String>,
        image_attachments: Vec<clawseed_api::provider::ImageAttachment>,
    ) -> Self {
        Self {
            workspace_dir,
            image_attachments,
            user_context,
            turn_id,
            tool_call_id,
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

    fn turn_id(&self) -> Option<&str> {
        self.turn_id.as_deref()
    }

    fn tool_call_id(&self) -> Option<&str> {
        self.tool_call_id.as_deref()
    }

    fn image_attachments(&self) -> &[clawseed_api::provider::ImageAttachment] {
        &self.image_attachments
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clawseed_api::tool_context::ToolContext;

    #[test]
    fn workspace_dir_returns_correct_path() {
        let ctx = AgentToolContext::new(PathBuf::from("/workspace"), None, None, None, Vec::new());
        assert_eq!(ctx.workspace_dir(), Path::new("/workspace"));
    }
}
