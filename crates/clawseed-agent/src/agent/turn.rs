//! Standard and streaming agent turn orchestration.

use super::{Agent, TurnEvent};
use crate::dispatcher::{ParsedToolCall, ToolExecutionResult};
use anyhow::Result;
use clawseed_api::memory_traits::{MemoryCategory, MemoryQuery, MemoryScope};
use clawseed_api::provider::{ChatMessage, ChatRequest, ChatResponse, ConversationMessage};

impl Agent {
    /// Prepare for a turn: add system prompt if needed, auto-save, enrich with timestamp.
    fn prepare_turn(&mut self, user_message: &str) -> Result<()> {
        if self.history.is_empty() {
            let partitioned = self.build_system_prompt_partitioned()?;
            self.stable_system_content = partitioned.stable.clone();
            self.history
                .push(ConversationMessage::Chat(ChatMessage::system_partitioned(
                    partitioned.stable,
                    partitioned.full,
                )));
        }

        let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S %Z");
        let enriched = format!("[{now}] {user_message}");

        self.history
            .push(ConversationMessage::Chat(ChatMessage::user(enriched)));

        Ok(())
    }

    async fn inject_recalled_memory(&mut self, user_message: &str) {
        if !self.auto_recall || self.memory.name() == "none" || self.auto_recall_limit == 0 {
            return;
        }
        let namespaces = self.memory.accessible_namespaces();
        let excluded_ids = self
            .stable_core_memories
            .iter()
            .map(|entry| entry.id.as_str())
            .collect::<Vec<_>>();
        let core_category = MemoryCategory::Core;
        let mut entries = Vec::new();
        for namespace in &namespaces {
            let recalled = self
                .memory
                .recall_scoped(MemoryQuery {
                    query: user_message,
                    scope: MemoryScope {
                        namespace,
                        session_id: None,
                    },
                    category: Some(&core_category),
                    since: None,
                    until: None,
                    limit: self.auto_recall_limit,
                    min_relevance_score: Some(self.memory_min_relevance_score),
                    search_mode: None,
                    exclude_ids: &excluded_ids,
                    exclude_keys: &[],
                })
                .await;
            match recalled {
                Ok(found) => entries.extend(found),
                Err(error) => {
                    tracing::debug!(%error, %namespace, "dynamic Core recall skipped");
                }
            }
        }
        entries.retain(|entry| !self.memory_is_covered_by_profile(entry));
        entries.sort_by(|left, right| {
            right
                .score
                .unwrap_or_default()
                .partial_cmp(&left.score.unwrap_or_default())
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| right.timestamp.cmp(&left.timestamp))
                .then_with(|| left.key.cmp(&right.key))
        });
        entries.truncate(self.auto_recall_limit);
        let context = entries
            .iter()
            .map(|entry| format!("- {}: {}", entry.key, entry.content))
            .collect::<Vec<_>>()
            .join("\n");
        if !context.is_empty()
            && let Some(ConversationMessage::Chat(message)) = self.history.last_mut()
        {
            message.content = format!(
                "[Memory context]\n{context}\n[/Memory context]\n\n{}",
                message.content
            );
        }
    }

    /// Execute tool calls, handle skill activations, format results, and append to history.
    async fn process_tool_calls(&mut self, calls: &[ParsedToolCall]) -> Vec<ToolExecutionResult> {
        let mut results = self.execute_tools(calls).await;
        self.handle_skill_tool_results(calls, &mut results);
        self.handle_skill_create_results(calls, &mut results);
        let formatted = self.tool_dispatcher.format_results(&results);
        self.history.push(formatted);
        self.trim_history();
        results
    }

    /// Execute a single agent turn: send message, dispatch tools, return final text.
    pub async fn turn(&mut self, user_message: &str) -> Result<String> {
        if self.refresh_user_profile().await && !self.history.is_empty() {
            self.rebuild_system_prompt()?;
        }
        // Refresh stable Core memories before preparing the turn.
        // This ensures the first system prompt includes stable memories,
        // and subsequent turns rebuild if Core memories changed.
        if self.refresh_stable_core_memories().await && !self.history.is_empty() {
            self.rebuild_system_prompt()?;
        }
        self.prepare_turn(user_message)?;
        // No dynamic refresh needed — with DateTimeSection removed, the entire
        // system prompt is stable across turns. Time context comes from the
        // user message timestamp prefix.
        // Auto-recall relevant memories and prepend context to user message.
        // Only Core memories are recalled — Daily and Conversation are excluded
        // to keep the context focused on truly important facts.
        // When stable memory injection is enabled, entries already in the system
        // prompt (tracked by injected_core_state) are deduplicated.
        self.inject_recalled_memory(user_message).await;

        let effective_model = self.model_name.clone();

        let mut auto_continue_count: usize = 0;

        for _ in 0..self.config.max_tool_iterations {
            let messages = self.tool_dispatcher.to_provider_messages(&self.history);

            let tool_specs = self.tool_registry.tool_specs();
            let response = match self
                .provider
                .chat(
                    ChatRequest {
                        messages: &messages,
                        tools: if self.tool_dispatcher.should_send_tool_specs() {
                            Some(&tool_specs)
                        } else {
                            None
                        },
                        provider_extra: self.provider_extra.as_ref(),
                    },
                    &effective_model,
                    Some(self.temperature),
                )
                .await
            {
                Ok(resp) => resp,
                Err(err) => return Err(err),
            };

            let (text, calls) = self.tool_dispatcher.parse_response(&response);
            if calls.is_empty() {
                let final_text = if text.is_empty() {
                    response.text.unwrap_or_default()
                } else {
                    text
                };

                self.history
                    .push(ConversationMessage::Chat(ChatMessage::assistant(
                        final_text.clone(),
                    )));
                self.trim_history();

                // Auto-continue when truncated due to max_tokens
                if response.stop_reason == clawseed_api::provider::StopReason::MaxTokens
                    && self.config.auto_continue_on_truncation
                    && auto_continue_count < self.config.max_auto_continue
                {
                    auto_continue_count += 1;
                    tracing::warn!(
                        auto_continue = auto_continue_count,
                        max = self.config.max_auto_continue,
                        "Response truncated by max_tokens, auto-continuing"
                    );
                    print!("\n[⚠ 输出被截断，自动续接中...]\n");
                    use std::io::Write;
                    let _ = std::io::stdout().lock().flush();
                    self.history
                        .push(ConversationMessage::Chat(ChatMessage::user(
                            "请继续输出，不要重复已输出的内容",
                        )));
                    continue;
                }

                self.complete_turn(user_message, &final_text);
                return Ok(final_text);
            }

            if !text.is_empty() {
                print!("{text}");
                use std::io::Write;
                let _ = std::io::stdout().lock().flush();
            }

            self.history.push(ConversationMessage::AssistantToolCalls {
                text: response.text.clone(),
                tool_calls: response.tool_calls.clone(),
                reasoning_content: response.reasoning_content.clone(),
            });

            let _results = self.process_tool_calls(&calls).await;
        }

        anyhow::bail!(
            "Agent exceeded maximum tool iterations ({})",
            self.config.max_tool_iterations
        )
    }

    /// Execute a single agent turn while streaming intermediate events.
    pub async fn turn_streamed(
        &mut self,
        user_message: &str,
        event_tx: tokio::sync::mpsc::Sender<TurnEvent>,
        cancel_token: Option<tokio_util::sync::CancellationToken>,
        debug: bool,
    ) -> Result<String> {
        if self.refresh_user_profile().await && !self.history.is_empty() {
            self.rebuild_system_prompt()?;
        }
        // Refresh stable Core memories before preparing the turn.
        if self.refresh_stable_core_memories().await && !self.history.is_empty() {
            self.rebuild_system_prompt()?;
        }
        self.prepare_turn(user_message)?;

        // Auto-recall relevant memories and prepend context to user message.
        // Only Core memories are recalled — Daily and Conversation are excluded
        // to keep the context focused on truly important facts.
        // When stable memory injection is enabled, entries already in the system
        // prompt (tracked by injected_core_state) are deduplicated.
        self.inject_recalled_memory(user_message).await;

        let effective_model = self.model_name.clone();

        // Try streaming first, fall back to non-streaming
        use futures_util::StreamExt;

        let mut auto_continue_count: usize = 0;

        for iteration in 0..self.config.max_tool_iterations {
            if cancel_token
                .as_ref()
                .is_some_and(tokio_util::sync::CancellationToken::is_cancelled)
            {
                return Err(anyhow::anyhow!("ToolLoopCancelled"));
            }

            let messages = self.tool_dispatcher.to_provider_messages(&self.history);

            if debug && iteration == 0 {
                let messages_json = serde_json::to_string(&messages).unwrap_or_default();
                let estimated_tokens = crate::history::estimate_history_tokens(&messages);
                let _ = event_tx
                    .send(TurnEvent::DebugPrompt {
                        messages_json,
                        estimated_tokens,
                    })
                    .await;
            }

            // Try streaming
            let stream_opts = clawseed_api::provider::StreamOptions::new(true);
            let tool_specs = self.tool_registry.tool_specs();
            let mut stream = self.provider.stream_chat(
                ChatRequest {
                    messages: &messages,
                    tools: if self.tool_dispatcher.should_send_tool_specs() {
                        Some(&tool_specs)
                    } else {
                        None
                    },
                    provider_extra: self.provider_extra.as_ref(),
                },
                &effective_model,
                Some(self.temperature),
                stream_opts,
            );

            let mut streamed_text = String::new();
            let mut streamed_reasoning = String::new();
            let mut streamed_tool_calls: Vec<clawseed_api::provider::ToolCall> = Vec::new();
            let mut streamed_stop_reason = clawseed_api::provider::StopReason::EndTurn;
            let mut got_stream = false;

            loop {
                let next_item = stream.next();
                let item = if let Some(ref token) = cancel_token {
                    tokio::select! {
                        biased;
                        () = token.cancelled() => break,
                        item = next_item => item,
                    }
                } else {
                    next_item.await
                };

                let Some(item) = item else { break };
                match item {
                    Ok(event) => match event {
                        clawseed_api::provider::StreamEvent::TextDelta(chunk) => {
                            if let Some(reasoning) = chunk.reasoning
                                && !reasoning.is_empty()
                            {
                                streamed_reasoning.push_str(&reasoning);
                                let _ = event_tx
                                    .send(TurnEvent::Thinking { delta: reasoning })
                                    .await;
                            }
                            if !chunk.delta.is_empty() {
                                got_stream = true;
                                streamed_text.push_str(&chunk.delta);
                                let _ =
                                    event_tx.send(TurnEvent::Chunk { delta: chunk.delta }).await;
                            }
                        }
                        clawseed_api::provider::StreamEvent::ToolCall(tc) => {
                            got_stream = true;
                            streamed_tool_calls.push(tc);
                        }
                        clawseed_api::provider::StreamEvent::PreExecutedToolCall { name, args } => {
                            let call_id = uuid::Uuid::new_v4().to_string();
                            let _ = event_tx
                                .send(TurnEvent::ToolCall {
                                    id: call_id,
                                    name,
                                    args: serde_json::from_str(&args).unwrap_or_default(),
                                })
                                .await;
                        }
                        clawseed_api::provider::StreamEvent::PreExecutedToolResult {
                            name,
                            output,
                        } => {
                            let result_id = uuid::Uuid::new_v4().to_string();
                            let _ = event_tx
                                .send(TurnEvent::ToolResult {
                                    id: result_id,
                                    name,
                                    output,
                                    presentation: None,
                                })
                                .await;
                        }
                        clawseed_api::provider::StreamEvent::Final { stop_reason } => {
                            streamed_stop_reason = stop_reason;
                            break;
                        }
                    },
                    Err(_) => break,
                }
            }
            drop(stream);

            let response = if got_stream {
                ChatResponse {
                    text: Some(streamed_text),
                    tool_calls: streamed_tool_calls,
                    usage: None,
                    reasoning_content: if streamed_reasoning.is_empty() {
                        None
                    } else {
                        Some(streamed_reasoning)
                    },
                    stop_reason: streamed_stop_reason,
                }
            } else {
                // Fall back to non-streaming
                let tool_specs = self.tool_registry.tool_specs();
                let chat_result = self.provider.chat(
                    ChatRequest {
                        messages: &messages,
                        tools: if self.tool_dispatcher.should_send_tool_specs() {
                            Some(&tool_specs)
                        } else {
                            None
                        },
                        provider_extra: self.provider_extra.as_ref(),
                    },
                    &effective_model,
                    Some(self.temperature),
                );
                match chat_result.await {
                    Ok(resp) => resp,
                    Err(err) => return Err(err),
                }
            };

            let (text, mut calls) = self.tool_dispatcher.parse_response(&response);
            if calls.is_empty() {
                let final_text = if text.is_empty() {
                    response.text.unwrap_or_default()
                } else {
                    text
                };

                if !got_stream && !final_text.is_empty() {
                    let _ = event_tx
                        .send(TurnEvent::Chunk {
                            delta: final_text.clone(),
                        })
                        .await;
                }

                self.history
                    .push(ConversationMessage::Chat(ChatMessage::assistant(
                        final_text.clone(),
                    )));
                self.trim_history();

                // Auto-continue when truncated due to max_tokens
                if response.stop_reason == clawseed_api::provider::StopReason::MaxTokens
                    && self.config.auto_continue_on_truncation
                    && auto_continue_count < self.config.max_auto_continue
                {
                    auto_continue_count += 1;
                    tracing::warn!(
                        auto_continue = auto_continue_count,
                        max = self.config.max_auto_continue,
                        "Response truncated by max_tokens, auto-continuing"
                    );
                    let _ = event_tx
                        .send(TurnEvent::Chunk {
                            delta: "\n[⚠ 输出被截断，自动续接中...]\n".to_string(),
                        })
                        .await;
                    self.history
                        .push(ConversationMessage::Chat(ChatMessage::user(
                            "请继续输出，不要重复已输出的内容",
                        )));
                    continue;
                }

                self.complete_turn(user_message, &final_text);
                return Ok(final_text);
            }

            // Assign IDs to tool calls
            for call in &mut calls {
                if call.tool_call_id.is_none() {
                    call.tool_call_id = Some(uuid::Uuid::new_v4().to_string());
                }
            }

            self.history.push(ConversationMessage::AssistantToolCalls {
                text: response.text.clone(),
                tool_calls: response.tool_calls.clone(),
                reasoning_content: response.reasoning_content.clone(),
            });

            for call in &calls {
                let call_id = call.tool_call_id.as_ref().unwrap().clone();
                let _ = event_tx
                    .send(TurnEvent::ToolCall {
                        id: call_id,
                        name: call.name.clone(),
                        args: call.arguments.clone(),
                    })
                    .await;
            }

            let mut results = self.execute_tools(&calls).await;

            // Handle skill activations BEFORE emitting events or formatting to history.
            self.handle_skill_tool_results(&calls, &mut results);
            self.handle_skill_create_results(&calls, &mut results);

            for result in &results {
                let result_id = result.tool_call_id.as_ref().unwrap().clone();
                let _ = event_tx
                    .send(TurnEvent::ToolResult {
                        id: result_id,
                        name: result.name.clone(),
                        output: result.output.clone(),
                        presentation: result.presentation.clone(),
                    })
                    .await;
            }

            let formatted = self.tool_dispatcher.format_results(&results);
            self.history.push(formatted);
            self.trim_history();
        }

        anyhow::bail!(
            "Agent exceeded maximum tool iterations ({})",
            self.config.max_tool_iterations
        )
    }

    pub async fn run_single(&mut self, message: &str) -> Result<String> {
        self.turn(message).await
    }
}
