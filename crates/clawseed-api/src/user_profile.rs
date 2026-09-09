//! Structured user profile types and persistence contract.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Stable identity propagated from the authenticated transport to an agent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserContext {
    pub user_id: String,
    pub session_id: Option<String>,
    pub persona_id: Option<String>,
}

/// Supported profile dimensions. Sensitive traits are intentionally excluded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProfileCategory {
    Identity,
    Preference,
    Expertise,
    Goal,
    Constraint,
    Accessibility,
}

impl std::fmt::Display for ProfileCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let value = match self {
            Self::Identity => "identity",
            Self::Preference => "preference",
            Self::Expertise => "expertise",
            Self::Goal => "goal",
            Self::Constraint => "constraint",
            Self::Accessibility => "accessibility",
        };
        f.write_str(value)
    }
}

impl std::str::FromStr for ProfileCategory {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "identity" => Ok(Self::Identity),
            "preference" => Ok(Self::Preference),
            "expertise" => Ok(Self::Expertise),
            "goal" => Ok(Self::Goal),
            "constraint" => Ok(Self::Constraint),
            "accessibility" => Ok(Self::Accessibility),
            _ => Err(format!("unsupported profile category: {value}")),
        }
    }
}

/// Canonical profile-key registry shared by inference, APIs, and imports.
pub struct ProfileKeyRegistry;

impl ProfileKeyRegistry {
    const BUILT_INS: &'static [(&'static str, ProfileCategory)] = &[
        ("identity.display_name", ProfileCategory::Identity),
        ("identity.locale", ProfileCategory::Identity),
        ("identity.pronouns", ProfileCategory::Identity),
        ("identity.timezone", ProfileCategory::Identity),
        ("preference.language", ProfileCategory::Preference),
        ("preference.response_style", ProfileCategory::Preference),
        ("preference.output_format", ProfileCategory::Preference),
        ("preference.code_language", ProfileCategory::Preference),
        ("goal.primary", ProfileCategory::Goal),
        ("constraint.no_cloud_services", ProfileCategory::Constraint),
        ("accessibility.captions", ProfileCategory::Accessibility),
        (
            "accessibility.color_contrast",
            ProfileCategory::Accessibility,
        ),
        ("accessibility.input_method", ProfileCategory::Accessibility),
        (
            "accessibility.screen_reader",
            ProfileCategory::Accessibility,
        ),
        ("accessibility.text_size", ProfileCategory::Accessibility),
    ];

    /// Normalize historical aliases and place unregistered extensible keys
    /// under `custom.<category>.*`. Identity and accessibility remain strict.
    pub fn normalize(key: &str, category: ProfileCategory) -> Option<String> {
        let key = key.trim().to_ascii_lowercase();
        let alias = match key.as_str() {
            "response.style" | "response_style" | "preference.style" => "preference.response_style",
            "language" | "response.language" => "preference.language",
            "output.format" | "preference.format" => "preference.output_format",
            "code.language" | "preference.programming_language" => "preference.code_language",
            "display_name" | "identity.name" => "identity.display_name",
            "locale" => "identity.locale",
            "timezone" => "identity.timezone",
            other => other,
        };
        if !Self::valid_syntax(alias) {
            return None;
        }
        if Self::BUILT_INS
            .iter()
            .any(|(registered, registered_category)| {
                *registered == alias && *registered_category == category
            })
        {
            return Some(alias.to_string());
        }
        if matches!(
            category,
            ProfileCategory::Identity | ProfileCategory::Accessibility
        ) {
            return None;
        }

        let category_prefix = format!("{category}.");
        let custom_prefix = format!("custom.{category}.");
        let suffix = alias
            .strip_prefix(&custom_prefix)
            .or_else(|| alias.strip_prefix(&category_prefix))
            .unwrap_or(alias)
            .trim_matches('.');
        if suffix.is_empty() {
            return None;
        }
        let normalized = format!("{custom_prefix}{suffix}");
        (normalized.len() <= 256).then_some(normalized)
    }

    fn valid_syntax(key: &str) -> bool {
        !key.is_empty()
            && key.len() <= 256
            && key.bytes().all(|byte| {
                byte.is_ascii_lowercase()
                    || byte.is_ascii_digit()
                    || matches!(byte, b'.' | b'_' | b'-')
            })
    }
}

/// Provenance for a profile item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProfileSource {
    Explicit,
    Inferred,
    Imported,
}

impl std::fmt::Display for ProfileSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let value = match self {
            Self::Explicit => "explicit",
            Self::Inferred => "inferred",
            Self::Imported => "imported",
        };
        f.write_str(value)
    }
}

impl std::str::FromStr for ProfileSource {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "explicit" => Ok(Self::Explicit),
            "inferred" => Ok(Self::Inferred),
            "imported" => Ok(Self::Imported),
            _ => Err(format!("unsupported profile source: {value}")),
        }
    }
}

/// Lifecycle state retained for audit and conflict resolution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProfileStatus {
    Active,
    Superseded,
    Rejected,
}

impl std::fmt::Display for ProfileStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let value = match self {
            Self::Active => "active",
            Self::Superseded => "superseded",
            Self::Rejected => "rejected",
        };
        f.write_str(value)
    }
}

impl std::str::FromStr for ProfileStatus {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "active" => Ok(Self::Active),
            "superseded" => Ok(Self::Superseded),
            "rejected" => Ok(Self::Rejected),
            _ => Err(format!("unsupported profile status: {value}")),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProfileItem {
    pub id: String,
    pub user_id: String,
    pub key: String,
    pub value: serde_json::Value,
    pub category: ProfileCategory,
    pub confidence: f64,
    pub source: ProfileSource,
    pub status: ProfileStatus,
    pub evidence_session_id: Option<String>,
    pub expires_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub version: u64,
}

/// Input used to create or replace the active value for a profile key.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProfileItemInput {
    pub key: String,
    pub value: serde_json::Value,
    pub category: ProfileCategory,
    #[serde(default = "default_explicit_profile_confidence")]
    pub confidence: f64,
    #[serde(default = "default_profile_source")]
    pub source: ProfileSource,
    #[serde(default = "default_profile_status")]
    pub status: ProfileStatus,
    #[serde(default)]
    pub evidence_session_id: Option<String>,
    #[serde(default)]
    pub expires_at: Option<String>,
}

fn default_explicit_profile_confidence() -> f64 {
    1.0
}

fn default_profile_source() -> ProfileSource {
    ProfileSource::Explicit
}

fn default_profile_status() -> ProfileStatus {
    ProfileStatus::Active
}

/// Conflict behavior for a profile backup import.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProfileImportStrategy {
    Replace,
    Merge,
    Append,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProfileImportResult {
    pub imported: usize,
    pub skipped: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UserProfile {
    pub user_id: String,
    pub version: u64,
    pub items: Vec<ProfileItem>,
}

/// Outcome of an atomic inferred-profile write.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "result", content = "item")]
pub enum InferenceWriteResult {
    Written(Box<ProfileItem>),
    Unchanged,
    Blocked,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ProfileSearchQuery {
    pub category: Option<ProfileCategory>,
    pub source: Option<ProfileSource>,
    pub status: Option<ProfileStatus>,
    pub key: Option<String>,
    pub text: Option<String>,
    pub updated_since: Option<String>,
    pub updated_until: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "action")]
pub enum ProfileChangeAction {
    Set {
        input: ProfileItemInput,
    },
    Reject {
        item_id: String,
    },
    Delete {
        item_id: String,
    },
    Rename {
        item_id: String,
        new_key: String,
    },
    Merge {
        item_ids: Vec<String>,
        target_key: String,
    },
    DeleteExpired,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProfileChangePlan {
    pub plan_id: String,
    pub expected_profile_version: u64,
    pub affected_items: Vec<ProfileItem>,
    pub actions: Vec<ProfileChangeAction>,
    pub summary: String,
    pub requires_confirmation: bool,
    pub expires_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProfileMutationResult {
    pub profile_version: u64,
    pub operation_id: String,
    pub affected: usize,
    pub skipped: usize,
}

#[async_trait]
pub trait UserProfileStore: Send + Sync {
    async fn load(&self, user_id: &str) -> anyhow::Result<UserProfile>;

    /// Insert a key or replace its current value for this user.
    async fn upsert(&self, user_id: &str, input: ProfileItemInput) -> anyhow::Result<ProfileItem>;

    /// Atomically write an inferred value only when the current row permits it.
    async fn upsert_inferred_if_allowed(
        &self,
        user_id: &str,
        input: ProfileItemInput,
    ) -> anyhow::Result<InferenceWriteResult>;

    async fn delete_item(&self, user_id: &str, item_id: &str) -> anyhow::Result<bool>;

    async fn clear(&self, user_id: &str) -> anyhow::Result<usize>;

    /// Atomically import a validated profile backup for one user.
    async fn import_items(
        &self,
        user_id: &str,
        items: Vec<ProfileItemInput>,
        strategy: ProfileImportStrategy,
    ) -> anyhow::Result<ProfileImportResult>;

    async fn search(
        &self,
        user_id: &str,
        query: ProfileSearchQuery,
    ) -> anyhow::Result<Vec<ProfileItem>>;

    async fn create_change_plan(
        &self,
        user_id: &str,
        actions: Vec<ProfileChangeAction>,
        ttl_minutes: u64,
        max_affected: usize,
    ) -> anyhow::Result<ProfileChangePlan>;

    async fn apply_change_plan(
        &self,
        user_id: &str,
        plan_id: &str,
        max_affected: usize,
    ) -> anyhow::Result<ProfileMutationResult>;

    async fn undo_operation(
        &self,
        user_id: &str,
        operation_id: &str,
        retention_hours: u64,
    ) -> anyhow::Result<ProfileMutationResult>;

    async fn health_check(&self) -> bool;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_key_registry_normalizes_aliases() {
        assert_eq!(
            ProfileKeyRegistry::normalize("response.style", ProfileCategory::Preference),
            Some("preference.response_style".into())
        );
        assert_eq!(
            ProfileKeyRegistry::normalize("preference.style", ProfileCategory::Preference),
            Some("preference.response_style".into())
        );
        assert_eq!(
            ProfileKeyRegistry::normalize("language", ProfileCategory::Preference),
            Some("preference.language".into())
        );
    }

    #[test]
    fn profile_key_registry_scopes_unregistered_keys() {
        assert_eq!(
            ProfileKeyRegistry::normalize("expertise.rust", ProfileCategory::Expertise),
            Some("custom.expertise.rust".into())
        );
        assert_eq!(
            ProfileKeyRegistry::normalize("goal.learn_rust", ProfileCategory::Goal),
            Some("custom.goal.learn_rust".into())
        );
        assert_eq!(
            ProfileKeyRegistry::normalize("identity.email", ProfileCategory::Identity),
            None
        );
    }

    #[test]
    fn profile_change_input_defaults_to_explicit_active_item() {
        let input: ProfileItemInput = serde_json::from_value(serde_json::json!({
            "key": "preference.language",
            "value": "zh-CN",
            "category": "preference"
        }))
        .unwrap();

        assert_eq!(input.confidence, 1.0);
        assert_eq!(input.source, ProfileSource::Explicit);
        assert_eq!(input.status, ProfileStatus::Active);
        assert_eq!(input.evidence_session_id, None);
        assert_eq!(input.expires_at, None);
    }
}
