//! Shared mock provider implementations for integration tests.

#![allow(dead_code)]

use std::sync::Mutex;

use async_trait::async_trait;
use clawseed_api::provider::{ChatMessage, ChatRequest, ChatResponse, Provider};

/// Mock provider that returns scripted responses in FIFO order.
pub struct MockProvider {
    responses: Mutex<Vec<ChatResponse>>,
}

impl MockProvider {
    pub fn new(responses: Vec<ChatResponse>) -> Self {
        Self {
            responses: Mutex::new(responses),
        }
    }
}

#[async_trait]
impl Provider for MockProvider {
    async fn chat_with_system(
        &self,
        _system_prompt: Option<&str>,
        _message: &str,
        _model: &str,
        _temperature: Option<f64>,
    ) -> anyhow::Result<String> {
        let mut guard = self.responses.lock().unwrap();
        if guard.is_empty() {
            return Ok("fallback".into());
        }
        let resp = guard.remove(0);
        Ok(resp.text.unwrap_or_else(|| "fallback".into()))
    }

    async fn chat(
        &self,
        _request: ChatRequest<'_>,
        _model: &str,
        _temperature: Option<f64>,
    ) -> anyhow::Result<ChatResponse> {
        let mut guard = self.responses.lock().unwrap();
        if guard.is_empty() {
            return Ok(ChatResponse {
                text: Some("done".into()),
                tool_calls: vec![],
                usage: None,
                reasoning_content: None,
            });
        }
        Ok(guard.remove(0))
    }
}

/// Mock provider that returns scripted responses AND records every request.
pub struct RecordingProvider {
    responses: Mutex<Vec<ChatResponse>>,
    recorded_requests: std::sync::Arc<Mutex<Vec<Vec<ChatMessage>>>>,
}

impl RecordingProvider {
    pub fn new(
        responses: Vec<ChatResponse>,
    ) -> (Self, std::sync::Arc<Mutex<Vec<Vec<ChatMessage>>>>) {
        let recorded = std::sync::Arc::new(Mutex::new(Vec::new()));
        let provider = Self {
            responses: Mutex::new(responses),
            recorded_requests: recorded.clone(),
        };
        (provider, recorded)
    }
}

#[async_trait]
impl Provider for RecordingProvider {
    async fn chat_with_system(
        &self,
        _system_prompt: Option<&str>,
        _message: &str,
        _model: &str,
        _temperature: Option<f64>,
    ) -> anyhow::Result<String> {
        Ok("fallback".into())
    }

    async fn chat(
        &self,
        request: ChatRequest<'_>,
        _model: &str,
        _temperature: Option<f64>,
    ) -> anyhow::Result<ChatResponse> {
        self.recorded_requests
            .lock()
            .unwrap()
            .push(request.messages.to_vec());

        let mut guard = self.responses.lock().unwrap();
        if guard.is_empty() {
            return Ok(ChatResponse {
                text: Some("done".into()),
                tool_calls: vec![],
                usage: None,
                reasoning_content: None,
            });
        }
        Ok(guard.remove(0))
    }
}
