use super::*;

// TODO: Re-enable once WatiConfig/FeishuConfig/LarkReceiveMode/scattered_types/migration
// are available in clawseed-config.
// #[test]
// fn masking_keeps_toml_valid_and_preserves_api_keys_type() {
//     let mut cfg = clawseed_config::schema::Config::default();
//     cfg.providers.fallback = Some("default".into());
//     cfg.providers.models.insert(
//         "default".into(),
//         clawseed_config::schema::ModelProviderConfig {
//             api_key: Some("sk-live-123".to_string()),
//             ..Default::default()
//         },
//     );
//     // Provider fields are now resolved directly — no cache needed.
//     cfg.reliability.api_keys = vec!["rk-1".to_string(), "rk-2".to_string()];
//     cfg.gateway.paired_tokens = vec!["pair-token-1".to_string()];
//     cfg.tunnel.cloudflare = Some(clawseed_config::schema::CloudflareTunnelConfig {
//         token: "cf-token".to_string(),
//     });
//     cfg.memory.qdrant.api_key = Some("qdrant-key".to_string());
//     cfg.channels.wati = Some(clawseed_config::schema::WatiConfig {
//         enabled: true,
//         api_token: "wati-token".to_string(),
//         api_url: "https://live-mt-server.wati.io".to_string(),
//         tenant_id: None,
//         allowed_numbers: vec![],
//         proxy_url: None,
//     });
//     cfg.channels.feishu = Some(clawseed_config::schema::FeishuConfig {
//         enabled: true,
//         app_id: "cli_aabbcc".to_string(),
//         app_secret: "feishu-secret".to_string(),
//         encrypt_key: Some("feishu-encrypt".to_string()),
//         verification_token: Some("feishu-verify".to_string()),
//         allowed_users: vec!["*".to_string()],
//         mention_only: false,
//         receive_mode: clawseed_config::schema::LarkReceiveMode::Websocket,
//         port: None,
//         proxy_url: None,
//     });
//     cfg.channels.email = Some(clawseed_config::scattered_types::EmailConfig {
//         enabled: true,
//         imap_host: "imap.example.com".to_string(),
//         imap_port: 993,
//         imap_folder: "INBOX".to_string(),
//         smtp_host: "smtp.example.com".to_string(),
//         smtp_port: 465,
//         smtp_tls: true,
//         username: "agent@example.com".to_string(),
//         password: "email-password-secret".to_string(),
//         from_address: "agent@example.com".to_string(),
//         idle_timeout_secs: 1740,
//         poll_interval_secs: 60,
//         allowed_senders: vec!["*".to_string()],
//         default_subject: "ClawSeed Message".to_string(),
//         max_attachment_bytes: 25 * 1024 * 1024,
//     });
//     cfg.providers.model_routes = vec![clawseed_config::schema::ModelRouteConfig {
//         hint: "reasoning".to_string(),
//         provider: "openrouter".to_string(),
//         model: "anthropic/claude-sonnet-4.6".to_string(),
//         api_key: Some("route-model-key".to_string()),
//     }];
//     cfg.providers.embedding_routes = vec![clawseed_config::schema::EmbeddingRouteConfig {
//         hint: "semantic".to_string(),
//         provider: "openai".to_string(),
//         model: "text-embedding-3-small".to_string(),
//         dimensions: Some(1536),
//         api_key: Some("route-embed-key".to_string()),
//     }];
//     // Provider fields are now resolved directly — no cache needed.
//
//     let masked = mask_sensitive_fields(&cfg);
//     let toml = toml::to_string_pretty(&masked).expect("masked config should serialize");
//     let parsed: clawseed_config::schema::Config =
//         toml::from_str::<clawseed_config::migration::V1Compat>(&toml)
//             .expect("masked config should remain valid TOML for Config")
//             .into_config();
//
//     assert_eq!(
//         parsed
//             .providers
//             .models
//             .get("default")
//             .and_then(|m| m.api_key.as_deref()),
//         Some(MASKED_SECRET)
//     );
//     assert_eq!(
//         parsed.reliability.api_keys,
//         vec![MASKED_SECRET.to_string(), MASKED_SECRET.to_string()]
//     );
//     assert_eq!(
//         parsed.gateway.paired_tokens,
//         vec![MASKED_SECRET.to_string()]
//     );
//     assert_eq!(
//         parsed.tunnel.cloudflare.as_ref().map(|v| v.token.as_str()),
//         Some(MASKED_SECRET)
//     );
//     assert_eq!(
//         parsed.channels.wati.as_ref().map(|v| v.api_token.as_str()),
//         Some(MASKED_SECRET)
//     );
//     assert_eq!(parsed.memory.qdrant.api_key.as_deref(), Some(MASKED_SECRET));
//     assert_eq!(
//         parsed
//             .channels
//             .feishu
//             .as_ref()
//             .map(|v| v.app_secret.as_str()),
//         Some(MASKED_SECRET)
//     );
//     assert_eq!(
//         parsed
//             .channels
//             .feishu
//             .as_ref()
//             .and_then(|v| v.encrypt_key.as_deref()),
//         Some(MASKED_SECRET)
//     );
//     assert_eq!(
//         parsed
//             .channels
//             .feishu
//             .as_ref()
//             .and_then(|v| v.verification_token.as_deref()),
//         Some(MASKED_SECRET)
//     );
//     assert_eq!(
//         parsed
//             .providers
//             .model_routes
//             .first()
//             .and_then(|v| v.api_key.as_deref()),
//         Some(MASKED_SECRET)
//     );
//     assert_eq!(
//         parsed
//             .providers
//             .embedding_routes
//             .first()
//             .and_then(|v| v.api_key.as_deref()),
//         Some(MASKED_SECRET)
//     );
//     assert_eq!(
//         parsed.channels.email.as_ref().map(|v| v.password.as_str()),
//         Some(MASKED_SECRET)
//     );
// }

// TODO: Re-enable once WatiConfig/FeishuConfig/LarkReceiveMode/scattered_types/migration
// are available in clawseed-config.
// #[test]
// fn hydrate_config_for_save_restores_masked_secrets_and_paths() {
//     let mut current = clawseed_config::schema::Config {
//         config_path: std::path::PathBuf::from("/tmp/current/config.toml"),
//         workspace_dir: std::path::PathBuf::from("/tmp/current/workspace"),
//         ..Default::default()
//     };
//     current.providers.fallback = Some("default".into());
//     current.providers.models.insert(
//         "default".into(),
//         clawseed_config::schema::ModelProviderConfig {
//             api_key: Some("real-key".to_string()),
//             ..Default::default()
//         },
//     );
//     current.reliability.api_keys = vec!["r1".to_string(), "r2".to_string()];
//     current.gateway.paired_tokens = vec!["pair-1".to_string(), "pair-2".to_string()];
//     current.tunnel.cloudflare = Some(clawseed_config::schema::CloudflareTunnelConfig {
//         token: "cf-token-real".to_string(),
//     });
//     current.tunnel.ngrok = Some(clawseed_config::schema::NgrokTunnelConfig {
//         auth_token: "ngrok-token-real".to_string(),
//         domain: None,
//     });
//     current.memory.qdrant.api_key = Some("qdrant-real".to_string());
//     current.channels.wati = Some(clawseed_config::schema::WatiConfig {
//         enabled: true,
//         api_token: "wati-real".to_string(),
//         api_url: "https://live-mt-server.wati.io".to_string(),
//         tenant_id: None,
//         allowed_numbers: vec![],
//         proxy_url: None,
//     });
//     current.channels.feishu = Some(clawseed_config::schema::FeishuConfig {
//         enabled: true,
//         app_id: "cli_current".to_string(),
//         app_secret: "feishu-secret-real".to_string(),
//         encrypt_key: Some("feishu-encrypt-real".to_string()),
//         verification_token: Some("feishu-verify-real".to_string()),
//         allowed_users: vec!["*".to_string()],
//         mention_only: false,
//         receive_mode: clawseed_config::schema::LarkReceiveMode::Websocket,
//         port: None,
//         proxy_url: None,
//     });
//     current.channels.email = Some(clawseed_config::scattered_types::EmailConfig {
//         enabled: true,
//         imap_host: "imap.example.com".to_string(),
//         imap_port: 993,
//         imap_folder: "INBOX".to_string(),
//         smtp_host: "smtp.example.com".to_string(),
//         smtp_port: 465,
//         smtp_tls: true,
//         username: "agent@example.com".to_string(),
//         password: "email-password-real".to_string(),
//         from_address: "agent@example.com".to_string(),
//         idle_timeout_secs: 1740,
//         poll_interval_secs: 60,
//         allowed_senders: vec!["*".to_string()],
//         default_subject: "ClawSeed Message".to_string(),
//         max_attachment_bytes: 25 * 1024 * 1024,
//     });
//     current.providers.model_routes = vec![
//         clawseed_config::schema::ModelRouteConfig {
//             hint: "reasoning".to_string(),
//             provider: "openrouter".to_string(),
//             model: "anthropic/claude-sonnet-4.6".to_string(),
//             api_key: Some("route-model-key-1".to_string()),
//         },
//         clawseed_config::schema::ModelRouteConfig {
//             hint: "fast".to_string(),
//             provider: "openrouter".to_string(),
//             model: "openai/gpt-4.1-mini".to_string(),
//             api_key: Some("route-model-key-2".to_string()),
//         },
//     ];
//     current.providers.embedding_routes = vec![
//         clawseed_config::schema::EmbeddingRouteConfig {
//             hint: "semantic".to_string(),
//             provider: "openai".to_string(),
//             model: "text-embedding-3-small".to_string(),
//             dimensions: Some(1536),
//             api_key: Some("route-embed-key-1".to_string()),
//         },
//         clawseed_config::schema::EmbeddingRouteConfig {
//             hint: "archive".to_string(),
//             provider: "custom:https://emb.example.com/v1".to_string(),
//             model: "bge-m3".to_string(),
//             dimensions: Some(1024),
//             api_key: Some("route-embed-key-2".to_string()),
//         },
//     ];
//
//     let mut incoming = mask_sensitive_fields(&current);
//     if let Some(entry) = incoming.providers.fallback_provider_mut() {
//         entry.model = Some("gpt-4.1-mini".to_string());
//     }
//     // Simulate UI changing only one key and keeping the first masked.
//     incoming.reliability.api_keys = vec![MASKED_SECRET.to_string(), "r2-new".to_string()];
//     incoming.gateway.paired_tokens = vec![MASKED_SECRET.to_string(), "pair-2-new".to_string()];
//     if let Some(cloudflare) = incoming.tunnel.cloudflare.as_mut() {
//         cloudflare.token = MASKED_SECRET.to_string();
//     }
//     if let Some(ngrok) = incoming.tunnel.ngrok.as_mut() {
//         ngrok.auth_token = MASKED_SECRET.to_string();
//     }
//     incoming.memory.qdrant.api_key = Some(MASKED_SECRET.to_string());
//     if let Some(wati) = incoming.channels.wati.as_mut() {
//         wati.api_token = MASKED_SECRET.to_string();
//     }
//     if let Some(feishu) = incoming.channels.feishu.as_mut() {
//         feishu.app_secret = MASKED_SECRET.to_string();
//         feishu.encrypt_key = Some(MASKED_SECRET.to_string());
//         feishu.verification_token = Some("feishu-verify-new".to_string());
//     }
//     if let Some(email) = incoming.channels.email.as_mut() {
//         email.password = MASKED_SECRET.to_string();
//     }
//     incoming.providers.model_routes[1].api_key = Some("route-model-key-2-new".to_string());
//     incoming.providers.embedding_routes[1].api_key = Some("route-embed-key-2-new".to_string());
//
//     let hydrated = hydrate_config_for_save(incoming, &current);
//
//     assert_eq!(hydrated.config_path, current.config_path);
//     assert_eq!(hydrated.workspace_dir, current.workspace_dir);
//     assert_eq!(
//         hydrated
//             .providers
//             .fallback_provider()
//             .and_then(|e| e.api_key.clone()),
//         current
//             .providers
//             .fallback_provider()
//             .and_then(|e| e.api_key.clone())
//     );
//     assert_eq!(
//         hydrated
//             .providers
//             .fallback_provider()
//             .and_then(|e| e.model.as_deref()),
//         Some("gpt-4.1-mini")
//     );
//     assert_eq!(
//         hydrated.reliability.api_keys,
//         vec!["r1".to_string(), "r2-new".to_string()]
//     );
//     assert_eq!(
//         hydrated.gateway.paired_tokens,
//         vec!["pair-1".to_string(), "pair-2-new".to_string()]
//     );
//     assert_eq!(
//         hydrated
//             .tunnel
//             .cloudflare
//             .as_ref()
//             .map(|v| v.token.as_str()),
//         Some("cf-token-real")
//     );
//     assert_eq!(
//         hydrated
//             .tunnel
//             .ngrok
//             .as_ref()
//             .map(|v| v.auth_token.as_str()),
//         Some("ngrok-token-real")
//     );
//     assert_eq!(
//         hydrated.memory.qdrant.api_key.as_deref(),
//         Some("qdrant-real")
//     );
//     assert_eq!(
//         hydrated
//             .channels
//             .wati
//             .as_ref()
//             .map(|v| v.api_token.as_str()),
//         Some("wati-real")
//     );
//     assert_eq!(
//         hydrated
//             .channels
//             .feishu
//             .as_ref()
//             .map(|v| v.app_secret.as_str()),
//         Some("feishu-secret-real")
//     );
//     assert_eq!(
//         hydrated
//             .channels
//             .feishu
//             .as_ref()
//             .and_then(|v| v.encrypt_key.as_deref()),
//         Some("feishu-encrypt-real")
//     );
//     assert_eq!(
//         hydrated
//             .channels
//             .feishu
//             .as_ref()
//             .and_then(|v| v.verification_token.as_deref()),
//         Some("feishu-verify-new")
//     );
//     assert_eq!(
//         hydrated.providers.model_routes[0].api_key.as_deref(),
//         Some("route-model-key-1")
//     );
//     assert_eq!(
//         hydrated.providers.model_routes[1].api_key.as_deref(),
//         Some("route-model-key-2-new")
//     );
//     assert_eq!(
//         hydrated.providers.embedding_routes[0].api_key.as_deref(),
//         Some("route-embed-key-1")
//     );
//     assert_eq!(
//         hydrated.providers.embedding_routes[1].api_key.as_deref(),
//         Some("route-embed-key-2-new")
//     );
//     assert_eq!(
//         hydrated
//             .channels
//             .email
//             .as_ref()
//             .map(|v| v.password.as_str()),
//         Some("email-password-real")
//     );
// }

#[test]
fn hydrate_config_for_save_restores_route_keys_by_identity_and_clears_unmatched_masks() {
    let mut current = clawseed_config::schema::Config::default();
    current.providers.model_routes = vec![
        clawseed_config::schema::ModelRouteConfig {
            hint: "reasoning".to_string(),
            provider: "openrouter".to_string(),
            model: "anthropic/claude-sonnet-4.6".to_string(),
            api_key: Some("route-model-key-1".to_string()),
        },
        clawseed_config::schema::ModelRouteConfig {
            hint: "fast".to_string(),
            provider: "openrouter".to_string(),
            model: "openai/gpt-4.1-mini".to_string(),
            api_key: Some("route-model-key-2".to_string()),
        },
    ];
    current.providers.embedding_routes = vec![
        clawseed_config::schema::EmbeddingRouteConfig {
            hint: "semantic".to_string(),
            provider: "openai".to_string(),
            model: "text-embedding-3-small".to_string(),
            dimensions: Some(1536),
            api_key: Some("route-embed-key-1".to_string()),
        },
        clawseed_config::schema::EmbeddingRouteConfig {
            hint: "archive".to_string(),
            provider: "custom:https://emb.example.com/v1".to_string(),
            model: "bge-m3".to_string(),
            dimensions: Some(1024),
            api_key: Some("route-embed-key-2".to_string()),
        },
    ];

    let mut incoming = mask_sensitive_fields(&current);
    incoming.providers.model_routes.swap(0, 1);
    incoming.providers.embedding_routes.swap(0, 1);
    incoming
        .providers
        .model_routes
        .push(clawseed_config::schema::ModelRouteConfig {
            hint: "new".to_string(),
            provider: "openai".to_string(),
            model: "gpt-4.1".to_string(),
            api_key: Some(MASKED_SECRET.to_string()),
        });
    incoming
        .providers
        .embedding_routes
        .push(clawseed_config::schema::EmbeddingRouteConfig {
            hint: "new-embed".to_string(),
            provider: "custom:https://emb2.example.com/v1".to_string(),
            model: "bge-small".to_string(),
            dimensions: Some(768),
            api_key: Some(MASKED_SECRET.to_string()),
        });

    let hydrated = hydrate_config_for_save(incoming, &current);

    assert_eq!(
        hydrated.providers.model_routes[0].api_key.as_deref(),
        Some("route-model-key-2")
    );
    assert_eq!(
        hydrated.providers.model_routes[1].api_key.as_deref(),
        Some("route-model-key-1")
    );
    assert_eq!(hydrated.providers.model_routes[2].api_key, None);
    assert_eq!(
        hydrated.providers.embedding_routes[0].api_key.as_deref(),
        Some("route-embed-key-2")
    );
    assert_eq!(
        hydrated.providers.embedding_routes[1].api_key.as_deref(),
        Some("route-embed-key-1")
    );
    assert_eq!(hydrated.providers.embedding_routes[2].api_key, None);
    assert!(
        hydrated
            .providers
            .model_routes
            .iter()
            .all(|route| route.api_key.as_deref() != Some(MASKED_SECRET))
    );
    assert!(
        hydrated
            .providers
            .embedding_routes
            .iter()
            .all(|route| route.api_key.as_deref() != Some(MASKED_SECRET))
    );
}
