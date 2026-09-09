use async_trait::async_trait;
use clawseed_api::tool::{
    ContentBlock, PresentationAction, ProfilePresentationItem, Tool, ToolPresentation, ToolResult,
};
use clawseed_api::tool_context::ToolContext;
use clawseed_api::user_profile::{
    ProfileChangeAction, ProfileChangePlan, ProfileItem, ProfileMutationResult, ProfileSearchQuery,
    UserProfileStore,
};
use serde::Deserialize;
use serde_json::json;
use std::collections::BTreeMap;
use std::sync::Arc;

const MAX_AFFECTED: usize = 100;

fn authenticated_user_id(ctx: &dyn ToolContext) -> anyhow::Result<&str> {
    ctx.user_context()
        .map(|context| context.user_id.as_str())
        .filter(|user_id| !user_id.trim().is_empty())
        .ok_or_else(|| anyhow::anyhow!("user profile tools require an authenticated user context"))
}

fn success(output: String, presentation: ToolPresentation) -> anyhow::Result<ToolResult> {
    presentation.validate()?;
    Ok(ToolResult {
        success: true,
        output,
        error: None,
        presentation: Some(presentation),
    })
}

fn item_view(item: &ProfileItem) -> ProfilePresentationItem {
    ProfilePresentationItem {
        id: item.id.clone(),
        key: item.key.clone(),
        before: Some(item.value.clone()),
        after: None,
        source: Some(item.source.to_string()),
        status: Some(item.status.to_string()),
    }
}

#[allow(clippy::too_many_arguments)]
fn profile_block(
    title: &str,
    summary: String,
    profile_version: u64,
    plan_id: Option<String>,
    operation_id: Option<String>,
    requires_confirmation: bool,
    items: Vec<ProfilePresentationItem>,
    actions: Vec<PresentationAction>,
) -> ToolPresentation {
    ToolPresentation::new(vec![ContentBlock::Profile {
        title: title.into(),
        summary,
        plan_id,
        operation_id,
        profile_version,
        requires_confirmation,
        items,
        actions,
    }])
}

pub struct UserProfileSearchTool {
    store: Arc<dyn UserProfileStore>,
}

impl UserProfileSearchTool {
    pub fn new(store: Arc<dyn UserProfileStore>) -> Self {
        Self { store }
    }
}

#[async_trait]
impl Tool for UserProfileSearchTool {
    fn name(&self) -> &str {
        "user_profile_search"
    }

    fn description(&self) -> &str {
        "Read the authenticated user's current profile without changing it. Matching results include stable item_id values required for delete, reject, rename, and merge change-plan actions."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "category": { "type": "string", "enum": ["identity", "preference", "expertise", "goal", "constraint", "accessibility"] },
                "source": { "type": "string", "enum": ["explicit", "inferred", "imported"] },
                "status": { "type": "string", "enum": ["active", "superseded", "rejected"] },
                "key": { "type": "string", "maxLength": 256 },
                "text": { "type": "string", "maxLength": 1024 },
                "updated_since": { "type": "string", "description": "RFC 3339 lower bound" },
                "updated_until": { "type": "string", "description": "RFC 3339 upper bound" }
            },
            "additionalProperties": false
        })
    }

    async fn execute(
        &self,
        args: serde_json::Value,
        ctx: &dyn ToolContext,
    ) -> anyhow::Result<ToolResult> {
        let user_id = authenticated_user_id(ctx)?;
        let query: ProfileSearchQuery = serde_json::from_value(args)?;
        let mut items = self.store.search(user_id, query).await?;
        let total = items.len();
        items.truncate(MAX_AFFECTED);
        let profile = self.store.load(user_id).await?;

        let mut categories = BTreeMap::<String, usize>::new();
        for item in &items {
            *categories.entry(item.category.to_string()).or_default() += 1;
        }
        let breakdown = categories
            .into_iter()
            .map(|(category, count)| format!("{category}: {count}"))
            .collect::<Vec<_>>()
            .join(", ");
        let summary = if breakdown.is_empty() {
            "No matching profile items.".to_string()
        } else {
            format!("{total} matching item(s). {breakdown}")
        };
        let item_details = items
            .iter()
            .map(|item| {
                let value = serde_json::to_string(&item.value).unwrap_or_else(|_| "null".into());
                format!(
                    "- item_id: {}\n  key: {}\n  value: {}\n  category: {}\n  source: {}\n  status: {}",
                    item.id, item.key, value, item.category, item.source, item.status,
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        let output = if item_details.is_empty() {
            format!("Profile version {}. {summary}", profile.version)
        } else {
            format!(
                "Profile version {}. {summary}\nUse the exact item_id below for delete, reject, rename, or merge actions:\n{item_details}",
                profile.version,
            )
        };
        success(
            output,
            profile_block(
                "User profile",
                summary,
                profile.version,
                None,
                None,
                false,
                items.iter().map(item_view).collect(),
                Vec::new(),
            ),
        )
    }
}

pub struct UserProfileChangePlanTool {
    store: Arc<dyn UserProfileStore>,
    ttl_minutes: u64,
}

impl UserProfileChangePlanTool {
    pub fn new(store: Arc<dyn UserProfileStore>, ttl_minutes: u64) -> Self {
        Self { store, ttl_minutes }
    }
}

#[derive(Deserialize)]
struct ChangePlanArgs {
    actions: Vec<ProfileChangeAction>,
}

fn plan_item_views(plan: &ProfileChangePlan) -> Vec<ProfilePresentationItem> {
    let mut views = plan
        .affected_items
        .iter()
        .map(item_view)
        .collect::<Vec<_>>();
    for action in &plan.actions {
        match action {
            ProfileChangeAction::Set { input } => {
                if let Some(view) = views.iter_mut().find(|view| view.key == input.key) {
                    view.after = Some(input.value.clone());
                } else {
                    views.push(ProfilePresentationItem {
                        id: format!("new:{}", input.key),
                        key: input.key.clone(),
                        before: None,
                        after: Some(input.value.clone()),
                        source: Some("explicit".into()),
                        status: Some("active".into()),
                    });
                }
            }
            ProfileChangeAction::Rename { item_id, new_key } => {
                if let Some(view) = views.iter_mut().find(|view| view.id == *item_id) {
                    view.after = Some(json!({ "key": new_key, "value": view.before }));
                }
            }
            ProfileChangeAction::Reject { item_id } => {
                if let Some(view) = views.iter_mut().find(|view| view.id == *item_id) {
                    view.after = Some(json!({ "status": "rejected" }));
                }
            }
            ProfileChangeAction::Delete { item_id } => {
                if let Some(view) = views.iter_mut().find(|view| view.id == *item_id) {
                    view.after = Some(serde_json::Value::Null);
                }
            }
            ProfileChangeAction::Merge {
                item_ids,
                target_key,
            } => {
                for view in views.iter_mut().filter(|view| item_ids.contains(&view.id)) {
                    view.after = Some(json!({ "merged_into": target_key }));
                }
            }
            ProfileChangeAction::DeleteExpired => {}
        }
    }
    views.truncate(MAX_AFFECTED);
    views
}

#[async_trait]
impl Tool for UserProfileChangePlanTool {
    fn name(&self) -> &str {
        "user_profile_change_plan"
    }

    fn description(&self) -> &str {
        "Create a short-lived, server-validated preview for profile changes. This never changes profile data. Use action 'set' to create a profile item or update the active item with the same key. Search first to obtain stable item IDs for reject, delete, or rename actions."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "actions": {
                    "type": "array",
                    "minItems": 1,
                    "maxItems": MAX_AFFECTED,
                    "items": {
                        "oneOf": [
                            { "type": "object", "properties": { "action": { "const": "set" }, "input": { "type": "object", "properties": { "key": { "type": "string", "maxLength": 256 }, "value": {}, "category": { "type": "string", "enum": ["identity", "preference", "expertise", "goal", "constraint", "accessibility"] }, "expires_at": { "type": ["string", "null"], "description": "Optional RFC 3339 expiry" } }, "required": ["key", "value", "category"], "additionalProperties": false } }, "required": ["action", "input"] },
                            { "type": "object", "properties": { "action": { "const": "reject" }, "item_id": { "type": "string" } }, "required": ["action", "item_id"] },
                            { "type": "object", "properties": { "action": { "const": "delete" }, "item_id": { "type": "string" } }, "required": ["action", "item_id"] },
                            { "type": "object", "properties": { "action": { "const": "rename" }, "item_id": { "type": "string" }, "new_key": { "type": "string" } }, "required": ["action", "item_id", "new_key"] },
                            { "type": "object", "properties": { "action": { "const": "merge" }, "item_ids": { "type": "array", "minItems": 2, "items": { "type": "string" } }, "target_key": { "type": "string" } }, "required": ["action", "item_ids", "target_key"] },
                            { "type": "object", "properties": { "action": { "const": "delete_expired" } }, "required": ["action"] }
                        ]
                    }
                }
            },
            "required": ["actions"],
            "additionalProperties": false
        })
    }

    async fn execute(
        &self,
        args: serde_json::Value,
        ctx: &dyn ToolContext,
    ) -> anyhow::Result<ToolResult> {
        let user_id = authenticated_user_id(ctx)?;
        let args: ChangePlanArgs = serde_json::from_value(args)?;
        let plan = self
            .store
            .create_change_plan(user_id, args.actions, self.ttl_minutes, MAX_AFFECTED)
            .await?;
        let actions = if plan.requires_confirmation {
            vec![
                PresentationAction {
                    id: "confirm".into(),
                    label: "Confirm".into(),
                    command: format!("Confirm profile change plan {}", plan.plan_id),
                    destructive: true,
                },
                PresentationAction {
                    id: "cancel".into(),
                    label: "Cancel".into(),
                    command: format!("Cancel profile change plan {}", plan.plan_id),
                    destructive: false,
                },
            ]
        } else {
            vec![PresentationAction {
                id: "apply".into(),
                label: "Apply".into(),
                command: format!("Apply profile change plan {}", plan.plan_id),
                destructive: false,
            }]
        };
        let output = format!(
            "Created profile change plan {} at profile version {}. {} Expires at {}.{}",
            plan.plan_id,
            plan.expected_profile_version,
            plan.summary,
            plan.expires_at,
            if plan.requires_confirmation {
                " Wait for explicit user confirmation before applying."
            } else {
                " It may be applied now."
            }
        );
        success(
            output,
            profile_block(
                "Profile change preview",
                plan.summary.clone(),
                plan.expected_profile_version,
                Some(plan.plan_id.clone()),
                None,
                plan.requires_confirmation,
                plan_item_views(&plan),
                actions,
            ),
        )
    }
}

pub struct UserProfileApplyPlanTool {
    store: Arc<dyn UserProfileStore>,
}

impl UserProfileApplyPlanTool {
    pub fn new(store: Arc<dyn UserProfileStore>) -> Self {
        Self { store }
    }
}

#[derive(Deserialize)]
struct PlanIdArgs {
    plan_id: String,
}

#[async_trait]
impl Tool for UserProfileApplyPlanTool {
    fn name(&self) -> &str {
        "user_profile_apply_plan"
    }

    fn description(&self) -> &str {
        "Apply one existing profile change plan by plan_id after any required user confirmation. The server verifies authenticated user, expiry, profile version, batch limit, and one-time use."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": { "plan_id": { "type": "string", "maxLength": 128 } },
            "required": ["plan_id"],
            "additionalProperties": false
        })
    }

    async fn execute(
        &self,
        args: serde_json::Value,
        ctx: &dyn ToolContext,
    ) -> anyhow::Result<ToolResult> {
        let user_id = authenticated_user_id(ctx)?;
        let args: PlanIdArgs = serde_json::from_value(args)?;
        let result = self
            .store
            .apply_change_plan(user_id, &args.plan_id, MAX_AFFECTED)
            .await?;
        mutation_result(self.store.as_ref(), user_id, result, "Profile updated").await
    }
}

/// Permanently removes one profile item without creating a change plan or undo record.
pub struct UserProfileDeleteTool {
    store: Arc<dyn UserProfileStore>,
}

impl UserProfileDeleteTool {
    pub fn new(store: Arc<dyn UserProfileStore>) -> Self {
        Self { store }
    }
}

#[derive(Deserialize)]
struct DeleteArgs {
    item_id: String,
}

#[async_trait]
impl Tool for UserProfileDeleteTool {
    fn name(&self) -> &str {
        "user_profile_delete"
    }

    fn description(&self) -> &str {
        "Permanently delete one authenticated user's profile item by item_id. This action applies immediately with no preview, confirmation, or undo. Use user_profile_search first to obtain the exact item_id."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": { "item_id": { "type": "string", "maxLength": 128 } },
            "required": ["item_id"],
            "additionalProperties": false
        })
    }

    async fn execute(
        &self,
        args: serde_json::Value,
        ctx: &dyn ToolContext,
    ) -> anyhow::Result<ToolResult> {
        let user_id = authenticated_user_id(ctx)?;
        let args: DeleteArgs = serde_json::from_value(args)?;
        if !self.store.delete_item(user_id, &args.item_id).await? {
            anyhow::bail!("profile item not found");
        }
        let profile = self.store.load(user_id).await?;
        Ok(ToolResult {
            success: true,
            output: format!(
                "Deleted profile item {}. Profile version {}.",
                args.item_id, profile.version
            ),
            error: None,
            presentation: None,
        })
    }
}

pub struct UserProfileUndoTool {
    store: Arc<dyn UserProfileStore>,
    retention_hours: u64,
}

impl UserProfileUndoTool {
    pub fn new(store: Arc<dyn UserProfileStore>, retention_hours: u64) -> Self {
        Self {
            store,
            retention_hours,
        }
    }
}

#[derive(Deserialize)]
struct UndoArgs {
    operation_id: String,
}

#[async_trait]
impl Tool for UserProfileUndoTool {
    fn name(&self) -> &str {
        "user_profile_undo"
    }

    fn description(&self) -> &str {
        "Undo one recent profile mutation by operation_id. The server verifies the authenticated user, current profile version, retention period, and one-time use."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": { "operation_id": { "type": "string", "maxLength": 128 } },
            "required": ["operation_id"],
            "additionalProperties": false
        })
    }

    async fn execute(
        &self,
        args: serde_json::Value,
        ctx: &dyn ToolContext,
    ) -> anyhow::Result<ToolResult> {
        let user_id = authenticated_user_id(ctx)?;
        let args: UndoArgs = serde_json::from_value(args)?;
        let result = self
            .store
            .undo_operation(user_id, &args.operation_id, self.retention_hours)
            .await?;
        mutation_result(
            self.store.as_ref(),
            user_id,
            result,
            "Profile change undone",
        )
        .await
    }
}

async fn mutation_result(
    store: &dyn UserProfileStore,
    user_id: &str,
    result: ProfileMutationResult,
    title: &str,
) -> anyhow::Result<ToolResult> {
    let profile = store.load(user_id).await?;
    let summary = format!(
        "Affected {} item(s); skipped {}. Operation {}.",
        result.affected, result.skipped, result.operation_id
    );
    let output = format!(
        "{title}. Profile version {}. {summary}",
        result.profile_version
    );
    success(
        output,
        profile_block(
            title,
            summary,
            result.profile_version,
            None,
            Some(result.operation_id.clone()),
            false,
            profile
                .items
                .iter()
                .take(MAX_AFFECTED)
                .map(item_view)
                .collect(),
            Vec::new(),
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use clawseed_api::tool_context::ToolContext;
    use clawseed_api::user_profile::{
        ProfileCategory, ProfileItemInput, ProfileSource, ProfileStatus, UserContext,
    };
    use std::path::Path;

    struct TestContext {
        user: Option<UserContext>,
    }

    impl ToolContext for TestContext {
        fn workspace_dir(&self) -> &Path {
            Path::new("/tmp")
        }

        fn user_context(&self) -> Option<&UserContext> {
            self.user.as_ref()
        }
    }

    fn input(key: &str, value: &str) -> ProfileItemInput {
        ProfileItemInput {
            key: key.into(),
            value: json!(value),
            category: ProfileCategory::Preference,
            confidence: 1.0,
            source: ProfileSource::Explicit,
            status: ProfileStatus::Active,
            evidence_session_id: None,
            expires_at: None,
        }
    }

    fn context() -> TestContext {
        TestContext {
            user: Some(UserContext {
                user_id: "owner".into(),
                session_id: Some("session-1".into()),
                persona_id: None,
            }),
        }
    }

    #[tokio::test]
    async fn search_is_read_only_and_user_id_is_not_a_parameter() {
        let dir = tempfile::tempdir().unwrap();
        let store: Arc<dyn UserProfileStore> = Arc::new(
            clawseed_memory::user_profile::SqliteUserProfileStore::new(dir.path()).unwrap(),
        );
        store
            .upsert("owner", input("language", "zh-CN"))
            .await
            .unwrap();
        let tool = UserProfileSearchTool::new(store.clone());
        assert!(
            tool.parameters_schema()["properties"]
                .get("user_id")
                .is_none()
        );
        let result = tool
            .execute(json!({"text": "zh-cn"}), &context())
            .await
            .unwrap();
        assert!(result.success);
        let item_id = store.load("owner").await.unwrap().items[0].id.clone();
        assert!(result.output.contains(&format!("item_id: {item_id}")));
        assert!(result.output.contains("key: preference.language"));
        result.presentation.unwrap().validate().unwrap();
        assert_eq!(store.load("owner").await.unwrap().version, 1);
    }

    #[tokio::test]
    async fn plan_apply_and_undo_use_authenticated_context() {
        let dir = tempfile::tempdir().unwrap();
        let store: Arc<dyn UserProfileStore> = Arc::new(
            clawseed_memory::user_profile::SqliteUserProfileStore::new(dir.path()).unwrap(),
        );
        let item = store
            .upsert("owner", input("language", "zh-CN"))
            .await
            .unwrap();
        let plan_tool = UserProfileChangePlanTool::new(store.clone(), 10);
        let preview = plan_tool
            .execute(
                json!({"actions": [{"action": "delete", "item_id": item.id}]}),
                &context(),
            )
            .await
            .unwrap();
        let output = preview.output;
        let plan_id = output
            .split_whitespace()
            .nth(4)
            .unwrap()
            .trim_end_matches('.')
            .to_string();
        let applied = UserProfileApplyPlanTool::new(store.clone())
            .execute(json!({"plan_id": plan_id}), &context())
            .await
            .unwrap();
        assert!(store.load("owner").await.unwrap().items.is_empty());
        let operation_id = applied
            .presentation
            .as_ref()
            .and_then(|presentation| match presentation.blocks.first() {
                Some(ContentBlock::Profile { operation_id, .. }) => operation_id.clone(),
                _ => None,
            })
            .unwrap();
        UserProfileUndoTool::new(store.clone(), 24)
            .execute(json!({"operation_id": operation_id}), &context())
            .await
            .unwrap();
        assert_eq!(store.load("owner").await.unwrap().items.len(), 1);

        let missing_context = TestContext { user: None };
        assert!(
            UserProfileSearchTool::new(store)
                .execute(json!({}), &missing_context)
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn direct_delete_uses_authenticated_context_without_presentation() {
        let dir = tempfile::tempdir().unwrap();
        let store: Arc<dyn UserProfileStore> = Arc::new(
            clawseed_memory::user_profile::SqliteUserProfileStore::new(dir.path()).unwrap(),
        );
        let item = store
            .upsert("owner", input("language", "zh-CN"))
            .await
            .unwrap();

        let result = UserProfileDeleteTool::new(store.clone())
            .execute(json!({"item_id": item.id}), &context())
            .await
            .unwrap();

        assert!(result.success);
        assert!(result.presentation.is_none());
        assert!(store.load("owner").await.unwrap().items.is_empty());
    }
}
