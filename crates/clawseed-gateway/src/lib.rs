//! Axum-based HTTP gateway with proper HTTP/1.1 compliance, body limits, and timeouts.
//!
//! This module replaces the raw TCP implementation with axum for:
//! - Proper HTTP/1.1 parsing and compliance
//! - Content-Length validation (handled by hyper)
//! - Request body size limits (64KB max)
//! - Request timeouts (30s) to prevent slow-loris attacks
//! - Header sanitization (handled by axum/hyper)

pub mod api;
pub mod auth_rate_limit;
pub mod handlers;
pub mod ratelimit;
pub mod remote_tool;
pub mod session_backend;
pub mod session_queue;
pub mod session_sqlite;
pub mod static_files;
pub mod tls;
pub mod ws;

use crate::handlers::{
    handle_admin_paircode, handle_admin_paircode_new, handle_admin_shutdown, handle_health,
    handle_metrics, handle_pair, handle_pair_code, handle_webhook, hash_webhook_secret,
};
use crate::ratelimit::RATE_LIMIT_MAX_KEYS_DEFAULT;
use crate::ratelimit::IDEMPOTENCY_MAX_KEYS_DEFAULT;
use crate::ratelimit::dirs_data_local;
use crate::ratelimit::normalize_max_keys;
use crate::session_backend::SessionBackend;
use crate::session_sqlite::SqliteSessionBackend;
use anyhow::Result;
use axum::{
    Router,
    http::StatusCode,
    routing::{delete, get, post, put},
};
use clawseed_agent::cost::CostTracker;
use clawseed_agent::security::SecurityPolicy;
use clawseed_agent::security::pairing::{PairingGuard, is_public_bind};
use clawseed_agent::tools;
use clawseed_agent::tools::CanvasStore;
use clawseed_api::memory_traits::Memory;
use clawseed_api::provider::Provider;

use clawseed_config::schema::Config;
use parking_lot::Mutex;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::timeout::TimeoutLayer;

/// Minimal event buffer stub (replaces removed `sse::EventBuffer`).
pub struct EventBuffer {
    _capacity: usize,
}
impl EventBuffer {
    pub fn new(cap: usize) -> Self {
        Self { _capacity: cap }
    }
}

/// Minimal node registry stub (replaces removed `nodes::NodeRegistry`).
pub struct NodeRegistry {
    _max: usize,
}
impl NodeRegistry {
    pub fn new(max: usize) -> Self {
        Self { _max: max }
    }
}

/// Maximum request body size (64KB) — prevents memory exhaustion
pub const MAX_BODY_SIZE: usize = 65_536;
/// Default request timeout (30s) — prevents slow-loris attacks.
pub const REQUEST_TIMEOUT_SECS: u64 = 30;

/// Read gateway request timeout from `CLAWSEED_GATEWAY_TIMEOUT_SECS` env var
/// at runtime, falling back to [`REQUEST_TIMEOUT_SECS`].
///
/// Agentic workloads with tool use (web search, MCP tools, sub-agent
/// delegation) regularly exceed 30 seconds. This allows operators to
/// increase the timeout without recompiling.
pub fn gateway_request_timeout_secs() -> u64 {
    std::env::var("CLAWSEED_GATEWAY_TIMEOUT_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(REQUEST_TIMEOUT_SECS)
}

// Re-export from submodules for backward compatibility
pub use ratelimit::GatewayRateLimiter;
pub use ratelimit::IdempotencyStore;

/// Shared state for all axum handlers
#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Mutex<Config>>,
    pub provider: Arc<dyn Provider>,
    pub model: String,
    pub temperature: f64,
    pub mem: Arc<dyn Memory>,
    pub auto_save: bool,
    /// SHA-256 hash of `X-Webhook-Secret` (hex-encoded), never plaintext.
    pub webhook_secret_hash: Option<Arc<str>>,
    pub pairing: Arc<PairingGuard>,
    pub trust_forwarded_headers: bool,
    pub rate_limiter: Arc<GatewayRateLimiter>,
    pub auth_limiter: Arc<auth_rate_limit::AuthRateLimiter>,
    pub idempotency_store: Arc<IdempotencyStore>,
    /// Observability backend for metrics scraping
    pub observer: Arc<dyn clawseed_agent::observability::Observer>,
    /// Registered tool registry (for web dashboard tools page and agent tool dispatch)
    pub tool_registry: Arc<dyn clawseed_api::tool_registry::ToolRegistry>,
    /// Cost tracker (optional, for web dashboard cost page)
    pub cost_tracker: Option<Arc<CostTracker>>,
    /// SSE broadcast channel for real-time events
    pub event_tx: tokio::sync::broadcast::Sender<serde_json::Value>,
    /// Ring buffer of recent events for history replay
    pub event_buffer: Arc<EventBuffer>,
    /// Shutdown signal sender for graceful shutdown
    pub shutdown_tx: tokio::sync::watch::Sender<bool>,
    /// Registry of dynamically connected nodes
    pub node_registry: Arc<NodeRegistry>,
    /// Path prefix for reverse-proxy deployments (empty string = no prefix)
    pub path_prefix: String,
    /// Filesystem path to `web/dist/` for serving the dashboard (None = API-only)
    pub web_dist_dir: Option<std::path::PathBuf>,
    /// Session backend for persisting gateway WS chat sessions
    pub session_backend: Option<Arc<dyn SessionBackend>>,
    /// Per-session actor queue for serializing concurrent turns
    pub session_queue: Arc<session_queue::SessionActorQueue>,
    /// Shared canvas store for Live Canvas (A2UI) system
    pub canvas_store: CanvasStore,
    /// Per-session cancellation tokens for aborting in-flight agent responses.
    /// Key is session_key (e.g. `gw_<session_id>`), value is the token for the
    /// current turn. Entries are inserted before each turn and removed after
    /// completion (normal or cancelled).
    pub cancel_tokens: Arc<
        std::sync::Mutex<std::collections::HashMap<String, tokio_util::sync::CancellationToken>>,
    >,
}

/// Run the HTTP gateway using axum with proper HTTP/1.1 compliance.
#[allow(clippy::too_many_lines)]
pub async fn run_gateway(
    host: &str,
    port: u16,
    config: Config,
    external_event_tx: Option<tokio::sync::broadcast::Sender<serde_json::Value>>,
) -> Result<()> {
    // ── Security: warn on public bind without tunnel or explicit opt-in ──
    if is_public_bind(host) && config.tunnel.provider == "none" && !config.gateway.allow_public_bind
    {
        tracing::warn!(
            "Binding to {host} — gateway will be exposed to all network interfaces.\n\
             Suggestion: use --host 127.0.0.1 (default), configure a tunnel, or set\n\
             [gateway] allow_public_bind = true in config.toml to silence this warning.\n\n\
             Docker/VM: if you are running inside a container or VM, this is expected."
        );
    }
    let config_state = Arc::new(Mutex::new(config.clone()));

    // ── Hooks ──────────────────────────────────────────────────────
    let hooks: Option<std::sync::Arc<clawseed_agent::hooks::HookRunner>> = if config.hooks.enabled {
        Some(std::sync::Arc::new(clawseed_agent::hooks::HookRunner::new()))
    } else {
        None
    };

    let addr: SocketAddr = format!("{host}:{port}").parse()?;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    let actual_port = listener.local_addr()?.port();
    let display_addr = format!("{host}:{actual_port}");

    let fallback = config.providers.fallback_provider();
    let provider: Arc<dyn Provider> =
        Arc::from(clawseed_providers::create_resilient_provider_with_options(
            config.providers.fallback.as_deref().unwrap_or("openrouter"),
            fallback.and_then(|e| e.api_key.as_deref()),
            fallback.and_then(|e| e.base_url.as_deref()),
            &config.reliability,
            &clawseed_providers::provider_runtime_options_from_config(&config),
        )?);
    let model = fallback
        .and_then(|e| e.model.clone())
        .unwrap_or_else(|| "anthropic/claude-sonnet-4".into());
    let temperature = fallback.and_then(|e| e.temperature).unwrap_or(0.7);
    let mem: Arc<dyn Memory> = clawseed_memory::create_memory_with_storage_and_routes(
        &config.memory,
        &config.providers,
        Some(&config.storage),
        &config.workspace_dir,
        fallback.and_then(|e| e.api_key.as_deref()),
    )?;
    let runtime: Box<dyn std::any::Any> = Box::new(());
    let security = Arc::new(SecurityPolicy::from_config(
        &config.autonomy,
        &config.workspace_dir,
    ));

    let (composio_key, composio_entity_id) = if config.composio.enabled {
        (
            config.composio.api_key.as_deref(),
            Some(config.composio.entity_id.as_str()),
        )
    } else {
        (None, None)
    };

    let canvas_store = tools::CanvasStore::new();

    let (
        mut tools_registry_raw,
        delegate_handle_gw,
        _reaction_handle_gw,
        _channel_map_handle,
        _ask_user_handle_gw,
        _escalate_handle_gw,
    ) = tools::all_tools_with_runtime(
        Arc::new(config.clone()),
        &security,
        runtime,
        Arc::clone(&mem),
        composio_key,
        composio_entity_id,
        &config,
        &config,
        &config,
        &config.workspace_dir,
        &config,
        config
            .providers
            .fallback_provider()
            .and_then(|e| e.api_key.as_deref()),
        &config,
        Some(canvas_store.clone()),
    );

    // ── Wire MCP tools into the gateway tool registry (non-fatal) ───
    // Without this, the `/api/tools` endpoint misses MCP tools.
    if config.mcp.enabled && !config.mcp.servers.is_empty() {
        tracing::info!(
            "Gateway: initializing MCP client — {} server(s) configured",
            config.mcp.servers.len()
        );
        match tools::McpRegistry::connect_all(&config.mcp.servers).await {
            Ok(registry) => {
                let registry = std::sync::Arc::new(registry);
                if config.mcp.deferred_loading {
                    let deferred_set =
                        tools::DeferredMcpToolSet::from_registry(std::sync::Arc::clone(&registry))
                            .await;
                    tracing::info!(
                        "Gateway MCP deferred: {} tool stub(s) from {} server(s)",
                        deferred_set.len(),
                        registry.server_count()
                    );
                    let activated =
                        std::sync::Arc::new(std::sync::Mutex::new(tools::ActivatedToolSet::new()));
                    tools_registry_raw.push(Box::new(tools::ToolSearchTool::new(
                        deferred_set,
                        activated,
                    )));
                } else {
                    let names = registry.tool_names();
                    let mut registered = 0usize;
                    for name in names {
                        if let Some(def) = registry.get_tool_def(&name).await {
                            let wrapper: std::sync::Arc<dyn tools::DynTool> =
                                std::sync::Arc::new(tools::McpToolWrapper::new(
                                    name,
                                    def,
                                    std::sync::Arc::clone(&registry),
                                ));
                            if let Some(ref handle) = delegate_handle_gw {
                                handle.write().push(std::sync::Arc::clone(&wrapper));
                            }
                            tools_registry_raw.push(Box::new(tools::ArcToolRef(wrapper)));
                            registered += 1;
                        }
                    }
                    tracing::info!(
                        "Gateway MCP: {} tool(s) registered from {} server(s)",
                        registered,
                        registry.server_count()
                    );
                }
            }
            Err(e) => {
                tracing::error!("Gateway MCP registry failed to initialize: {e:#}");
            }
        }
    }

    // Build the shared tool registry from all sources
    let shared_tool_registry: Arc<dyn clawseed_api::tool_registry::ToolRegistry> = {
        let reg = clawseed_agent::tool_registry::DefaultToolRegistry::new();
        reg.register_all(
            tools_registry_raw,
            clawseed_api::tool_registry::ToolSource::BuiltIn,
        );
        Arc::new(reg)
    };

    // Cost tracker — process-global singleton so channels share the same instance
    let cost_tracker = Some(CostTracker::get_or_init_global(
        config.cost.clone(),
        &config.workspace_dir,
    ));

    // SSE broadcast channel for real-time events.
    // Use an externally provided sender (e.g. from the daemon) so that other
    // components (cron, heartbeat) can publish events to the same bus.
    let event_tx = external_event_tx.unwrap_or_else(|| {
        let (tx, _rx) = tokio::sync::broadcast::channel::<serde_json::Value>(256);
        tx
    });
    let event_buffer = Arc::new(EventBuffer::new(500));
    // Extract webhook secret for authentication
    let webhook_secret_hash: Option<Arc<str>> =
        config.channels.webhook.as_ref().and_then(|webhook| {
            webhook.secret.as_ref().and_then(|raw_secret| {
                let trimmed_secret = raw_secret.trim();
                (!trimmed_secret.is_empty())
                    .then(|| Arc::<str>::from(hash_webhook_secret(trimmed_secret)))
            })
        });

    // ── Session persistence for WS chat ─────────────────────
    let session_backend: Option<Arc<dyn SessionBackend>> = if config.gateway.session_persistence {
        match SqliteSessionBackend::new(&config.workspace_dir) {
            Ok(b) => {
                tracing::info!("Gateway session persistence enabled (SQLite)");
                if config.gateway.session_ttl_hours > 0
                    && let Ok(cleaned) = b.cleanup_stale(config.gateway.session_ttl_hours)
                    && cleaned > 0
                {
                    tracing::info!("Cleaned up {cleaned} stale gateway sessions");
                }
                Some(Arc::new(b))
            }
            Err(e) => {
                tracing::warn!("Session persistence disabled: {e}");
                None
            }
        }
    } else {
        None
    };

    // ── Pairing guard ──────────────────────────────────────
    let pairing = Arc::new(PairingGuard::new(
        config.gateway.require_pairing,
        &config.gateway.paired_tokens,
    ));
    let rate_limit_max_keys = normalize_max_keys(
        config.gateway.rate_limit_max_keys,
        RATE_LIMIT_MAX_KEYS_DEFAULT,
    );
    let rate_limiter = Arc::new(GatewayRateLimiter::new(
        config.gateway.pair_rate_limit_per_minute,
        config.gateway.webhook_rate_limit_per_minute,
        rate_limit_max_keys,
    ));
    let idempotency_max_keys = normalize_max_keys(
        config.gateway.idempotency_max_keys,
        IDEMPOTENCY_MAX_KEYS_DEFAULT,
    );
    let idempotency_store = Arc::new(IdempotencyStore::new(
        Duration::from_secs(config.gateway.idempotency_ttl_secs.max(1)),
        idempotency_max_keys,
    ));

    // Resolve optional path prefix for reverse-proxy deployments.
    let path_prefix: Option<&str> = config
        .gateway
        .path_prefix
        .as_deref()
        .filter(|p| !p.is_empty());

    // ── Tunnel ────────────────────────────────────────────────
    let tunnel_url: Option<String> = None;

    // Resolve web_dist_dir: explicit config → auto-detect common locations
    let web_dist_dir: Option<std::path::PathBuf> = config
        .gateway
        .web_dist_dir
        .as_ref()
        .map(std::path::PathBuf::from)
        .or_else(|| {
            // Auto-detect: check common locations relative to the binary and CWD
            let mut candidates = vec![
                // Relative to CWD (development: running from repo root)
                std::path::PathBuf::from("web/dist"),
                // Relative to binary (installed alongside binary)
                std::env::current_exe()
                    .ok()
                    .and_then(|p| p.parent().map(|d| d.join("web/dist")))
                    .unwrap_or_default(),
                // Docker / packaged layout
                std::path::PathBuf::from("/clawseed-data/web/dist"),
                // AUR / system package
                std::path::PathBuf::from("/usr/share/clawseed/web/dist"),
            ];
            // XDG data home (prebuilt binary installer)
            if let Some(data_dir) = dirs_data_local() {
                candidates.push(data_dir.join("clawseed/web/dist"));
            }
            candidates
                .into_iter()
                .find(|p| !p.as_os_str().is_empty() && p.join("index.html").is_file())
        });

    if let Some(ref dir) = web_dist_dir {
        tracing::info!("Web dashboard: serving from {}", dir.display());
    } else {
        tracing::info!(
            "Web dashboard: not available (set gateway.web_dist_dir or CLAWSEED_WEB_DIST_DIR)"
        );
    }

    let pfx = path_prefix.unwrap_or("");
    println!("ClawSeed Gateway listening on http://{display_addr}{pfx}");
    if let Some(ref url) = tunnel_url {
        println!("  Public URL: {url}");
    }
    println!("  Web Dashboard: http://{display_addr}{pfx}/");
    if let Some(code) = pairing.pairing_code() {
        println!();
        println!("  PAIRING REQUIRED — use this one-time code:");
        println!("     +--------------+");
        println!("     |  {code}  |");
        println!("     +--------------+");
        println!("     Send: POST {pfx}/pair with header X-Pairing-Code: {code}");
    } else if pairing.require_pairing() {
        println!("  Pairing: ACTIVE (bearer token required)");
        println!("     To pair a new device: clawseed gateway get-paircode --new");
        println!();
    } else {
        println!("  Pairing: DISABLED (all requests accepted)");
        println!();
    }
    println!("  POST {pfx}/pair      — pair a new client (X-Pairing-Code header)");
    println!("  POST {pfx}/webhook   — {{\"message\": \"your prompt\"}}");
    println!("  GET  {pfx}/api/*     — REST API (bearer token required)");
    println!("  GET  {pfx}/ws/chat   — WebSocket agent chat");
    if config.nodes.enabled {
        println!("  GET  {pfx}/ws/nodes  — WebSocket node discovery");
    }
    println!("  GET  {pfx}/health    — health check");
    println!("  GET  {pfx}/metrics   — Prometheus metrics");
    println!("  Press Ctrl+C to stop.\n");

    // Gateway is ready.

    // Fire gateway start hook
    if let Some(ref hooks) = hooks {
        hooks.fire_gateway_start(host, actual_port).await;
    }

    // Wrap observer with broadcast capability for SSE
    let broadcast_observer: Arc<dyn clawseed_agent::observability::Observer> =
        Arc::new(clawseed_agent::observability::NoopObserver);

    let (shutdown_tx, mut shutdown_rx) = tokio::sync::watch::channel(false);

    let node_registry = Arc::new(NodeRegistry::new(config.nodes.max_nodes));

    let state = AppState {
        config: config_state,
        provider,
        model,
        temperature,
        mem,
        auto_save: config.memory.auto_save,
        webhook_secret_hash,
        pairing,
        trust_forwarded_headers: config.gateway.trust_forwarded_headers,
        rate_limiter,
        auth_limiter: Arc::new(auth_rate_limit::AuthRateLimiter::new()),
        idempotency_store,
        observer: broadcast_observer,
        tool_registry: shared_tool_registry,
        cost_tracker,
        event_tx,
        event_buffer,
        shutdown_tx,
        node_registry,
        session_backend,
        session_queue: Arc::new(session_queue::SessionActorQueue::new(8, 30, 600)),
        path_prefix: path_prefix.unwrap_or("").to_string(),
        web_dist_dir,
        canvas_store,
        cancel_tokens: Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
    };

    // Config PUT needs larger body limit (1MB)
    let config_put_router = Router::new()
        .route("/api/config", put(api::handle_api_config_put))
        .layer(RequestBodyLimitLayer::new(1_048_576));

    // Build router with middleware
    let inner = Router::new()
        // ── Admin routes (for CLI management) ──
        .route("/admin/shutdown", post(handle_admin_shutdown))
        .route("/admin/paircode", get(handle_admin_paircode))
        .route("/admin/paircode/new", post(handle_admin_paircode_new))
        // ── Existing routes ──
        .route("/health", get(handle_health))
        .route("/metrics", get(handle_metrics))
        .route("/pair", post(handle_pair))
        .route("/pair/code", get(handle_pair_code))
        .route("/webhook", post(handle_webhook))
        // ── Claude Code runner hooks ──
        .route("/hooks/claude-code", post(api::handle_claude_code_hook))
        // ── Web Dashboard API routes ──
        .route("/api/status", get(api::handle_api_status))
        .route("/api/config", get(api::handle_api_config_get))
        .route("/api/tools", get(api::handle_api_tools))
        .route("/api/provider/models", get(api::handle_api_provider_models))
        .route("/api/cron", get(api::handle_api_cron_list))
        .route("/api/cron", post(api::handle_api_cron_add))
        .route(
            "/api/cron/settings",
            get(api::handle_api_cron_settings_get).patch(api::handle_api_cron_settings_patch),
        )
        .route(
            "/api/cron/{id}",
            delete(api::handle_api_cron_delete).patch(api::handle_api_cron_patch),
        )
        .route("/api/cron/{id}/runs", get(api::handle_api_cron_runs))
        .route("/api/integrations", get(api::handle_api_integrations))
        .route(
            "/api/integrations/settings",
            get(api::handle_api_integrations_settings),
        )
        .route(
            "/api/doctor",
            get(api::handle_api_doctor).post(api::handle_api_doctor),
        )
        .route("/api/memory", get(api::handle_api_memory_list))
        .route("/api/memory", post(api::handle_api_memory_store))
        .route("/api/memory/{key}", delete(api::handle_api_memory_delete))
        .route("/api/cost", get(api::handle_api_cost))
        .route("/api/cli-tools", get(api::handle_api_cli_tools))
        .route("/api/channels", get(api::handle_api_channels))
        .route("/api/health", get(api::handle_api_health))
        .route("/api/sessions", get(api::handle_api_sessions_list))
        .route(
            "/api/sessions/running",
            get(api::handle_api_sessions_running),
        )
        .route(
            "/api/sessions/{id}/messages",
            get(api::handle_api_session_messages),
        )
        .route(
            "/api/sessions/{id}",
            delete(api::handle_api_session_delete).put(api::handle_api_session_rename),
        )
        .route(
            "/api/sessions/{id}/state",
            get(api::handle_api_session_state),
        )
        .route(
            "/api/sessions/{id}/abort",
            post(api::handle_api_session_abort),
        );

    let inner = inner
        // ── WebSocket agent chat ──
        .route("/ws/chat", get(ws::handle_ws_chat))
        // ── Static assets (web dashboard) ──
        .route("/_app/{*path}", get(static_files::handle_static))
        // ── Config PUT with larger body limit ──
        .merge(config_put_router)
        // ── SPA fallback: non-API GET requests serve index.html ──
        .fallback(get(static_files::handle_spa_fallback))
        .with_state(state)
        .layer(RequestBodyLimitLayer::new(MAX_BODY_SIZE))
        .layer(TimeoutLayer::with_status_code(
            StatusCode::REQUEST_TIMEOUT,
            Duration::from_secs(gateway_request_timeout_secs()),
        ));

    // Nest under path prefix when configured (axum strips prefix before routing).
    // nest() at "/prefix" handles both "/prefix" and "/prefix/*" but not "/prefix/"
    // with a trailing slash, so we add a fallback redirect for that case.
    let app = if let Some(prefix) = path_prefix {
        let redirect_target = prefix.to_string();
        Router::new().nest(prefix, inner).route(
            &format!("{prefix}/"),
            get(|| async move { axum::response::Redirect::permanent(&redirect_target) }),
        )
    } else {
        inner
    };

    // ── TLS / mTLS setup ───────────────────────────────────────────
    let tls_acceptor = match &config.gateway.tls {
        Some(tls_cfg) => {
            let has_mtls = tls_cfg.client_auth.as_ref().is_some_and(|ca| ca.enabled);
            if has_mtls {
                tracing::info!("TLS enabled with mutual TLS (mTLS) client verification");
            } else {
                tracing::info!("TLS enabled (no client certificate requirement)");
            }
            Some(tls::build_tls_acceptor(tls_cfg)?)
        }
        _ => None,
    };

    if let Some(tls_acceptor) = tls_acceptor {
        // Manual TLS accept loop — serves each connection via hyper.
        let app = app.into_make_service_with_connect_info::<SocketAddr>();
        let mut app = app;

        let mut shutdown_signal = shutdown_rx;
        loop {
            tokio::select! {
                conn = listener.accept() => {
                    let (tcp_stream, remote_addr) = conn?;
                    let tls_acceptor = tls_acceptor.clone();
                    let svc = tower::MakeService::<
                        SocketAddr,
                        hyper::Request<hyper::body::Incoming>,
                    >::make_service(&mut app, remote_addr)
                    .await
                    .expect("infallible make_service");

                    tokio::spawn(async move {
                        let tls_stream = match tls_acceptor.accept(tcp_stream).await {
                            Ok(s) => s,
                            Err(e) => {
                                tracing::debug!("TLS handshake failed from {remote_addr}: {e}");
                                return;
                            }
                        };
                        let io = hyper_util::rt::TokioIo::new(tls_stream);
                        let hyper_svc = hyper::service::service_fn(move |req: hyper::Request<hyper::body::Incoming>| {
                            let mut svc = svc.clone();
                            async move {
                                tower::Service::call(&mut svc, req).await
                            }
                        });
                        if let Err(e) = hyper_util::server::conn::auto::Builder::new(
                            hyper_util::rt::TokioExecutor::new(),
                        )
                        .serve_connection(io, hyper_svc)
                        .await
                        {
                            tracing::debug!("connection error from {remote_addr}: {e}");
                        }
                    });
                }
                _ = shutdown_signal.changed() => {
                    tracing::info!("ClawSeed Gateway shutting down...");
                    break;
                }
            }
        }
    } else {
        // Plain TCP — use axum's built-in serve.
        axum::serve(
            listener,
            app.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .with_graceful_shutdown(async move {
            let _ = shutdown_rx.changed().await;
            tracing::info!("ClawSeed Gateway shutting down...");
        })
        .await?;
    }

    Ok(())
}
