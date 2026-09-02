use super::*;

#[tokio::test]
async fn user_profile_api_crud_round_trip() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = test_state(clawseed_config::schema::Config::default());
    state.user_profile_store = Some(Arc::new(
        clawseed_memory::user_profile::SqliteUserProfileStore::new(dir.path()).unwrap(),
    ));
    let headers = HeaderMap::new();

    let create = handle_api_user_profile_upsert(
        State(state.clone()),
        headers.clone(),
        Json(UserProfileCreateBody {
            key: "response.style".into(),
            value: serde_json::json!("concise"),
            category: ProfileCategory::Preference,
            expires_at: None,
        }),
    )
    .await
    .into_response();
    assert_eq!(create.status(), StatusCode::CREATED);
    let created = response_json(create).await;
    let item_id = created["id"].as_str().unwrap().to_string();

    let get = handle_api_user_profile_get(State(state.clone()), headers.clone())
        .await
        .into_response();
    let profile = response_json(get).await;
    assert_eq!(profile["user_id"], crate::LOCAL_OWNER_USER_ID);
    assert_eq!(profile["version"], 1);
    assert_eq!(profile["items"][0]["value"], "concise");

    let patch = handle_api_user_profile_patch(
        State(state.clone()),
        headers.clone(),
        Path(item_id.clone()),
        Json(UserProfilePatchBody {
            value: Some(serde_json::json!("balanced")),
            category: None,
            status: None,
            expires_at: None,
            clear_expires_at: false,
        }),
    )
    .await
    .into_response();
    assert_eq!(patch.status(), StatusCode::OK);
    assert_eq!(response_json(patch).await["value"], "balanced");

    let delete =
        handle_api_user_profile_delete(State(state.clone()), headers.clone(), Path(item_id))
            .await
            .into_response();
    assert_eq!(delete.status(), StatusCode::OK);
    assert_eq!(response_json(delete).await["deleted"], true);

    let get = handle_api_user_profile_get(State(state), headers)
        .await
        .into_response();
    let profile = response_json(get).await;
    assert_eq!(profile["version"], 3);
    assert_eq!(profile["items"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn user_profile_api_reject_preserves_inferred_provenance() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = test_state(clawseed_config::schema::Config::default());
    state.user_profile_store = Some(Arc::new(
        clawseed_memory::user_profile::SqliteUserProfileStore::new(dir.path()).unwrap(),
    ));
    let store = state.user_profile_store.as_ref().unwrap();
    let inferred = store
        .upsert(
            crate::LOCAL_OWNER_USER_ID,
            ProfileItemInput {
                key: "preference.response_style".into(),
                value: serde_json::json!("concise"),
                category: ProfileCategory::Preference,
                confidence: 0.91,
                source: ProfileSource::Inferred,
                status: ProfileStatus::Active,
                evidence_session_id: Some("session-1".into()),
                expires_at: None,
            },
        )
        .await
        .unwrap();

    let response = handle_api_user_profile_patch(
        State(state),
        HeaderMap::new(),
        Path(inferred.id),
        Json(UserProfilePatchBody {
            value: None,
            category: None,
            status: Some(ProfileStatus::Rejected),
            expires_at: None,
            clear_expires_at: false,
        }),
    )
    .await
    .into_response();

    assert_eq!(response.status(), StatusCode::OK);
    let rejected = response_json(response).await;
    assert_eq!(rejected["status"], "rejected");
    assert_eq!(rejected["source"], "inferred");
    assert_eq!(rejected["confidence"], 0.91);
    assert_eq!(rejected["evidence_session_id"], "session-1");
}

#[tokio::test]
async fn user_profile_import_marks_items_imported_and_applies_strategy() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = test_state(clawseed_config::schema::Config::default());
    state.user_profile_store = Some(Arc::new(
        clawseed_memory::user_profile::SqliteUserProfileStore::new(dir.path()).unwrap(),
    ));
    let store = state.user_profile_store.as_ref().unwrap();
    store
        .upsert(
            crate::LOCAL_OWNER_USER_ID,
            ProfileItemInput {
                key: "preference.language".into(),
                value: serde_json::json!("en-US"),
                category: ProfileCategory::Preference,
                confidence: 1.0,
                source: ProfileSource::Explicit,
                status: ProfileStatus::Active,
                evidence_session_id: None,
                expires_at: None,
            },
        )
        .await
        .unwrap();

    let response = handle_api_user_profile_import(
        State(state.clone()),
        HeaderMap::new(),
        Json(UserProfileImportBody {
            strategy: ProfileImportStrategy::Append,
            items: vec![
                UserProfileImportItemBody {
                    key: "preference.language".into(),
                    value: serde_json::json!("zh-CN"),
                    category: ProfileCategory::Preference,
                    confidence: 0.9,
                    status: ProfileStatus::Active,
                    evidence_session_id: Some("session-1".into()),
                    expires_at: None,
                },
                UserProfileImportItemBody {
                    key: "preference.response_style".into(),
                    value: serde_json::json!("concise"),
                    category: ProfileCategory::Preference,
                    confidence: 0.88,
                    status: ProfileStatus::Rejected,
                    evidence_session_id: Some("session-2".into()),
                    expires_at: None,
                },
            ],
        }),
    )
    .await
    .into_response();

    assert_eq!(response.status(), StatusCode::OK);
    let result = response_json(response).await;
    assert_eq!(result["imported"], 1);
    assert_eq!(result["skipped"], 1);
    let profile = store.load(crate::LOCAL_OWNER_USER_ID).await.unwrap();
    let language = profile
        .items
        .iter()
        .find(|item| item.key == "preference.language")
        .unwrap();
    assert_eq!(language.value, serde_json::json!("en-US"));
    assert_eq!(language.source, ProfileSource::Explicit);
    let response_style = profile
        .items
        .iter()
        .find(|item| item.key == "preference.response_style")
        .unwrap();
    assert_eq!(response_style.source, ProfileSource::Imported);
    assert_eq!(response_style.status, ProfileStatus::Rejected);
    assert_eq!(
        response_style.evidence_session_id.as_deref(),
        Some("session-2")
    );
}
