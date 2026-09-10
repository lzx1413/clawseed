use super::*;

#[tokio::test]
async fn balance_endpoint_authenticates_and_never_reuses_another_providers_key() {
    let mut config = clawseed_config::schema::Config::default();
    config.providers.fallback = Some("saved".into());
    config.providers.models.insert(
        "saved".into(),
        serde_json::from_value(serde_json::json!({
            "base_url": "https://api.openai.com/v1",
            "api_key": "saved-secret"
        }))
        .unwrap(),
    );
    let state = test_state(config);
    let response = handle_api_provider_balance(
        State(state.clone()),
        HeaderMap::new(),
        Json(ProviderBalanceRequest {
            base_url: "https://api.deepseek.com/v1".into(),
            api_key: None,
        }),
    )
    .await
    .into_response();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["cache-control"], "no-store");
    assert_eq!(response_json(response).await["status"], "missing_key");

    // Explicitly clearing the draft key must not fall back to a saved key.
    state
        .config
        .lock()
        .providers
        .models
        .get_mut("saved")
        .unwrap()
        .base_url = Some("https://api.deepseek.com/v1".into());
    let response = handle_api_provider_balance(
        State(state.clone()),
        HeaderMap::new(),
        Json(ProviderBalanceRequest {
            base_url: "https://api.deepseek.com/v1".into(),
            api_key: Some(String::new()),
        }),
    )
    .await
    .into_response();
    assert_eq!(response_json(response).await["status"], "missing_key");

    let mut secured = state;
    secured.pairing = Arc::new(PairingGuard::new(true, &[]));
    let response = handle_api_provider_balance(
        State(secured),
        HeaderMap::new(),
        Json(ProviderBalanceRequest {
            base_url: "https://api.deepseek.com/v1".into(),
            api_key: None,
        }),
    )
    .await
    .into_response();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}
