//! Deterministic first-stage routing for post-turn knowledge candidates.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KnowledgeTarget {
    Profile,
    Memory,
    Drop,
}

pub struct KnowledgeRouter;

impl KnowledgeRouter {
    pub fn route_user_text(text: &str) -> KnowledgeTarget {
        let normalized = text.trim().to_lowercase();
        if normalized.is_empty() {
            return KnowledgeTarget::Drop;
        }

        const PROFILE_SIGNALS: &[&str] = &[
            "my name is",
            "i prefer",
            "i like",
            "i dislike",
            "my goal",
            "my timezone",
            "call me ",
            "answer concisely",
            "keep responses concise",
            "answer in detail",
            "我的名字",
            "叫我",
            "我喜欢",
            "我不喜欢",
            "我的目标",
            "我的时区",
            "以后回答",
            "回答简洁",
            "详细解释",
            "需要字幕",
            "屏幕阅读器",
        ];
        if PROFILE_SIGNALS
            .iter()
            .any(|signal| normalized.contains(signal))
        {
            return KnowledgeTarget::Profile;
        }

        const MEMORY_SIGNALS: &[&str] = &[
            "remember that",
            "we decided",
            "decision",
            "project uses",
            "project stack",
            "architecture",
            "completed task",
            "task result",
            "记住这个项目",
            "项目使用",
            "项目技术栈",
            "架构",
            "我们决定",
            "决策",
            "任务完成",
            "执行结果",
        ];
        if MEMORY_SIGNALS
            .iter()
            .any(|signal| normalized.contains(signal))
        {
            KnowledgeTarget::Memory
        } else {
            KnowledgeTarget::Drop
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn routes_chinese_profile_changes_away_from_memory() {
        assert_eq!(
            KnowledgeRouter::route_user_text("以后回答简洁一点。"),
            KnowledgeTarget::Profile
        );
        assert_eq!(
            KnowledgeRouter::route_user_text("我现在不喜欢简洁回答了，请详细解释。"),
            KnowledgeTarget::Profile
        );
    }

    #[test]
    fn routes_english_response_preferences_to_profile() {
        assert_eq!(
            KnowledgeRouter::route_user_text("Please keep responses concise."),
            KnowledgeTarget::Profile
        );
    }

    #[test]
    fn routes_project_decisions_to_memory() {
        assert_eq!(
            KnowledgeRouter::route_user_text("我们决定这个项目使用 Rust。"),
            KnowledgeTarget::Memory
        );
        assert_eq!(
            KnowledgeRouter::route_user_text("The project architecture uses an event log."),
            KnowledgeTarget::Memory
        );
    }

    #[test]
    fn drops_transient_requests() {
        assert_eq!(
            KnowledgeRouter::route_user_text("What time is it?"),
            KnowledgeTarget::Drop
        );
    }
}
