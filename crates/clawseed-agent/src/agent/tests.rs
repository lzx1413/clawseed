use super::Agent;
use anyhow::Result;
use async_trait::async_trait;
use clawseed_api::memory_traits::Memory;
use clawseed_api::provider::{ChatRequest, ChatResponse, ConversationMessage, Provider};
use clawseed_api::tool::{Tool, ToolResult};
use clawseed_api::user_profile::{UserContext, UserProfileStore};
use parking_lot::Mutex;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use crate::dispatcher::{NativeToolDispatcher, XmlToolDispatcher};
use crate::observer::Observer;

struct MockProvider {
    responses: Mutex<Vec<ChatResponse>>,
}

struct ProfileInferenceProvider;

struct CountingInferenceProvider {
    responses: Mutex<Vec<anyhow::Result<ChatResponse>>>,
    inference_calls: Arc<AtomicUsize>,
}

#[async_trait]
impl Provider for MockProvider {
    async fn chat_with_system(
        &self,
        _system_prompt: Option<&str>,
        _message: &str,
        _model: &str,
        _temperature: Option<f64>,
    ) -> Result<String> {
        Ok("ok".into())
    }

    async fn chat(
        &self,
        _request: ChatRequest<'_>,
        _model: &str,
        _temperature: Option<f64>,
    ) -> Result<ChatResponse> {
        let mut guard = self.responses.lock();
        if guard.is_empty() {
            return Ok(ChatResponse {
                text: Some("done".into()),
                tool_calls: vec![],
                usage: None,
                reasoning_content: None,
                stop_reason: clawseed_api::provider::StopReason::EndTurn,
            });
        }
        Ok(guard.remove(0))
    }
}

#[async_trait]
impl Provider for ProfileInferenceProvider {
    async fn chat_with_system(
        &self,
        _system_prompt: Option<&str>,
        _message: &str,
        _model: &str,
        _temperature: Option<f64>,
    ) -> Result<String> {
        Ok(r#"{"items":[{"key":"preference.response_style","value":"concise","category":"preference","confidence":0.95,"expires_in_days":null}]}"#.into())
    }

    async fn chat(
        &self,
        _request: ChatRequest<'_>,
        _model: &str,
        _temperature: Option<f64>,
    ) -> Result<ChatResponse> {
        Ok(ChatResponse {
            text: Some("Understood.".into()),
            tool_calls: vec![],
            usage: None,
            reasoning_content: None,
            stop_reason: clawseed_api::provider::StopReason::EndTurn,
        })
    }
}

#[async_trait]
impl Provider for CountingInferenceProvider {
    async fn chat_with_system(
        &self,
        _system_prompt: Option<&str>,
        _message: &str,
        _model: &str,
        _temperature: Option<f64>,
    ) -> Result<String> {
        self.inference_calls.fetch_add(1, Ordering::SeqCst);
        Ok(r#"{"items":[]}"#.into())
    }

    async fn chat(
        &self,
        _request: ChatRequest<'_>,
        _model: &str,
        _temperature: Option<f64>,
    ) -> Result<ChatResponse> {
        let mut responses = self.responses.lock();
        if responses.is_empty() {
            anyhow::bail!("unexpected provider call");
        }
        responses.remove(0)
    }
}

struct MockTool;

#[async_trait]
impl Tool for MockTool {
    fn name(&self) -> &str {
        "echo"
    }
    fn description(&self) -> &str {
        "echo"
    }
    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({"type": "object"})
    }

    async fn execute(
        &self,
        _args: serde_json::Value,
        _ctx: &dyn clawseed_api::tool_context::ToolContext,
    ) -> Result<ToolResult> {
        Ok(ToolResult {
            success: true,
            output: "tool-out".into(),
            error: None,
            presentation: None,
        })
    }
}

fn make_memory() -> Arc<dyn Memory> {
    Arc::new(clawseed_memory::none::NoneMemory::new())
}

#[tokio::test]
async fn turn_without_tools_returns_text() {
    let provider = Box::new(MockProvider {
        responses: Mutex::new(vec![ChatResponse {
            text: Some("hello".into()),
            tool_calls: vec![],
            usage: None,
            reasoning_content: None,
            stop_reason: clawseed_api::provider::StopReason::EndTurn,
        }]),
    });

    let observer: Arc<dyn Observer> = Arc::new(crate::observer::NoopObserver);
    let mut agent = Agent::builder()
        .provider(provider)
        .tools(vec![Box::new(MockTool)])
        .memory(make_memory())
        .observer(observer)
        .tool_dispatcher(Box::new(XmlToolDispatcher))
        .workspace_dir(std::path::PathBuf::from("/tmp"))
        .build()
        .expect("agent builder should succeed");

    let response = agent.turn("hi").await.unwrap();
    assert_eq!(response, "hello");
}

#[tokio::test]
async fn completed_turn_schedules_profile_inference() {
    let dir = tempfile::tempdir().unwrap();
    let store =
        Arc::new(clawseed_memory::user_profile::SqliteUserProfileStore::new(dir.path()).unwrap());
    let observer: Arc<dyn Observer> = Arc::new(crate::observer::NoopObserver);
    let user_model_config = clawseed_config::schema::UserModelConfig {
        auto_infer: true,
        ..Default::default()
    };
    let mut agent = Agent::builder()
        .provider(Box::new(ProfileInferenceProvider))
        .tools(vec![Box::new(MockTool)])
        .memory(make_memory())
        .observer(observer)
        .tool_dispatcher(Box::new(NativeToolDispatcher))
        .workspace_dir(dir.path().to_path_buf())
        .user_profile_store(store.clone())
        .user_context(UserContext {
            user_id: "owner".into(),
            session_id: Some("session-1".into()),
            persona_id: None,
        })
        .user_model_config(user_model_config)
        .build()
        .expect("agent builder should succeed");

    assert_eq!(
        agent.turn("Please keep responses concise.").await.unwrap(),
        "Understood."
    );

    let mut profile = store.load("owner").await.unwrap();
    for _ in 0..50 {
        if !profile.items.is_empty() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        profile = store.load("owner").await.unwrap();
    }
    assert_eq!(profile.items.len(), 1);
    assert_eq!(profile.items[0].key, "preference.response_style");
}

fn response(text: &str, stop_reason: clawseed_api::provider::StopReason) -> ChatResponse {
    ChatResponse {
        text: Some(text.into()),
        tool_calls: vec![],
        usage: None,
        reasoning_content: None,
        stop_reason,
    }
}

fn learning_test_agent(
    responses: Vec<anyhow::Result<ChatResponse>>,
) -> (Agent, Arc<AtomicUsize>, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let store =
        Arc::new(clawseed_memory::user_profile::SqliteUserProfileStore::new(dir.path()).unwrap());
    let observer: Arc<dyn Observer> = Arc::new(crate::observer::NoopObserver);
    let agent = Agent::builder()
        .provider(Box::new(CountingInferenceProvider {
            responses: Mutex::new(responses),
            inference_calls: calls.clone(),
        }))
        .tools(vec![])
        .memory(make_memory())
        .observer(observer)
        .tool_dispatcher(Box::new(NativeToolDispatcher))
        .workspace_dir(dir.path().to_path_buf())
        .user_profile_store(store)
        .user_context(UserContext {
            user_id: "owner".into(),
            session_id: Some("session-1".into()),
            persona_id: None,
        })
        .user_model_config(clawseed_config::schema::UserModelConfig {
            auto_infer: true,
            ..Default::default()
        })
        .build()
        .unwrap();
    (agent, calls, dir)
}

#[tokio::test]
async fn standard_and_streamed_turn_each_schedule_learning_once() {
    for streamed in [false, true] {
        let (mut agent, calls, _dir) =
            learning_test_agent(vec![Ok(response("done", Default::default()))]);
        if streamed {
            let (event_tx, _event_rx) = tokio::sync::mpsc::channel(8);
            agent
                .turn_streamed("Please keep responses concise.", event_tx, None, false)
                .await
                .unwrap();
        } else {
            agent.turn("Please keep responses concise.").await.unwrap();
        }
        agent.shutdown_learning().await;
        assert_eq!(calls.load(Ordering::SeqCst), 1, "streamed={streamed}");
    }
}

#[tokio::test]
async fn failed_and_cancelled_turns_do_not_schedule_learning() {
    let (mut failed_agent, failed_calls, _dir) =
        learning_test_agent(vec![Err(anyhow::anyhow!("provider failed"))]);
    assert!(
        failed_agent
            .turn("Please keep responses concise.")
            .await
            .is_err()
    );
    failed_agent.shutdown_learning().await;
    assert_eq!(failed_calls.load(Ordering::SeqCst), 0);

    let (mut cancelled_agent, cancelled_calls, _dir) =
        learning_test_agent(vec![Ok(response("unused", Default::default()))]);
    let cancellation = tokio_util::sync::CancellationToken::new();
    cancellation.cancel();
    let (event_tx, _event_rx) = tokio::sync::mpsc::channel(8);
    assert!(
        cancelled_agent
            .turn_streamed(
                "Please keep responses concise.",
                event_tx,
                Some(cancellation),
                false,
            )
            .await
            .is_err()
    );
    cancelled_agent.shutdown_learning().await;
    assert_eq!(cancelled_calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn truncated_turn_schedules_learning_only_after_final_success() {
    use clawseed_api::provider::StopReason;

    let (mut agent, calls, _dir) = learning_test_agent(vec![
        Ok(response("partial", StopReason::MaxTokens)),
        Ok(response("finished", StopReason::EndTurn)),
    ]);
    agent.config.auto_continue_on_truncation = true;
    agent.config.max_auto_continue = 1;
    assert_eq!(
        agent.turn("Please keep responses concise.").await.unwrap(),
        "finished"
    );
    agent.shutdown_learning().await;
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn turn_with_native_dispatcher_handles_tool_results_variant() {
    let provider = Box::new(MockProvider {
        responses: Mutex::new(vec![
            ChatResponse {
                text: Some(String::new()),
                tool_calls: vec![clawseed_api::provider::ToolCall {
                    id: "tc1".into(),
                    name: "echo".into(),
                    arguments: "{}".into(),
                }],
                usage: None,
                reasoning_content: None,
                stop_reason: clawseed_api::provider::StopReason::EndTurn,
            },
            ChatResponse {
                text: Some("done".into()),
                tool_calls: vec![],
                usage: None,
                reasoning_content: None,
                stop_reason: clawseed_api::provider::StopReason::EndTurn,
            },
        ]),
    });

    let observer: Arc<dyn Observer> = Arc::new(crate::observer::NoopObserver);
    let mut agent = Agent::builder()
        .provider(provider)
        .tools(vec![Box::new(MockTool)])
        .memory(make_memory())
        .observer(observer)
        .tool_dispatcher(Box::new(NativeToolDispatcher))
        .workspace_dir(std::path::PathBuf::from("/tmp"))
        .build()
        .expect("agent builder should succeed");

    let response = agent.turn("hi").await.unwrap();
    assert_eq!(response, "done");
    assert!(
        agent
            .history()
            .iter()
            .any(|msg| matches!(msg, ConversationMessage::ToolResults(_)))
    );
}

#[test]
fn builder_allowed_tools_none_keeps_all_tools() {
    let provider = Box::new(MockProvider {
        responses: Mutex::new(vec![]),
    });

    let observer: Arc<dyn Observer> = Arc::new(crate::observer::NoopObserver);
    let agent = Agent::builder()
        .provider(provider)
        .tools(vec![Box::new(MockTool)])
        .memory(make_memory())
        .observer(observer)
        .tool_dispatcher(Box::new(NativeToolDispatcher))
        .workspace_dir(std::path::PathBuf::from("/tmp"))
        .allowed_tools(None)
        .build()
        .expect("agent builder should succeed");

    assert_eq!(agent.tool_registry.tool_specs().len(), 1);
    assert_eq!(agent.tool_registry.tool_specs()[0].name, "echo");
}

#[test]
fn builder_allowed_tools_some_filters_tools() {
    let provider = Box::new(MockProvider {
        responses: Mutex::new(vec![]),
    });

    let observer: Arc<dyn Observer> = Arc::new(crate::observer::NoopObserver);
    let agent = Agent::builder()
        .provider(provider)
        .tools(vec![Box::new(MockTool)])
        .memory(make_memory())
        .observer(observer)
        .tool_dispatcher(Box::new(NativeToolDispatcher))
        .workspace_dir(std::path::PathBuf::from("/tmp"))
        .allowed_tools(Some(vec!["nonexistent".to_string()]))
        .build()
        .expect("agent builder should succeed");

    assert!(agent.tool_registry.tool_specs().is_empty());
}

#[test]
fn builder_registers_profile_crud_tools_when_profile_store_is_available() {
    let dir = tempfile::tempdir().unwrap();
    let store =
        Arc::new(clawseed_memory::user_profile::SqliteUserProfileStore::new(dir.path()).unwrap());
    let observer: Arc<dyn Observer> = Arc::new(crate::observer::NoopObserver);
    let agent = Agent::builder()
        .provider(Box::new(MockProvider {
            responses: Mutex::new(vec![]),
        }))
        .tools(vec![])
        .memory(make_memory())
        .observer(observer)
        .tool_dispatcher(Box::new(NativeToolDispatcher))
        .workspace_dir(dir.path().to_path_buf())
        .user_profile_store(store)
        .user_context(UserContext {
            user_id: "owner".into(),
            session_id: None,
            persona_id: None,
        })
        .build()
        .expect("agent builder should succeed");

    let names = agent
        .tool_registry
        .tool_specs()
        .into_iter()
        .map(|spec| spec.name)
        .collect::<Vec<_>>();
    assert!(names.contains(&"user_profile_search".to_string()));
    assert!(names.contains(&"user_profile_change_plan".to_string()));
    assert!(names.contains(&"user_profile_apply_plan".to_string()));
    assert!(names.contains(&"user_profile_delete".to_string()));
    assert!(!names.contains(&"user_profile_undo".to_string()));
}

#[test]
fn add_remote_tools_no_duplicates_on_repeated_calls() {
    struct NamedMockTool {
        name: String,
    }
    #[async_trait]
    impl Tool for NamedMockTool {
        fn name(&self) -> &str {
            &self.name
        }
        fn description(&self) -> &str {
            "mock"
        }
        fn parameters_schema(&self) -> serde_json::Value {
            serde_json::json!({"type": "object"})
        }
        async fn execute(
            &self,
            _args: serde_json::Value,
            _ctx: &dyn clawseed_api::tool_context::ToolContext,
        ) -> Result<ToolResult> {
            Ok(ToolResult {
                success: true,
                output: "ok".into(),
                error: None,
                presentation: None,
            })
        }
    }

    let provider = Box::new(MockProvider {
        responses: Mutex::new(vec![]),
    });
    let observer: Arc<dyn Observer> = Arc::new(crate::observer::NoopObserver);
    let mut agent = Agent::builder()
        .provider(provider)
        .tools(vec![])
        .memory(make_memory())
        .observer(observer)
        .tool_dispatcher(Box::new(XmlToolDispatcher))
        .workspace_dir(std::path::PathBuf::from("/tmp"))
        .build()
        .expect("agent builder should succeed");

    let make_named = |n: &str| -> Box<dyn Tool> {
        Box::new(NamedMockTool {
            name: n.to_string(),
        })
    };

    agent.add_remote_tools(
        vec![make_named("tool_a"), make_named("tool_b")],
        "s1".to_string(),
    );
    assert_eq!(agent.tool_registry.len(), 2);
    agent.add_remote_tools(
        vec![make_named("tool_a"), make_named("tool_b")],
        "s1".to_string(),
    );
    assert_eq!(agent.tool_registry.len(), 2);
}

#[tokio::test]
async fn skill_activation_updates_history_not_sentinel() {
    // Create a skill directory with manifest.toml + SKILL.md
    let dir = tempfile::tempdir().unwrap();
    let skill_dir = dir
        .path()
        .join(".clawseed")
        .join("skills")
        .join("test-skill");
    std::fs::create_dir_all(&skill_dir).unwrap();
    std::fs::write(
        skill_dir.join("manifest.toml"),
        r#"[skill]
name = "test-skill"
description = "A test skill"
"#,
    )
    .unwrap();
    std::fs::write(skill_dir.join("SKILL.md"), "# Test Skill\nDo the thing.\n").unwrap();

    // Load the skill index
    let skill_index = crate::skills::load_skill_index(dir.path());

    // Provider returns a tool call for Skill, then a final response
    let provider = Box::new(MockProvider {
        responses: Mutex::new(vec![
            ChatResponse {
                text: Some(String::new()),
                tool_calls: vec![clawseed_api::provider::ToolCall {
                    id: "tc-skill-1".into(),
                    name: "Skill".into(),
                    arguments: r#"{"skill": "test-skill"}"#.into(),
                }],
                usage: None,
                reasoning_content: None,
                stop_reason: clawseed_api::provider::StopReason::EndTurn,
            },
            ChatResponse {
                text: Some("done".into()),
                tool_calls: vec![],
                usage: None,
                reasoning_content: None,
                stop_reason: clawseed_api::provider::StopReason::EndTurn,
            },
        ]),
    });

    let observer: Arc<dyn Observer> = Arc::new(crate::observer::NoopObserver);
    let mut agent = Agent::builder()
        .provider(provider)
        .tools(vec![Box::new(clawseed_tools::skill_tool::SkillTool::new())])
        .memory(make_memory())
        .observer(observer)
        .tool_dispatcher(Box::new(NativeToolDispatcher))
        .workspace_dir(dir.path().to_path_buf())
        .skill_index(skill_index)
        .build()
        .expect("agent builder should succeed");

    let _response = agent.turn("use test-skill").await.unwrap();

    // Verify no sentinel JSON in history
    for msg in agent.history() {
        if let ConversationMessage::ToolResults(results) = msg {
            for result in results {
                assert!(
                    !result.content.contains("__skill_action"),
                    "Sentinel JSON leaked into history: {}",
                    result.content
                );
                assert!(
                    !result.content.contains("__skill_name"),
                    "Sentinel JSON leaked into history: {}",
                    result.content
                );
            }
        }
        if let ConversationMessage::Chat(chat) = msg {
            assert!(
                !chat.content.contains("__skill_action"),
                "Sentinel JSON leaked into history chat: {}",
                chat.content
            );
        }
    }

    // Verify the skill was activated and history contains the activation message
    assert_eq!(agent.active_skills.len(), 1);
    assert_eq!(agent.active_skills[0].skill.name, "test-skill");
    let has_activation_msg = agent.history().iter().any(|msg| {
        if let ConversationMessage::ToolResults(results) = msg {
            results.iter().any(|r| r.content.contains("activated"))
        } else {
            false
        }
    });
    assert!(
        has_activation_msg,
        "History should contain skill activation message"
    );
}
