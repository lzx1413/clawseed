//! Present images attached to the current user turn in the client.

use async_trait::async_trait;
use clawseed_api::tool::{ContentBlock, MediaReference, Tool, ToolPresentation, ToolResult};
use clawseed_api::tool_context::ToolContext;
use serde_json::Value;

/// Exposes current-turn image attachments as structured client media.
///
/// The attachment ID is intentionally returned as an opaque `asset_id`. The
/// gateway and client resolve it through the authenticated session attachment
/// endpoint, so tools never need to construct or expose filesystem paths.
pub struct MediaPresentTool;

impl MediaPresentTool {
    pub fn new() -> Self {
        Self
    }
}

impl Default for MediaPresentTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for MediaPresentTool {
    fn name(&self) -> &str {
        "media_present"
    }

    fn description(&self) -> &str {
        "Display one or more images uploaded in the current user turn. Use this when the user asks to show or display an uploaded image."
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "attachment_ids": {
                    "type": "array",
                    "description": "Optional attachment IDs to display. Omit this field to display all images uploaded in the current turn.",
                    "items": { "type": "string" },
                    "maxItems": 4
                }
            },
            "additionalProperties": false
        })
    }

    async fn execute(&self, args: Value, ctx: &dyn ToolContext) -> anyhow::Result<ToolResult> {
        let requested_ids = args
            .get("attachment_ids")
            .and_then(Value::as_array)
            .map(|values| values.iter().filter_map(Value::as_str).collect::<Vec<_>>());

        let attachments = ctx.image_attachments();
        if attachments.is_empty() {
            return Ok(ToolResult {
                success: false,
                output: "No image was uploaded in the current user turn.".into(),
                error: Some("no current image attachments".into()),
                presentation: None,
            });
        }

        let selected = attachments.iter().filter(|attachment| {
            requested_ids
                .as_ref()
                .is_none_or(|ids| ids.contains(&attachment.id.as_str()))
        });
        let blocks = selected
            .map(|attachment| ContentBlock::Image {
                media: MediaReference {
                    asset_id: Some(attachment.id.clone()),
                    url: None,
                    mime_type: Some(attachment.mime_type.clone()),
                    thumbnail_url: None,
                    width: Some(attachment.width),
                    height: Some(attachment.height),
                    duration_ms: None,
                },
                alt: Some("用户上传的图片".into()),
            })
            .collect::<Vec<_>>();

        if blocks.is_empty() {
            return Ok(ToolResult {
                success: false,
                output: "The requested image attachment was not found in the current turn.".into(),
                error: Some("requested image attachment not found".into()),
                presentation: None,
            });
        }

        Ok(ToolResult {
            success: true,
            output: format!("Displaying {} uploaded image(s).", blocks.len()),
            error: None,
            presentation: Some(ToolPresentation::new(blocks)),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clawseed_api::provider::ImageAttachment;
    use std::path::Path;

    struct Context {
        attachments: Vec<ImageAttachment>,
    }

    impl ToolContext for Context {
        fn workspace_dir(&self) -> &Path {
            Path::new("/tmp")
        }

        fn image_attachments(&self) -> &[ImageAttachment] {
            &self.attachments
        }
    }

    fn attachment(id: &str) -> ImageAttachment {
        ImageAttachment {
            id: id.into(),
            mime_type: "image/png".into(),
            size_bytes: 10,
            width: 20,
            height: 30,
            resolved_path: None,
        }
    }

    #[tokio::test]
    async fn presents_all_current_images_by_default() {
        let tool = MediaPresentTool::new();
        let ctx = Context {
            attachments: vec![attachment("att_one"), attachment("att_two")],
        };
        let result = tool.execute(serde_json::json!({}), &ctx).await.unwrap();

        assert!(result.success);
        let presentation = result.presentation.unwrap();
        assert_eq!(presentation.blocks.len(), 2);
        assert!(matches!(
            &presentation.blocks[0],
            ContentBlock::Image { media, .. } if media.asset_id.as_deref() == Some("att_one")
        ));
    }

    #[tokio::test]
    async fn filters_requested_image_ids() {
        let tool = MediaPresentTool::new();
        let ctx = Context {
            attachments: vec![attachment("att_one"), attachment("att_two")],
        };
        let result = tool
            .execute(serde_json::json!({"attachment_ids": ["att_two"]}), &ctx)
            .await
            .unwrap();

        let presentation = result.presentation.unwrap();
        assert_eq!(presentation.blocks.len(), 1);
        assert!(matches!(
            &presentation.blocks[0],
            ContentBlock::Image { media, .. } if media.asset_id.as_deref() == Some("att_two")
        ));
    }
}
