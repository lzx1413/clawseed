use super::*;

#[tokio::test]
async fn background_summary_requires_auth_and_reports_the_dispatch_manager() {
    let mut state = test_state(clawseed_config::schema::Config::default());
    state.pairing = Arc::new(PairingGuard::new(true, &[]));
    let response = handle_api_background_jobs(State(state.clone()), HeaderMap::new())
        .await
        .into_response();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    state.pairing = Arc::new(PairingGuard::new(false, &[]));
    let workspace = tempfile::tempdir().unwrap();
    let manager = clawseed_tools::background::BackgroundJobManager::new(None);
    let job = manager
        .start("sleep 30".into(), workspace.path().into())
        .unwrap();
    state.shared_builtin_tools =
        vec![
            Arc::new(clawseed_tools::background::BackgroundStatusTool::new(
                manager.clone(),
            )) as Arc<dyn clawseed_api::tool::Tool>,
        ]
        .into();
    let response = handle_api_background_jobs(State(state), HeaderMap::new())
        .await
        .into_response();
    assert_eq!(response.status(), StatusCode::OK);
    let body = response_json(response).await;
    assert_eq!(body["active_count"], 1);
    assert_eq!(body["jobs"][0]["job_id"], job.job_id);
    assert!(body["jobs"][0].get("stdout").is_none());
    manager.cancel(&job.job_id).await.unwrap();
}
