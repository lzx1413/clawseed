use super::*;
use crate::{AppState, GatewayRateLimiter, IdempotencyStore};
use async_trait::async_trait;
use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Json},
};
use clawseed_agent::security::pairing::PairingGuard;
use clawseed_api::memory_traits::{Memory, MemoryCategory, MemoryEntry, SearchMode};
use clawseed_api::provider::Provider;
use clawseed_api::user_profile::{
    ProfileCategory, ProfileImportStrategy, ProfileItemInput, ProfileSource, ProfileStatus,
};
use http_body_util::BodyExt;
use parking_lot::Mutex;
use std::sync::Arc;
use std::time::Duration;

mod config;
mod cron;
mod identity;
mod profile;

struct MockMemory;

#[async_trait]
impl Memory for MockMemory {
    fn name(&self) -> &str {
        "mock"
    }

    async fn store(
        &self,
        _key: &str,
        _content: &str,
        _category: MemoryCategory,
        _session_id: Option<&str>,
    ) -> anyhow::Result<()> {
        Ok(())
    }

    async fn recall(
        &self,
        _query: &str,
        _limit: usize,
        _session_id: Option<&str>,
        _since: Option<&str>,
        _until: Option<&str>,
        _search_mode: Option<SearchMode>,
    ) -> anyhow::Result<Vec<MemoryEntry>> {
        Ok(Vec::new())
    }

    async fn get(&self, _key: &str) -> anyhow::Result<Option<MemoryEntry>> {
        Ok(None)
    }

    async fn list(
        &self,
        _category: Option<&MemoryCategory>,
        _session_id: Option<&str>,
    ) -> anyhow::Result<Vec<MemoryEntry>> {
        Ok(Vec::new())
    }

    async fn forget(&self, _key: &str) -> anyhow::Result<bool> {
        Ok(false)
    }

    async fn count(&self) -> anyhow::Result<usize> {
        Ok(0)
    }

    async fn health_check(&self) -> bool {
        true
    }
}

struct MockProvider;

#[async_trait]
impl Provider for MockProvider {
    async fn chat_with_system(
        &self,
        _system_prompt: Option<&str>,
        _message: &str,
        _model: &str,
        _temperature: Option<f64>,
    ) -> anyhow::Result<String> {
        Ok("ok".to_string())
    }
}

fn test_state(config: clawseed_config::schema::Config) -> AppState {
    AppState {
        config: Arc::new(Mutex::new(config)),
        provider: Arc::new(MockProvider),
        model: "test-model".into(),
        temperature: 0.0,
        mem: Arc::new(MockMemory),
        user_profile_store: None,
        auto_save: false,
        webhook_secret_hash: None,
        pairing: Arc::new(PairingGuard::new(false, &[])),
        trust_forwarded_headers: false,
        rate_limiter: Arc::new(GatewayRateLimiter::new(100, 100, 100)),
        auth_limiter: Arc::new(crate::auth_rate_limit::AuthRateLimiter::new()),
        idempotency_store: Arc::new(IdempotencyStore::new(Duration::from_secs(300), 1000)),
        observer: Arc::new(clawseed_agent::observability::NoopObserver),
        tool_registry: Arc::new(clawseed_agent::tool_registry::DefaultToolRegistry::new()),
        shared_builtin_tools: Arc::new([]),
        skill_index: Arc::new(parking_lot::RwLock::new(Vec::new())),
        skills_excluded: Arc::new(std::sync::Mutex::new(Vec::new())),
        cost_tracker: None,
        event_tx: tokio::sync::broadcast::channel(16).0,
        event_buffer: Arc::new(crate::EventBuffer::new(16)),
        shutdown_tx: tokio::sync::watch::channel(false).0,
        node_registry: Arc::new(crate::NodeRegistry::new(16)),
        session_backend: None,
        session_queue: Arc::new(crate::session_queue::SessionActorQueue::new(8, 30, 600)),
        path_prefix: String::new(),
        web_dist_dir: None,
        canvas_store: clawseed_agent::tools::CanvasStore::new(),
        cancel_tokens: Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
    }
}

async fn response_json(response: axum::response::Response) -> serde_json::Value {
    let body = response
        .into_body()
        .collect()
        .await
        .expect("response body")
        .to_bytes();
    serde_json::from_slice(&body).expect("valid json response")
}
