use super::*;
use axum::extract::State;
use std::fs;

#[tokio::test]
async fn lists_plugin_manifest_and_policy() {
    let root = tempfile::tempdir().expect("temp root");
    let plugin_dir = root.path().join(".clawseed/plugins/example.echo");
    fs::create_dir_all(&plugin_dir).expect("plugin directory");
    fs::write(plugin_dir.join("main.py"), "print('ok')\n").expect("entry");
    fs::write(
        plugin_dir.join("manifest.json"),
        serde_json::json!({
            "schema_version": 1,
            "plugin_id": "example.echo",
            "name": "Echo",
            "version": "1.0.0",
            "entry": "main.py",
            "protocol": {"versions": [2], "transport": "stdio"},
            "runtime": {"language": "python"}
        })
        .to_string(),
    )
    .expect("manifest");

    let mut config = clawseed_config::schema::Config {
        workspace_dir: root.path().to_path_buf(),
        ..Default::default()
    };
    config.plugins.enabled = true;
    let state = test_state(config);
    let response = crate::api::handle_api_plugins(State(state), axum::http::HeaderMap::new())
        .await
        .into_response();
    let body = response_json(response).await;
    assert_eq!(body["plugins"][0]["plugin_id"], "example.echo");
    assert_eq!(body["plugins"][0]["status"], "enabled");
}

#[tokio::test]
async fn plugin_config_patch_rejects_invalid_ids_and_updates_section() {
    let state = test_state(clawseed_config::schema::Config::default());
    let invalid = crate::api::handle_api_plugins_config(
        State(state.clone()),
        axum::http::HeaderMap::new(),
        axum::Json(crate::api::PluginConfigPatch {
            enabled: None,
            extra_roots: None,
            max_loaded: None,
            allowed: Some(vec!["Bad ID".into()]),
            denied: None,
        }),
    )
    .await
    .into_response();
    assert_eq!(invalid.status(), axum::http::StatusCode::BAD_REQUEST);

    let updated = crate::api::handle_api_plugins_config(
        State(state.clone()),
        axum::http::HeaderMap::new(),
        axum::Json(crate::api::PluginConfigPatch {
            enabled: Some(true),
            extra_roots: Some(vec!["/tmp/plugins".into()]),
            max_loaded: Some(8),
            allowed: Some(vec!["example.echo".into()]),
            denied: Some(Vec::new()),
        }),
    )
    .await
    .into_response();
    assert_eq!(updated.status(), axum::http::StatusCode::OK);
    assert!(state.config.lock().plugins.enabled);
    assert_eq!(state.config.lock().plugins.max_loaded, 8);
}
