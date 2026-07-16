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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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
    pub confidence: f64,
    pub source: ProfileSource,
    pub status: ProfileStatus,
    pub evidence_session_id: Option<String>,
    pub expires_at: Option<String>,
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

#[async_trait]
pub trait UserProfileStore: Send + Sync {
    async fn load(&self, user_id: &str) -> anyhow::Result<UserProfile>;

    /// Insert a key or replace its current value for this user.
    async fn upsert(&self, user_id: &str, input: ProfileItemInput) -> anyhow::Result<ProfileItem>;

    async fn delete_item(&self, user_id: &str, item_id: &str) -> anyhow::Result<bool>;

    async fn clear(&self, user_id: &str) -> anyhow::Result<usize>;

    /// Atomically import a validated profile backup for one user.
    async fn import_items(
        &self,
        user_id: &str,
        items: Vec<ProfileItemInput>,
        strategy: ProfileImportStrategy,
    ) -> anyhow::Result<ProfileImportResult>;

    async fn health_check(&self) -> bool;
}
