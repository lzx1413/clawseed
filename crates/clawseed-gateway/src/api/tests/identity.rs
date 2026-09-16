use super::*;

fn config_with_temp_save_path() -> (clawseed_config::schema::Config, tempfile::TempDir) {
    let temp_dir = tempfile::tempdir().expect("temp config dir");
    let config = clawseed_config::schema::Config {
        config_path: temp_dir.path().join("clawseed.toml"),
        ..Default::default()
    };
    (config, temp_dir)
}

#[tokio::test]
async fn persona_api_round_trips_model_thinking_and_visuals() {
    let (config, _temp_dir) = config_with_temp_save_path();
    let state = test_state(config);
    let headers = axum::http::HeaderMap::new();

    let put_response = handle_api_persona_put(
        axum::extract::State(state.clone()),
        headers.clone(),
        axum::extract::Path("translator".to_string()),
        axum::Json(PersonaPutBody {
            provider: None,
            identity: None,
            system_prompt: Some("Translate between English and Chinese.".to_string()),
            memory_namespace: Some("translator".to_string()),
            allowed_tools: vec!["web_search".to_string(), "memory_*".to_string()],
            denied_tools: Vec::new(),
            denied_skills: vec!["skill-a".to_string()],
            model: Some("gpt-4.1-mini".to_string()),
            thinking_enabled: Some(true),
            vision: Some(clawseed_config::schema::VisionMode::Disabled),
            avatar: Some("译".to_string()),
            color: Some("#0F766E".to_string()),
        }),
    )
    .await
    .into_response();
    assert_eq!(put_response.status(), axum::http::StatusCode::OK);

    let get_response = handle_api_persona_get(
        axum::extract::State(state.clone()),
        headers.clone(),
        axum::extract::Path("translator".to_string()),
    )
    .await
    .into_response();
    assert_eq!(get_response.status(), axum::http::StatusCode::OK);
    let detail = response_json(get_response).await;
    assert_eq!(detail["name"], "translator");
    assert_eq!(detail["is_persona"], true);
    assert_eq!(
        detail["system_prompt"],
        "Translate between English and Chinese."
    );
    assert_eq!(detail["memory_namespace"], "translator");
    assert_eq!(detail["allowed_tools"][0], "web_search");
    assert_eq!(detail["denied_skills"][0], "skill-a");
    assert_eq!(detail["model"], "gpt-4.1-mini");
    assert_eq!(detail["thinking_enabled"], true);
    assert_eq!(detail["vision"], "disabled");
    assert_eq!(detail["avatar"], "译");
    assert_eq!(detail["color"], "#0F766E");

    let list_response = handle_api_personas_list(axum::extract::State(state), headers)
        .await
        .into_response();
    assert_eq!(list_response.status(), axum::http::StatusCode::OK);
    let list = response_json(list_response).await;
    let persona = list["personas"]
        .as_array()
        .expect("personas array")
        .iter()
        .find(|entry| entry["name"] == "translator")
        .expect("translator persona");
    assert_eq!(persona["model"], "gpt-4.1-mini");
    assert_eq!(persona["thinking_enabled"], true);
    assert_eq!(persona["vision"], "disabled");
    assert_eq!(persona["avatar"], "译");
    assert_eq!(persona["color"], "#0F766E");
}

#[tokio::test]
async fn persona_api_delete_removes_saved_entry() {
    let (mut config, _temp_dir) = config_with_temp_save_path();
    config.agents.insert(
        "temp".to_string(),
        clawseed_config::schema::AgentEntryConfig {
            system_prompt: Some("Temporary persona".to_string()),
            model: Some("gpt-4.1-mini".to_string()),
            ..Default::default()
        },
    );
    let state = test_state(config);
    let headers = axum::http::HeaderMap::new();

    let delete_response = handle_api_persona_delete(
        axum::extract::State(state.clone()),
        headers.clone(),
        axum::extract::Path("temp".to_string()),
    )
    .await
    .into_response();
    assert_eq!(delete_response.status(), axum::http::StatusCode::OK);

    let get_response = handle_api_persona_get(
        axum::extract::State(state),
        headers,
        axum::extract::Path("temp".to_string()),
    )
    .await
    .into_response();
    assert_eq!(get_response.status(), axum::http::StatusCode::NOT_FOUND);
}
