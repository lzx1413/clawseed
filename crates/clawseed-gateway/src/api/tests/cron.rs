use super::*;

#[tokio::test]
async fn cron_api_shell_roundtrip_includes_delivery() {
    let tmp = tempfile::TempDir::new().unwrap();
    let config = clawseed_config::schema::Config {
        workspace_dir: tmp.path().join("workspace"),
        config_path: tmp.path().join("config.toml"),
        ..clawseed_config::schema::Config::default()
    };
    std::fs::create_dir_all(&config.workspace_dir).unwrap();
    let state = test_state(config);

    let add_response = handle_api_cron_add(
        State(state.clone()),
        HeaderMap::new(),
        Json(
            serde_json::from_value::<CronAddBody>(serde_json::json!({
                "name": "test-job",
                "schedule": "*/5 * * * *",
                "command": "echo hello",
                "delivery": {
                    "mode": "announce",
                    "channel": "discord",
                    "to": "1234567890",
                    "best_effort": true
                }
            }))
            .expect("body should deserialize"),
        ),
    )
    .await
    .into_response();

    let add_json = response_json(add_response).await;
    assert_eq!(add_json["status"], "ok");
    assert_eq!(add_json["job"]["delivery"]["mode"], "announce");
    assert_eq!(add_json["job"]["delivery"]["channel"], "discord");
    assert_eq!(add_json["job"]["delivery"]["to"], "1234567890");

    let list_response = handle_api_cron_list(State(state), HeaderMap::new())
        .await
        .into_response();
    let list_json = response_json(list_response).await;
    let jobs = list_json["jobs"].as_array().expect("jobs array");
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0]["delivery"]["mode"], "announce");
    assert_eq!(jobs[0]["delivery"]["channel"], "discord");
    assert_eq!(jobs[0]["delivery"]["to"], "1234567890");
}

#[tokio::test]
async fn cron_api_accepts_agent_jobs() {
    let tmp = tempfile::TempDir::new().unwrap();
    let config = clawseed_config::schema::Config {
        workspace_dir: tmp.path().join("workspace"),
        config_path: tmp.path().join("config.toml"),
        ..clawseed_config::schema::Config::default()
    };
    std::fs::create_dir_all(&config.workspace_dir).unwrap();
    let state = test_state(config);

    let response = handle_api_cron_add(
        State(state.clone()),
        HeaderMap::new(),
        Json(
            serde_json::from_value::<CronAddBody>(serde_json::json!({
                "name": "agent-job",
                "schedule": "*/5 * * * *",
                "job_type": "agent",
                "command": "ignored shell command",
                "prompt": "summarize the latest logs"
            }))
            .expect("body should deserialize"),
        ),
    )
    .await
    .into_response();

    let json = response_json(response).await;
    assert_eq!(json["status"], "ok");

    let config = state.config.lock().clone();
    let jobs = clawseed_agent::cron::list_jobs(&config).unwrap();
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0].job_type, clawseed_agent::cron::JobType::Agent);
    assert_eq!(jobs[0].prompt.as_deref(), Some("summarize the latest logs"));
}

#[tokio::test]
async fn cron_api_rejects_announce_delivery_without_target() {
    let tmp = tempfile::TempDir::new().unwrap();
    let config = clawseed_config::schema::Config {
        workspace_dir: tmp.path().join("workspace"),
        config_path: tmp.path().join("config.toml"),
        ..clawseed_config::schema::Config::default()
    };
    std::fs::create_dir_all(&config.workspace_dir).unwrap();
    let state = test_state(config);

    let response = handle_api_cron_add(
        State(state.clone()),
        HeaderMap::new(),
        Json(
            serde_json::from_value::<CronAddBody>(serde_json::json!({
                "name": "invalid-delivery-job",
                "schedule": "*/5 * * * *",
                "command": "echo hello",
                "delivery": {
                    "mode": "announce",
                    "channel": "discord"
                }
            }))
            .expect("body should deserialize"),
        ),
    )
    .await
    .into_response();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let json = response_json(response).await;
    assert!(
        json["error"]
            .as_str()
            .unwrap_or_default()
            .contains("delivery.to is required")
    );

    let config = state.config.lock().clone();
    assert!(clawseed_agent::cron::list_jobs(&config).unwrap().is_empty());
}

#[tokio::test]
async fn cron_api_rejects_announce_delivery_with_unsupported_channel() {
    let tmp = tempfile::TempDir::new().unwrap();
    let config = clawseed_config::schema::Config {
        workspace_dir: tmp.path().join("workspace"),
        config_path: tmp.path().join("config.toml"),
        ..clawseed_config::schema::Config::default()
    };
    std::fs::create_dir_all(&config.workspace_dir).unwrap();
    let state = test_state(config);

    let response = handle_api_cron_add(
        State(state.clone()),
        HeaderMap::new(),
        Json(
            serde_json::from_value::<CronAddBody>(serde_json::json!({
                "name": "invalid-delivery-job",
                "schedule": "*/5 * * * *",
                "command": "echo hello",
                "delivery": {
                    "mode": "announce",
                    "channel": "email",
                    "to": "alerts@example.com"
                }
            }))
            .expect("body should deserialize"),
        ),
    )
    .await
    .into_response();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let json = response_json(response).await;
    assert!(
        json["error"]
            .as_str()
            .unwrap_or_default()
            .contains("unsupported delivery channel")
    );

    let config = state.config.lock().clone();
    assert!(clawseed_agent::cron::list_jobs(&config).unwrap().is_empty());
}
