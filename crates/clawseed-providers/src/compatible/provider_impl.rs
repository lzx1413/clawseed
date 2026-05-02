use super::OpenAiCompatibleProvider;
use super::auth::apply_auth_to_request;
use super::parsing::*;
use crate::multimodal;
use crate::traits::{
    ChatMessage, ChatRequest as ProviderChatRequest, ChatResponse as ProviderChatResponse,
    Provider, StreamChunk, StreamError, StreamEvent, StreamOptions, StreamResult, TokenUsage,
    ToolCall as ProviderToolCall,
};
use async_trait::async_trait;
use futures_util::{StreamExt, stream};

impl OpenAiCompatibleProvider {
    fn apply_auth_header(
        &self,
        req: reqwest::RequestBuilder,
        credential: Option<&str>,
    ) -> reqwest::RequestBuilder {
        apply_auth_to_request(req, &self.auth_header, credential)
    }

    async fn chat_via_responses(
        &self,
        credential: Option<&str>,
        messages: &[ChatMessage],
        model: &str,
    ) -> anyhow::Result<String> {
        let (instructions, input) = build_responses_prompt(messages);
        if input.is_empty() {
            anyhow::bail!(
                "{} Responses API fallback requires at least one non-system message",
                self.name
            );
        }

        let request = ResponsesRequest {
            model: model.to_string(),
            input,
            instructions,
            stream: Some(false),
        };

        let url = self.responses_url();

        let response = self
            .apply_auth_header(self.http_client().post(&url).json(&request), credential)
            .send()
            .await?;

        if !response.status().is_success() {
            let error = response.text().await?;
            anyhow::bail!("{} Responses API error: {error}", self.name);
        }

        let body = response.text().await?;
        let responses = parse_responses_response_body(&self.name, &body)?;

        extract_responses_text(responses)
            .ok_or_else(|| anyhow::anyhow!("No response from {} Responses API", self.name))
    }

    fn convert_tool_specs(
        tools: Option<&[clawseed_api::tool::ToolSpec]>,
    ) -> Option<Vec<serde_json::Value>> {
        tools.map(|items| {
            items
                .iter()
                .map(|tool| {
                    let params = clawseed_api::schema::SchemaCleanr::clean_for_openai(
                        tool.parameters.clone(),
                    );
                    serde_json::json!({
                        "type": "function",
                        "function": {
                            "name": tool.name,
                            "description": tool.description,
                            "parameters": params,
                        }
                    })
                })
                .collect()
        })
    }

    fn to_message_content(
        role: &str,
        content: &str,
        allow_user_image_parts: bool,
    ) -> MessageContent {
        if role != "user" || !allow_user_image_parts {
            return MessageContent::Text(content.to_string());
        }

        let (cleaned_text, image_refs) = multimodal::parse_image_markers(content);
        if image_refs.is_empty() {
            return MessageContent::Text(content.to_string());
        }

        let mut parts = Vec::with_capacity(image_refs.len() + 1);
        let trimmed_text = cleaned_text.trim();
        if !trimmed_text.is_empty() {
            parts.push(MessagePart::Text {
                text: trimmed_text.to_string(),
            });
        }

        for image_ref in image_refs {
            parts.push(MessagePart::ImageUrl {
                image_url: ImageUrlPart { url: image_ref },
            });
        }

        MessageContent::Parts(parts)
    }

    fn convert_messages_for_native(
        messages: &[ChatMessage],
        allow_user_image_parts: bool,
    ) -> Vec<NativeMessage> {
        messages
            .iter()
            .map(|message| {
                if message.role == "assistant"
                    && let Ok(value) = serde_json::from_str::<serde_json::Value>(&message.content)
                    && let Some(tool_calls_value) = value.get("tool_calls")
                    && let Ok(parsed_calls) =
                        serde_json::from_value::<Vec<ProviderToolCall>>(tool_calls_value.clone())
                {
                    let tool_calls = parsed_calls
                        .into_iter()
                        .map(|tc| ToolCall {
                            id: Some(tc.id),
                            kind: Some("function".to_string()),
                            function: Some(Function {
                                name: Some(tc.name),
                                arguments: Some(tc.arguments),
                            }),
                            name: None,
                            arguments: None,
                            parameters: None,
                        })
                        .collect::<Vec<_>>();

                    let content = value
                        .get("content")
                        .and_then(serde_json::Value::as_str)
                        .map(|value| MessageContent::Text(value.to_string()));

                    let reasoning_content = value
                        .get("reasoning_content")
                        .and_then(serde_json::Value::as_str)
                        .map(ToString::to_string);

                    return NativeMessage {
                        role: "assistant".to_string(),
                        content,
                        tool_call_id: None,
                        tool_calls: Some(tool_calls),
                        reasoning_content,
                    };
                }

                if message.role == "tool"
                    && let Ok(value) = serde_json::from_str::<serde_json::Value>(&message.content)
                {
                    let tool_call_id = value
                        .get("tool_call_id")
                        .and_then(serde_json::Value::as_str)
                        .map(ToString::to_string);
                    let content = value
                        .get("content")
                        .and_then(serde_json::Value::as_str)
                        .map(|value| MessageContent::Text(value.to_string()))
                        .or_else(|| Some(MessageContent::Text(message.content.clone())));

                    return NativeMessage {
                        role: "tool".to_string(),
                        content,
                        tool_call_id,
                        tool_calls: None,
                        reasoning_content: None,
                    };
                }

                NativeMessage {
                    role: message.role.clone(),
                    content: Some(Self::to_message_content(
                        &message.role,
                        &message.content,
                        allow_user_image_parts,
                    )),
                    tool_call_id: None,
                    tool_calls: None,
                    reasoning_content: None,
                }
            })
            .collect()
    }

    /// Strip native tool-calling constructs from messages for providers that
    /// do not support native tool calling (e.g. MiniMax).
    ///
    /// Conversation history may contain tool-role messages and assistant
    /// messages with `tool_calls` JSON from previous sessions or from
    /// provider switches.  Sending these to a non-native-tool provider
    /// causes hard API errors like MiniMax's
    /// "tool result's tool id not found" (#5743).
    ///
    /// - **tool-role messages** are dropped entirely.
    /// - **assistant messages with `tool_calls`** are converted to plain
    ///   text by extracting only the `content` field (or dropped when the
    ///   content is empty).
    fn strip_native_tool_messages(&self, messages: &[ChatMessage]) -> Vec<ChatMessage> {
        if self.native_tool_calling {
            return messages.to_vec();
        }
        let intermediate = messages.iter().filter_map(|msg| {
            if msg.role == "tool" {
                return None;
            }
            if msg.role == "assistant"
                && let Ok(value) = serde_json::from_str::<serde_json::Value>(&msg.content)
                && value.get("tool_calls").is_some()
            {
                let text = value
                    .get("content")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("")
                    .to_string();
                return if text.is_empty() {
                    None
                } else {
                    Some(ChatMessage::assistant(&text))
                };
            }
            Some(msg.clone())
        });

        // Coalesce adjacent assistant messages.
        //
        // A typical trace is:
        //     user → assistant{content, tool_calls} → tool{result} → assistant{reply}
        // After the filter_map above the `tool` message is gone and the first
        // assistant has been rewritten to plain text, leaving two assistant
        // messages in a row. Providers targeted by the `native_tool_calling =
        // false` path (Anthropic upstream, MiniMax, and other OpenAI-compat
        // wrappers) reject consecutive same-role messages with HTTP 400, so we
        // merge them here. See #5825.
        let mut coalesced: Vec<ChatMessage> = Vec::with_capacity(messages.len());
        for msg in intermediate {
            match coalesced.last_mut() {
                Some(last) if last.role == "assistant" && msg.role == "assistant" => {
                    if !last.content.is_empty() && !msg.content.is_empty() {
                        last.content.push_str("\n\n");
                    }
                    last.content.push_str(&msg.content);
                }
                _ => coalesced.push(msg),
            }
        }
        coalesced
    }

    fn with_prompt_guided_tool_instructions(
        messages: &[ChatMessage],
        tools: Option<&[clawseed_api::tool::ToolSpec]>,
    ) -> Vec<ChatMessage> {
        let Some(tools) = tools else {
            return messages.to_vec();
        };

        if tools.is_empty() {
            return messages.to_vec();
        }

        let instructions = clawseed_api::provider::build_tool_instructions_text(tools);
        let mut modified_messages = messages.to_vec();

        if let Some(system_message) = modified_messages.iter_mut().find(|m| m.role == "system") {
            if !system_message.content.is_empty() {
                system_message.content.push_str("\n\n");
            }
            system_message.content.push_str(&instructions);
        } else {
            modified_messages.insert(0, ChatMessage::system(instructions));
        }

        modified_messages
    }

    fn parse_native_response(message: ResponseMessage) -> ProviderChatResponse {
        let text = message.effective_content_optional();
        let reasoning_content = message.reasoning_content.clone();
        let tool_calls = message
            .tool_calls
            .unwrap_or_default()
            .into_iter()
            .filter_map(|tc| {
                let name = tc.function_name()?;
                let arguments = tc.function_arguments().unwrap_or_else(|| "{}".to_string());
                let normalized_arguments =
                    if serde_json::from_str::<serde_json::Value>(&arguments).is_ok() {
                        arguments
                    } else {
                        tracing::warn!(
                            function = %name,
                            arguments = %arguments,
                            "Invalid JSON in native tool-call arguments, using empty object"
                        );
                        "{}".to_string()
                    };
                Some(ProviderToolCall {
                    id: tc.id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
                    name,
                    arguments: normalized_arguments,
                })
            })
            .collect::<Vec<_>>();

        ProviderChatResponse {
            text,
            tool_calls,
            usage: None,
            reasoning_content,
        }
    }

    fn is_native_tool_schema_unsupported(status: reqwest::StatusCode, error: &str) -> bool {
        if !matches!(
            status,
            reqwest::StatusCode::BAD_REQUEST | reqwest::StatusCode::UNPROCESSABLE_ENTITY
        ) {
            return false;
        }

        let lower = error.to_lowercase();
        [
            "unknown parameter: tools",
            "unsupported parameter: tools",
            "unrecognized field `tools`",
            "does not support tools",
            "function calling is not supported",
            "tool_choice",
            "tool call validation failed",
            "was not in request",
        ]
        .iter()
        .any(|hint| lower.contains(hint))
    }
}

#[async_trait]
impl Provider for OpenAiCompatibleProvider {
    fn capabilities(&self) -> clawseed_api::provider::ProviderCapabilities {
        clawseed_api::provider::ProviderCapabilities {
            native_tool_calling: self.native_tool_calling,
            vision: self.supports_vision,
            prompt_caching: false,
        }
    }

    async fn chat_with_system(
        &self,
        system_prompt: Option<&str>,
        message: &str,
        model: &str,
        temperature: Option<f64>,
    ) -> anyhow::Result<String> {
        let temperature = temperature.unwrap_or(self.default_temperature());
        let credential = self.credential.as_deref();

        let merge = self.effective_merge_system(model);
        let mut messages = Vec::new();

        if merge {
            let content = match system_prompt {
                Some(sys) => format!("{sys}\n\n{message}"),
                None => message.to_string(),
            };
            messages.push(Message {
                role: "user".to_string(),
                content: Self::to_message_content("user", &content, !merge),
            });
        } else {
            if let Some(sys) = system_prompt {
                messages.push(Message {
                    role: "system".to_string(),
                    content: MessageContent::Text(sys.to_string()),
                });
            }
            messages.push(Message {
                role: "user".to_string(),
                content: Self::to_message_content("user", message, true),
            });
        }

        let request = ApiChatRequest {
            model: model.to_string(),
            messages,
            temperature,
            stream: Some(false),
            reasoning_effort: self.reasoning_effort_for_model(model),
            tool_stream: None,
            tools: None,
            tool_choice: None,
            max_tokens: self.max_tokens,
        };

        let url = self.chat_completions_url();

        let mut fallback_messages = Vec::new();
        if let Some(system_prompt) = system_prompt {
            fallback_messages.push(ChatMessage::system(system_prompt));
        }
        fallback_messages.push(ChatMessage::user(message));
        let fallback_messages = Self::flatten_system_messages(&fallback_messages, merge);

        let response = match self
            .apply_auth_header(self.http_client().post(&url).json(&request), credential)
            .send()
            .await
        {
            Ok(response) => response,
            Err(chat_error) => {
                if self.supports_responses_fallback {
                    let sanitized = crate::sanitize_api_error(&chat_error.to_string());
                    return self
                        .chat_via_responses(credential, &fallback_messages, model)
                        .await
                        .map_err(|responses_err| {
                            anyhow::anyhow!(
                                "{} chat completions transport error: {sanitized} (responses fallback failed: {responses_err})",
                                self.name
                            )
                        });
                }

                return Err(chat_error.into());
            }
        };

        if !response.status().is_success() {
            let status = response.status();
            let error = response.text().await?;
            let sanitized = crate::sanitize_api_error(&error);

            if status == reqwest::StatusCode::NOT_FOUND && self.supports_responses_fallback {
                return self
                    .chat_via_responses(credential, &fallback_messages, model)
                    .await
                    .map_err(|responses_err| {
                        anyhow::anyhow!(
                            "{} API error ({status}): {sanitized} (chat completions unavailable; responses fallback failed: {responses_err})",
                            self.name
                        )
                    });
            }

            anyhow::bail!("{} API error ({status}): {sanitized}", self.name);
        }

        let body = response.text().await?;
        let chat_response = parse_chat_response_body(&self.name, &body)?;

        chat_response
            .choices
            .into_iter()
            .next()
            .map(|c| {
                // If tool_calls are present, serialize the full message as JSON
                // so parse_tool_calls can handle the OpenAI-style format
                if c.message.tool_calls.is_some()
                    && c.message.tool_calls.as_ref().is_some_and(|t| !t.is_empty())
                {
                    serde_json::to_string(&c.message)
                        .unwrap_or_else(|_| c.message.effective_content())
                } else {
                    // No tool calls, return content (with reasoning_content fallback)
                    c.message.effective_content()
                }
            })
            .ok_or_else(|| anyhow::anyhow!("No response from {}", self.name))
    }

    async fn chat_with_history(
        &self,
        messages: &[ChatMessage],
        model: &str,
        temperature: Option<f64>,
    ) -> anyhow::Result<String> {
        let temperature = temperature.unwrap_or(self.default_temperature());
        let credential = self.credential.as_deref();

        let merge = self.effective_merge_system(model);
        let effective_messages = Self::flatten_system_messages(messages, merge);
        // Strip native tool constructs for non-native-tool providers (#5743).
        let effective_messages = self.strip_native_tool_messages(&effective_messages);
        let api_messages: Vec<Message> = effective_messages
            .iter()
            .map(|m| Message {
                role: m.role.clone(),
                content: Self::to_message_content(&m.role, &m.content, !merge),
            })
            .collect();

        let request = ApiChatRequest {
            model: model.to_string(),
            messages: api_messages,
            temperature,
            stream: Some(false),
            reasoning_effort: self.reasoning_effort_for_model(model),
            tool_stream: None,
            tools: None,
            tool_choice: None,
            max_tokens: self.max_tokens,
        };

        let url = self.chat_completions_url();
        let response = match self
            .apply_auth_header(self.http_client().post(&url).json(&request), credential)
            .send()
            .await
        {
            Ok(response) => response,
            Err(chat_error) => {
                if self.supports_responses_fallback {
                    let sanitized = crate::sanitize_api_error(&chat_error.to_string());
                    return self
                        .chat_via_responses(credential, &effective_messages, model)
                        .await
                        .map_err(|responses_err| {
                            anyhow::anyhow!(
                                "{} chat completions transport error: {sanitized} (responses fallback failed: {responses_err})",
                                self.name
                            )
                        });
                }

                return Err(chat_error.into());
            }
        };

        if !response.status().is_success() {
            let status = response.status();

            // Mirror chat_with_system: 404 may mean this provider uses the Responses API
            if status == reqwest::StatusCode::NOT_FOUND && self.supports_responses_fallback {
                return self
                    .chat_via_responses(credential, &effective_messages, model)
                    .await
                    .map_err(|responses_err| {
                        anyhow::anyhow!(
                            "{} API error (chat completions unavailable; responses fallback failed: {responses_err})",
                            self.name
                        )
                    });
            }

            return Err(crate::api_error(&self.name, response).await);
        }

        let body = response.text().await?;
        let chat_response = parse_chat_response_body(&self.name, &body)?;

        chat_response
            .choices
            .into_iter()
            .next()
            .map(|c| {
                // If tool_calls are present, serialize the full message as JSON
                // so parse_tool_calls can handle the OpenAI-style format
                if c.message.tool_calls.is_some()
                    && c.message.tool_calls.as_ref().is_some_and(|t| !t.is_empty())
                {
                    serde_json::to_string(&c.message)
                        .unwrap_or_else(|_| c.message.effective_content())
                } else {
                    // No tool calls, return content (with reasoning_content fallback)
                    c.message.effective_content()
                }
            })
            .ok_or_else(|| anyhow::anyhow!("No response from {}", self.name))
    }

    async fn chat_with_tools(
        &self,
        messages: &[ChatMessage],
        tools: &[serde_json::Value],
        model: &str,
        temperature: Option<f64>,
    ) -> anyhow::Result<ProviderChatResponse> {
        let temperature = temperature.unwrap_or(self.default_temperature());
        let credential = self.credential.as_deref();

        let merge = self.effective_merge_system(model);
        let effective_messages = Self::flatten_system_messages(messages, merge);
        let effective_messages = self.strip_native_tool_messages(&effective_messages);
        let api_messages: Vec<Message> = effective_messages
            .iter()
            .map(|m| Message {
                role: m.role.clone(),
                content: Self::to_message_content(&m.role, &m.content, !merge),
            })
            .collect();

        let request = ApiChatRequest {
            model: model.to_string(),
            messages: api_messages,
            temperature,
            stream: Some(false),
            reasoning_effort: self.reasoning_effort_for_model(model),
            tool_stream: self.tool_stream_for_tools(!tools.is_empty()),
            tools: if tools.is_empty() {
                None
            } else {
                Some(tools.to_vec())
            },
            tool_choice: if tools.is_empty() {
                None
            } else {
                Some("auto".to_string())
            },
            max_tokens: self.max_tokens,
        };

        let url = self.chat_completions_url();
        let response = match self
            .apply_auth_header(self.http_client().post(&url).json(&request), credential)
            .send()
            .await
        {
            Ok(response) => response,
            Err(error) => {
                tracing::warn!(
                    "{} native tool call transport failed: {error}; falling back to history path",
                    self.name
                );
                let text = self
                    .chat_with_history(messages, model, Some(temperature))
                    .await?;
                return Ok(ProviderChatResponse {
                    text: Some(text),
                    tool_calls: vec![],
                    usage: None,
                    reasoning_content: None,
                });
            }
        };

        if !response.status().is_success() {
            return Err(crate::api_error(&self.name, response).await);
        }

        let body = response.text().await?;
        let chat_response = parse_chat_response_body(&self.name, &body)?;
        let usage = chat_response.usage.map(|u| TokenUsage {
            input_tokens: u.prompt_tokens,
            output_tokens: u.completion_tokens,
            cached_input_tokens: None,
        });
        let choice = chat_response
            .choices
            .into_iter()
            .next()
            .ok_or_else(|| anyhow::anyhow!("No response from {}", self.name))?;

        let text = choice.message.effective_content_optional();
        let reasoning_content = choice.message.reasoning_content;
        let tool_calls = choice
            .message
            .tool_calls
            .unwrap_or_default()
            .into_iter()
            .filter_map(|tc| {
                let function = tc.function?;
                let name = function.name?;
                let arguments = function.arguments.unwrap_or_else(|| "{}".to_string());
                Some(ProviderToolCall {
                    id: uuid::Uuid::new_v4().to_string(),
                    name,
                    arguments,
                })
            })
            .collect::<Vec<_>>();

        Ok(ProviderChatResponse {
            text,
            tool_calls,
            usage,
            reasoning_content,
        })
    }

    async fn chat(
        &self,
        request: ProviderChatRequest<'_>,
        model: &str,
        temperature: Option<f64>,
    ) -> anyhow::Result<ProviderChatResponse> {
        let temperature = temperature.unwrap_or(self.default_temperature());
        let credential = self.credential.as_deref();

        let merge = self.effective_merge_system(model);
        let tools = Self::convert_tool_specs(request.tools);
        let effective_messages = Self::flatten_system_messages(request.messages, merge);
        let effective_messages = self.strip_native_tool_messages(&effective_messages);
        let native_request = NativeChatRequest {
            model: model.to_string(),
            messages: Self::convert_messages_for_native(&effective_messages, !merge),
            temperature,
            stream: Some(false),
            reasoning_effort: self.reasoning_effort_for_model(model),
            tool_stream: self
                .tool_stream_for_tools(tools.as_ref().is_some_and(|tools| !tools.is_empty())),
            tool_choice: tools.as_ref().map(|_| "auto".to_string()),
            tools,
            max_tokens: self.max_tokens,
        };

        let url = self.chat_completions_url();
        let response = match self
            .apply_auth_header(
                self.http_client().post(&url).json(&native_request),
                credential,
            )
            .send()
            .await
        {
            Ok(response) => response,
            Err(chat_error) => {
                if self.supports_responses_fallback {
                    let sanitized = crate::sanitize_api_error(&chat_error.to_string());
                    return self
                        .chat_via_responses(credential, &effective_messages, model)
                        .await
                        .map(|text| ProviderChatResponse {
                            text: Some(text),
                            tool_calls: vec![],
                            usage: None,
                            reasoning_content: None,
                        })
                        .map_err(|responses_err| {
                            anyhow::anyhow!(
                                "{} native chat transport error: {sanitized} (responses fallback failed: {responses_err})",
                                self.name
                            )
                        });
                }

                return Err(chat_error.into());
            }
        };

        if !response.status().is_success() {
            let status = response.status();
            let error = response.text().await?;
            let sanitized = crate::sanitize_api_error(&error);

            if Self::is_native_tool_schema_unsupported(status, &sanitized) {
                let fallback_messages =
                    Self::with_prompt_guided_tool_instructions(request.messages, request.tools);
                let text = self
                    .chat_with_history(&fallback_messages, model, Some(temperature))
                    .await?;
                return Ok(ProviderChatResponse {
                    text: Some(text),
                    tool_calls: vec![],
                    usage: None,
                    reasoning_content: None,
                });
            }

            if status == reqwest::StatusCode::NOT_FOUND && self.supports_responses_fallback {
                return self
                    .chat_via_responses(credential, &effective_messages, model)
                    .await
                    .map(|text| ProviderChatResponse {
                        text: Some(text),
                        tool_calls: vec![],
                        usage: None,
                        reasoning_content: None,
                    })
                    .map_err(|responses_err| {
                        anyhow::anyhow!(
                            "{} API error ({status}): {sanitized} (chat completions unavailable; responses fallback failed: {responses_err})",
                            self.name
                        )
                    });
            }

            anyhow::bail!("{} API error ({status}): {sanitized}", self.name);
        }

        let native_response: ApiChatResponse = response.json().await?;
        let usage = native_response.usage.map(|u| TokenUsage {
            input_tokens: u.prompt_tokens,
            output_tokens: u.completion_tokens,
            cached_input_tokens: None,
        });
        let message = native_response
            .choices
            .into_iter()
            .next()
            .map(|choice| choice.message)
            .ok_or_else(|| anyhow::anyhow!("No response from {}", self.name))?;

        let mut result = Self::parse_native_response(message);
        result.usage = usage;
        Ok(result)
    }

    fn supports_native_tools(&self) -> bool {
        self.native_tool_calling
    }

    fn supports_streaming(&self) -> bool {
        true
    }

    fn supports_streaming_tool_events(&self) -> bool {
        self.native_tool_calling
    }

    fn stream_chat(
        &self,
        request: ProviderChatRequest<'_>,
        model: &str,
        temperature: Option<f64>,
        options: StreamOptions,
    ) -> stream::BoxStream<'static, StreamResult<StreamEvent>> {
        if !options.enabled {
            return stream::once(async { Ok(StreamEvent::Final) }).boxed();
        }

        let temperature = temperature.unwrap_or(self.default_temperature());
        let credential = self.credential.clone();

        let merge = self.effective_merge_system(model);
        let has_tools = request.tools.is_some_and(|tools| !tools.is_empty());
        let effective_messages = Self::flatten_system_messages(request.messages, merge);
        let effective_messages = self.strip_native_tool_messages(&effective_messages);

        let tools = Self::convert_tool_specs(request.tools);
        let payload = if has_tools {
            serde_json::to_value(NativeChatRequest {
                model: model.to_string(),
                messages: Self::convert_messages_for_native(&effective_messages, !merge),
                temperature,
                reasoning_effort: self.reasoning_effort.clone(),
                tool_stream: if options.enabled {
                    self.tool_stream_for_tools(true)
                } else {
                    None
                },
                stream: Some(options.enabled),
                tools: tools.clone(),
                tool_choice: tools.as_ref().map(|_| "auto".to_string()),
                max_tokens: self.max_tokens,
            })
        } else {
            let messages = effective_messages
                .iter()
                .map(|message| Message {
                    role: message.role.clone(),
                    content: Self::to_message_content(&message.role, &message.content, !merge),
                })
                .collect();

            serde_json::to_value(ApiChatRequest {
                model: model.to_string(),
                messages,
                temperature,
                reasoning_effort: self.reasoning_effort.clone(),
                tool_stream: if options.enabled {
                    self.tool_stream_for_tools(false)
                } else {
                    None
                },
                stream: Some(options.enabled),
                tools: None,
                tool_choice: None,
                max_tokens: self.max_tokens,
            })
        };

        let payload = match payload {
            Ok(payload) => payload,
            Err(error) => {
                return stream::once(async move { Err(StreamError::Json(error)) }).boxed();
            }
        };

        let url = self.chat_completions_url();
        let client = self.http_client();
        let auth_header = self.auth_header.clone();
        let count_tokens = options.count_tokens;

        let (tx, rx) = tokio::sync::mpsc::channel::<StreamResult<StreamEvent>>(100);

        tokio::spawn(async move {
            let mut req_builder = client.post(&url).json(&payload);

            req_builder = apply_auth_to_request(req_builder, &auth_header, credential.as_deref());
            req_builder = req_builder.header("Accept", "text/event-stream");

            let response = match req_builder.send().await {
                Ok(r) => r,
                Err(e) => {
                    let _ = tx.send(Err(StreamError::Http(e.to_string()))).await;
                    return;
                }
            };

            if !response.status().is_success() {
                let status = response.status();
                let error = match response.text().await {
                    Ok(text) => text,
                    Err(_) => format!("HTTP error: {}", status),
                };
                let _ = tx
                    .send(Err(StreamError::Provider(format!("{}: {}", status, error))))
                    .await;
                return;
            }

            let mut event_stream = sse_bytes_to_events(response, count_tokens);
            while let Some(event) = event_stream.next().await {
                if tx.send(event).await.is_err() {
                    break;
                }
            }
        });

        stream::unfold(rx, |mut rx| async move {
            rx.recv().await.map(|event| (event, rx))
        })
        .boxed()
    }

    fn stream_chat_with_system(
        &self,
        system_prompt: Option<&str>,
        message: &str,
        model: &str,
        temperature: Option<f64>,
        options: StreamOptions,
    ) -> stream::BoxStream<'static, StreamResult<StreamChunk>> {
        let temperature = temperature.unwrap_or(self.default_temperature());
        let credential = self.credential.clone();

        let merge = self.effective_merge_system(model);
        let mut messages = Vec::new();
        if merge {
            let content = match system_prompt {
                Some(sys) => format!("{sys}\n\n{message}"),
                None => message.to_string(),
            };
            messages.push(Message {
                role: "user".to_string(),
                content: Self::to_message_content("user", &content, !merge),
            });
        } else {
            if let Some(sys) = system_prompt {
                messages.push(Message {
                    role: "system".to_string(),
                    content: MessageContent::Text(sys.to_string()),
                });
            }
            messages.push(Message {
                role: "user".to_string(),
                content: Self::to_message_content("user", message, !merge),
            });
        }

        let request = ApiChatRequest {
            model: model.to_string(),
            messages,
            temperature,
            stream: Some(options.enabled),
            reasoning_effort: self.reasoning_effort_for_model(model),
            tool_stream: None,
            tools: None,
            tool_choice: None,
            max_tokens: self.max_tokens,
        };

        let url = self.chat_completions_url();
        let client = self.http_client();
        let auth_header = self.auth_header.clone();

        // Use a channel to bridge the async HTTP response to the stream
        let (tx, rx) = tokio::sync::mpsc::channel::<StreamResult<StreamChunk>>(100);

        tokio::spawn(async move {
            // Build request with auth
            let mut req_builder = client.post(&url).json(&request);

            // Apply auth header
            req_builder = apply_auth_to_request(req_builder, &auth_header, credential.as_deref());

            // Set accept header for streaming
            req_builder = req_builder.header("Accept", "text/event-stream");

            // Send request
            let response = match req_builder.send().await {
                Ok(r) => r,
                Err(e) => {
                    let _ = tx.send(Err(StreamError::Http(e.to_string()))).await;
                    return;
                }
            };

            // Check status
            if !response.status().is_success() {
                let status = response.status();
                let error = match response.text().await {
                    Ok(e) => e,
                    Err(_) => format!("HTTP error: {}", status),
                };
                let _ = tx
                    .send(Err(StreamError::Provider(format!("{}: {}", status, error))))
                    .await;
                return;
            }

            // Convert to chunk stream and forward to channel
            let mut chunk_stream = sse_bytes_to_chunks(response, options.count_tokens);
            while let Some(chunk) = chunk_stream.next().await {
                if tx.send(chunk).await.is_err() {
                    break; // Receiver dropped
                }
            }
        });

        // Convert channel receiver to stream
        stream::unfold(rx, |mut rx| async move {
            rx.recv().await.map(|chunk| (chunk, rx))
        })
        .boxed()
    }

    fn stream_chat_with_history(
        &self,
        messages: &[ChatMessage],
        model: &str,
        temperature: Option<f64>,
        options: StreamOptions,
    ) -> stream::BoxStream<'static, StreamResult<StreamChunk>> {
        let temperature = temperature.unwrap_or(self.default_temperature());
        let credential = self.credential.clone();

        let merge = self.effective_merge_system(model);
        let effective_messages = Self::flatten_system_messages(messages, merge);
        let effective_messages = self.strip_native_tool_messages(&effective_messages);
        let api_messages: Vec<Message> = effective_messages
            .iter()
            .map(|m| Message {
                role: m.role.clone(),
                content: Self::to_message_content(&m.role, &m.content, !merge),
            })
            .collect();

        let request = ApiChatRequest {
            model: model.to_string(),
            messages: api_messages,
            temperature,
            stream: Some(options.enabled),
            reasoning_effort: self.reasoning_effort_for_model(model),
            tool_stream: None,
            tools: None,
            tool_choice: None,
            max_tokens: self.max_tokens,
        };

        let url = self.chat_completions_url();
        let client = self.http_client();
        let auth_header = self.auth_header.clone();

        let (tx, rx) = tokio::sync::mpsc::channel::<StreamResult<StreamChunk>>(100);

        tokio::spawn(async move {
            let mut req_builder = client.post(&url).json(&request);
            req_builder = apply_auth_to_request(req_builder, &auth_header, credential.as_deref());
            req_builder = req_builder.header("Accept", "text/event-stream");

            let response = match req_builder.send().await {
                Ok(r) => r,
                Err(e) => {
                    let _ = tx.send(Err(StreamError::Http(e.to_string()))).await;
                    return;
                }
            };

            if !response.status().is_success() {
                let status = response.status();
                let error = match response.text().await {
                    Ok(e) => e,
                    Err(_) => format!("HTTP error: {}", status),
                };
                let _ = tx
                    .send(Err(StreamError::Provider(format!("{}: {}", status, error))))
                    .await;
                return;
            }

            let mut chunk_stream = sse_bytes_to_chunks(response, options.count_tokens);
            while let Some(chunk) = chunk_stream.next().await {
                if tx.send(chunk).await.is_err() {
                    break;
                }
            }
        });

        stream::unfold(rx, |mut rx| async move {
            rx.recv().await.map(|chunk| (chunk, rx))
        })
        .boxed()
    }

    async fn warmup(&self) -> anyhow::Result<()> {
        // Hit the chat completions URL with a GET to establish the connection pool.
        // The server will likely return 405 Method Not Allowed, which is fine -
        // the goal is TLS handshake and HTTP/2 negotiation.
        let url = self.chat_completions_url();
        let _ = self
            .apply_auth_header(self.http_client().get(&url), self.credential.as_deref())
            .send()
            .await?;
        Ok(())
    }
}
