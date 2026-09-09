//! Tool trait and related types.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::tool_context::ToolContext;

/// Versioned, client-facing content emitted alongside a tool's text result.
///
/// `ToolResult::output` remains the plain-text context passed back to the LLM.
/// A presentation is optional and is never parsed from that text, so clients can
/// render rich content without making the agent history less reliable.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolPresentation {
    pub version: u16,
    pub blocks: Vec<ContentBlock>,
}

impl ToolPresentation {
    pub const VERSION: u16 = 1;
    pub const MAX_BLOCKS: usize = 20;
    pub const MAX_SEARCH_RESULTS: usize = 10;
    pub const MAX_PROFILE_ITEMS: usize = 100;

    pub fn new(blocks: Vec<ContentBlock>) -> Self {
        Self {
            version: Self::VERSION,
            blocks,
        }
    }

    /// Validates the untrusted, client-facing portion of a tool result before
    /// it crosses the gateway boundary. The LLM-facing `ToolResult::output`
    /// remains available even when a presentation is rejected.
    pub fn validate(&self) -> anyhow::Result<()> {
        if self.version != Self::VERSION {
            anyhow::bail!("unsupported presentation version: {}", self.version);
        }
        if self.blocks.len() > Self::MAX_BLOCKS {
            anyhow::bail!("presentation exceeds {} blocks", Self::MAX_BLOCKS);
        }
        for block in &self.blocks {
            match block {
                ContentBlock::Markdown { text } => validate_text("markdown", text, 16 * 1024)?,
                ContentBlock::Image { media, alt } => {
                    validate_media(media)?;
                    validate_optional_text("image alt", alt.as_deref(), 1_024)?;
                }
                ContentBlock::Audio { media, title } | ContentBlock::Video { media, title } => {
                    validate_media(media)?;
                    validate_optional_text("media title", title.as_deref(), 1_024)?;
                }
                ContentBlock::Link {
                    title,
                    url,
                    description,
                    source,
                    thumbnail,
                } => {
                    validate_text("link title", title, 1_024)?;
                    validate_https_url("link URL", url)?;
                    validate_optional_text("link description", description.as_deref(), 4_096)?;
                    validate_optional_text("link source", source.as_deref(), 512)?;
                    if let Some(thumbnail) = thumbnail {
                        validate_media(thumbnail)?;
                    }
                }
                ContentBlock::SearchResults { query, items } => {
                    validate_text("search query", query, 1_024)?;
                    if items.len() > Self::MAX_SEARCH_RESULTS {
                        anyhow::bail!("search results exceed {} items", Self::MAX_SEARCH_RESULTS);
                    }
                    for item in items {
                        validate_search_result(item)?;
                    }
                }
                ContentBlock::Profile {
                    title,
                    summary,
                    plan_id,
                    operation_id,
                    items,
                    actions,
                    ..
                } => {
                    validate_text("profile title", title, 1_024)?;
                    validate_text("profile summary", summary, 4_096)?;
                    validate_optional_text("profile plan id", plan_id.as_deref(), 128)?;
                    validate_optional_text("profile operation id", operation_id.as_deref(), 128)?;
                    if items.len() > Self::MAX_PROFILE_ITEMS {
                        anyhow::bail!(
                            "profile presentation exceeds {} items",
                            Self::MAX_PROFILE_ITEMS
                        );
                    }
                    for item in items {
                        validate_text("profile item id", &item.id, 128)?;
                        validate_text("profile item key", &item.key, 256)?;
                        validate_optional_json(
                            "profile item before",
                            item.before.as_ref(),
                            8 * 1_024,
                        )?;
                        validate_optional_json(
                            "profile item after",
                            item.after.as_ref(),
                            8 * 1_024,
                        )?;
                        validate_optional_text("profile item source", item.source.as_deref(), 64)?;
                        validate_optional_text("profile item status", item.status.as_deref(), 64)?;
                    }
                    if actions.len() > 3 {
                        anyhow::bail!("profile presentation exceeds 3 actions");
                    }
                    for action in actions {
                        validate_text("profile action id", &action.id, 64)?;
                        validate_text("profile action label", &action.label, 128)?;
                        validate_text("profile action command", &action.command, 512)?;
                    }
                }
            }
        }
        Ok(())
    }
}

fn validate_optional_json(
    name: &str,
    value: Option<&serde_json::Value>,
    limit: usize,
) -> anyhow::Result<()> {
    if let Some(value) = value
        && serde_json::to_vec(value)?.len() > limit
    {
        anyhow::bail!("{name} exceeds {limit} bytes");
    }
    Ok(())
}

fn validate_search_result(item: &SearchResultItem) -> anyhow::Result<()> {
    validate_optional_text("search result id", item.id.as_deref(), 128)?;
    validate_text("search result title", &item.title, 1_024)?;
    validate_https_url("search result URL", &item.url)?;
    validate_optional_text(
        "search result description",
        item.description.as_deref(),
        4_096,
    )?;
    validate_optional_text("search result source", item.source.as_deref(), 512)?;
    if let Some(thumbnail) = &item.thumbnail {
        validate_media(thumbnail)?;
    }
    Ok(())
}

fn validate_media(media: &MediaReference) -> anyhow::Result<()> {
    if media.asset_id.is_none() && media.url.is_none() {
        anyhow::bail!("media requires an asset_id or HTTPS URL");
    }
    validate_optional_text("media asset id", media.asset_id.as_deref(), 128)?;
    if let Some(url) = media.url.as_deref() {
        validate_https_url("media URL", url)?;
    }
    if let Some(url) = media.thumbnail_url.as_deref() {
        validate_https_url("media thumbnail URL", url)?;
    }
    validate_optional_text("media MIME type", media.mime_type.as_deref(), 128)?;
    Ok(())
}

fn validate_optional_text(name: &str, value: Option<&str>, limit: usize) -> anyhow::Result<()> {
    if let Some(value) = value {
        validate_text(name, value, limit)?;
    }
    Ok(())
}

fn validate_text(name: &str, value: &str, limit: usize) -> anyhow::Result<()> {
    if value.len() > limit {
        anyhow::bail!("{name} exceeds {limit} bytes");
    }
    Ok(())
}

fn validate_https_url(name: &str, value: &str) -> anyhow::Result<()> {
    if !value.starts_with("https://") {
        anyhow::bail!("{name} must use HTTPS");
    }
    Ok(())
}

/// A structured content block that a client can render without interpreting
/// model-generated HTML or scraping URLs from tool text.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlock {
    Markdown {
        text: String,
    },
    Image {
        media: MediaReference,
        alt: Option<String>,
    },
    Audio {
        media: MediaReference,
        title: Option<String>,
    },
    Video {
        media: MediaReference,
        title: Option<String>,
    },
    Link {
        title: String,
        url: String,
        description: Option<String>,
        source: Option<String>,
        thumbnail: Option<MediaReference>,
    },
    SearchResults {
        query: String,
        items: Vec<SearchResultItem>,
    },
    /// A deterministic profile view or change preview. Actions are user-visible
    /// commands; applying a plan still requires server-side identity, TTL, and
    /// version validation.
    Profile {
        title: String,
        summary: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        plan_id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        operation_id: Option<String>,
        profile_version: u64,
        requires_confirmation: bool,
        items: Vec<ProfilePresentationItem>,
        actions: Vec<PresentationAction>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProfilePresentationItem {
    pub id: String,
    pub key: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub before: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub after: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PresentationAction {
    pub id: String,
    pub label: String,
    pub command: String,
    pub destructive: bool,
}

/// A remote or gateway-managed media item. At least one of `asset_id` and
/// `url` must be supplied by the producer; client-side URL policy is enforced
/// at the presentation boundary.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MediaReference {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub asset_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumbnail_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
}

/// A single item in a structured web-search result block.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SearchResultItem {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub title: String,
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumbnail: Option<MediaReference>,
}

/// Result of a tool execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    pub success: bool,
    pub output: String,
    pub error: Option<String>,
    /// Optional structured data for clients. This is deliberately separate
    /// from `output`, which is the tool context visible to the LLM.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub presentation: Option<ToolPresentation>,
}

/// Description of a tool for the LLM.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

/// Core tool trait — implement for any capability.
///
/// Tools only depend on `clawseed-api` traits. Runtime dependencies
/// (memory, etc.) are received via constructor injection.
#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn parameters_schema(&self) -> serde_json::Value;

    /// Execute the tool with given arguments and context.
    async fn execute(
        &self,
        args: serde_json::Value,
        ctx: &dyn ToolContext,
    ) -> anyhow::Result<ToolResult>;

    /// Get the full spec for LLM registration.
    fn spec(&self) -> ToolSpec {
        ToolSpec {
            name: self.name().to_string(),
            description: self.description().to_string(),
            parameters: self.parameters_schema(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presentation_serializes_as_a_versioned_tagged_payload() {
        let presentation = ToolPresentation::new(vec![ContentBlock::SearchResults {
            query: "mars exploration".into(),
            items: vec![SearchResultItem {
                id: Some("nasa-mars".into()),
                title: "NASA Mars Exploration".into(),
                url: "https://mars.nasa.gov/".into(),
                description: Some("Mission information".into()),
                source: Some("NASA".into()),
                thumbnail: None,
            }],
        }]);

        let value = serde_json::to_value(presentation).unwrap();
        assert_eq!(value["version"], 1);
        assert_eq!(value["blocks"][0]["type"], "search_results");
    }

    #[test]
    fn presentation_rejects_insecure_media_urls() {
        let presentation = ToolPresentation::new(vec![ContentBlock::Image {
            media: MediaReference {
                asset_id: None,
                url: Some("http://example.test/image.jpg".into()),
                mime_type: Some("image/jpeg".into()),
                thumbnail_url: None,
                width: None,
                height: None,
                duration_ms: None,
            },
            alt: None,
        }]);

        assert!(
            presentation
                .validate()
                .unwrap_err()
                .to_string()
                .contains("HTTPS")
        );
    }

    #[test]
    fn profile_presentation_round_trips_and_validates() {
        let presentation = ToolPresentation::new(vec![ContentBlock::Profile {
            title: "About me".into(),
            summary: "One proposed change".into(),
            plan_id: Some("plan-1".into()),
            operation_id: None,
            profile_version: 3,
            requires_confirmation: true,
            items: vec![ProfilePresentationItem {
                id: "item-1".into(),
                key: "preference.response_style".into(),
                before: Some(serde_json::json!("detailed")),
                after: Some(serde_json::json!("concise")),
                source: Some("explicit".into()),
                status: Some("active".into()),
            }],
            actions: vec![PresentationAction {
                id: "confirm".into(),
                label: "Confirm".into(),
                command: "Apply profile plan plan-1".into(),
                destructive: false,
            }],
        }]);
        presentation.validate().unwrap();
        let json = serde_json::to_string(&presentation).unwrap();
        let decoded: ToolPresentation = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, presentation);
    }
}
