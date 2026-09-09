use super::*;
use axum::extract::Query;
use clawseed_api::user_profile::UserProfileStore;

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

fn explicit_input(key: &str, value: serde_json::Value) -> ProfileItemInput {
    ProfileItemInput {
        key: key.into(),
        value,
        category: ProfileCategory::Preference,
        confidence: 1.0,
        source: ProfileSource::Explicit,
        status: ProfileStatus::Active,
        evidence_session_id: None,
        expires_at: None,
    }
}

#[tokio::test]
async fn profile_change_plan_rejects_conflicts_reuse_expiry_and_cross_user_access() {
    let dir = tempfile::tempdir().unwrap();
    let mut state = test_state(clawseed_config::schema::Config::default());
    state.config.lock().workspace_dir = dir.path().to_path_buf();
    let store =
        Arc::new(clawseed_memory::user_profile::SqliteUserProfileStore::new(dir.path()).unwrap());
    state.user_profile_store = Some(store.clone());
    let item = store
        .upsert(
            crate::LOCAL_OWNER_USER_ID,
            explicit_input("preference.language", serde_json::json!("zh-CN")),
        )
        .await
        .unwrap();

    let create = handle_api_user_profile_change_plan(
        State(state.clone()),
        HeaderMap::new(),
        Json(UserProfileChangePlanBody {
            actions: vec![clawseed_api::user_profile::ProfileChangeAction::Delete {
                item_id: item.id.clone(),
            }],
        }),
    )
    .await
    .into_response();
    let plan_id = response_json(create).await["plan_id"]
        .as_str()
        .unwrap()
        .to_string();
    store
        .upsert(
            crate::LOCAL_OWNER_USER_ID,
            explicit_input("preference.output_format", serde_json::json!("markdown")),
        )
        .await
        .unwrap();
    let conflict =
        handle_api_user_profile_apply_plan(State(state.clone()), HeaderMap::new(), Path(plan_id))
            .await
            .into_response();
    assert_eq!(conflict.status(), StatusCode::CONFLICT);

    let current = store.load(crate::LOCAL_OWNER_USER_ID).await.unwrap();
    let current_item = current.items.iter().find(|row| row.id == item.id).unwrap();
    let create = handle_api_user_profile_change_plan(
        State(state.clone()),
        HeaderMap::new(),
        Json(UserProfileChangePlanBody {
            actions: vec![clawseed_api::user_profile::ProfileChangeAction::Delete {
                item_id: current_item.id.clone(),
            }],
        }),
    )
    .await
    .into_response();
    let plan_id = response_json(create).await["plan_id"]
        .as_str()
        .unwrap()
        .to_string();
    let applied = handle_api_user_profile_apply_plan(
        State(state.clone()),
        HeaderMap::new(),
        Path(plan_id.clone()),
    )
    .await
    .into_response();
    assert_eq!(applied.status(), StatusCode::OK);
    let repeated =
        handle_api_user_profile_apply_plan(State(state.clone()), HeaderMap::new(), Path(plan_id))
            .await
            .into_response();
    assert_eq!(repeated.status(), StatusCode::CONFLICT);

    let other = store
        .upsert(
            "other-user",
            explicit_input("language", serde_json::json!("en")),
        )
        .await
        .unwrap();
    let other_plan = store
        .create_change_plan(
            "other-user",
            vec![clawseed_api::user_profile::ProfileChangeAction::Delete { item_id: other.id }],
            10,
            100,
        )
        .await
        .unwrap();
    let cross_user = handle_api_user_profile_apply_plan(
        State(state.clone()),
        HeaderMap::new(),
        Path(other_plan.plan_id),
    )
    .await
    .into_response();
    assert_eq!(cross_user.status(), StatusCode::NOT_FOUND);

    let remaining = store.load(crate::LOCAL_OWNER_USER_ID).await.unwrap().items;
    let expiring = remaining.first().unwrap();
    let expired_plan = store
        .create_change_plan(
            crate::LOCAL_OWNER_USER_ID,
            vec![clawseed_api::user_profile::ProfileChangeAction::Delete {
                item_id: expiring.id.clone(),
            }],
            10,
            100,
        )
        .await
        .unwrap();
    let conn = rusqlite::Connection::open(dir.path().join("user_model/profiles.db")).unwrap();
    conn.execute(
        "UPDATE user_profile_change_plans SET expires_at = '2000-01-01T00:00:00Z' WHERE plan_id = ?1",
        rusqlite::params![expired_plan.plan_id],
    )
    .unwrap();
    let expired = handle_api_user_profile_apply_plan(
        State(state),
        HeaderMap::new(),
        Path(expired_plan.plan_id),
    )
    .await
    .into_response();
    assert_eq!(expired.status(), StatusCode::GONE);
}

#[tokio::test]
async fn memory_api_rejects_arbitrary_namespaces() {
    let state = test_state(clawseed_config::schema::Config::default());
    let response = handle_api_memory_list(
        State(state),
        HeaderMap::new(),
        Query(MemoryQuery {
            query: None,
            category: None,
            since: None,
            until: None,
            namespace: Some("not-configured".into()),
            persona: None,
        }),
    )
    .await
    .into_response();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn knowledge_forget_plan_recovers_applying_journal_and_deletes_once() {
    let dir = tempfile::tempdir().unwrap();
    let config = clawseed_config::schema::Config {
        workspace_dir: dir.path().to_path_buf(),
        ..Default::default()
    };
    let mut state = test_state(config);
    let store =
        Arc::new(clawseed_memory::user_profile::SqliteUserProfileStore::new(dir.path()).unwrap());
    let memory = Arc::new(clawseed_memory::sqlite::SqliteMemory::new(dir.path()).unwrap());
    state.user_profile_store = Some(store.clone());
    state.mem = memory.clone();
    store
        .upsert(
            crate::LOCAL_OWNER_USER_ID,
            explicit_input("preference.code_language", serde_json::json!("Python")),
        )
        .await
        .unwrap();
    memory
        .store(
            "project-python",
            "The project uses Python",
            MemoryCategory::Core,
            None,
        )
        .await
        .unwrap();

    let preview = handle_api_knowledge_forget_plan(
        State(state.clone()),
        HeaderMap::new(),
        Json(KnowledgeForgetPlanBody {
            query: "Python".into(),
        }),
    )
    .await
    .into_response();
    assert_eq!(preview.status(), StatusCode::OK);
    let preview = response_json(preview).await;
    assert_eq!(preview["exact_profile_items"].as_array().unwrap().len(), 1);
    assert_eq!(preview["exact_memories"].as_array().unwrap().len(), 1);
    let plan_id = preview["plan_id"].as_str().unwrap().to_string();

    // Simulate a process stopping immediately after the write-ahead status update.
    rusqlite::Connection::open(dir.path().join("user_model/knowledge_operations.db"))
        .unwrap()
        .execute(
            "UPDATE knowledge_forget_plans SET status = 'applying' WHERE plan_id = ?1",
            rusqlite::params![plan_id],
        )
        .unwrap();

    let applied = handle_api_knowledge_forget_apply(
        State(state.clone()),
        HeaderMap::new(),
        Path(plan_id.clone()),
    )
    .await
    .into_response();
    assert_eq!(applied.status(), StatusCode::OK);
    assert!(
        store
            .load(crate::LOCAL_OWNER_USER_ID)
            .await
            .unwrap()
            .items
            .is_empty()
    );
    assert!(memory.get("project-python").await.unwrap().is_none());

    let repeated = handle_api_knowledge_forget_apply(State(state), HeaderMap::new(), Path(plan_id))
        .await
        .into_response();
    assert_eq!(repeated.status(), StatusCode::CONFLICT);
}
