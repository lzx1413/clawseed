use async_trait::async_trait;
use clawseed_api::tool::{Tool, ToolResult};
use clawseed_api::tool_context::ToolContext;
use serde_json::json;

/// Maximum PDF file size (50 MB).
const MAX_PDF_BYTES: u64 = 50 * 1024 * 1024;
/// Default character limit returned to the LLM.
const DEFAULT_MAX_CHARS: usize = 50_000;
/// Hard ceiling regardless of what the caller requests.
const MAX_OUTPUT_CHARS: usize = 200_000;

/// Extract plain text from a PDF file in the workspace.
///
/// PDF extraction requires the `rag-pdf` feature flag:
///   cargo build --features rag-pdf
///
/// Without the feature the tool is still registered so the LLM receives a
/// clear, actionable error rather than a missing-tool confusion.
pub struct PdfReadTool;

impl PdfReadTool {
    pub fn new() -> Self {
        Self
    }
}

impl Default for PdfReadTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for PdfReadTool {
    fn name(&self) -> &str {
        "pdf_read"
    }

    fn description(&self) -> &str {
        "Extract plain text from a PDF file in the workspace. \
         Returns all readable text. Image-only or encrypted PDFs return an empty result. \
         Requires the 'rag-pdf' build feature."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path to the PDF file. Relative paths resolve from workspace."
                },
                "max_chars": {
                    "type": "integer",
                    "description": "Maximum characters to return (default: 50000, max: 200000)",
                    "minimum": 1,
                    "maximum": 200_000
                }
            },
            "required": ["path"]
        })
    }

    async fn execute(
        &self,
        args: serde_json::Value,
        ctx: &dyn ToolContext,
    ) -> anyhow::Result<ToolResult> {
        let path = args
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'path' parameter"))?;

        let max_chars = args
            .get("max_chars")
            .and_then(|v| v.as_u64())
            .map(|n| {
                usize::try_from(n)
                    .unwrap_or(MAX_OUTPUT_CHARS)
                    .min(MAX_OUTPUT_CHARS)
            })
            .unwrap_or(DEFAULT_MAX_CHARS);

        // Security: reject path traversal
        if path.contains("../") || path.contains("..\\") || path == ".." {
            return Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some("Path traversal ('..') is not allowed.".into()),
            });
        }

        let full_path = ctx.workspace_dir().join(path);

        let resolved_path = match tokio::fs::canonicalize(&full_path).await {
            Ok(p) => p,
            Err(e) => {
                return Ok(ToolResult {
                    success: false,
                    output: String::new(),
                    error: Some(format!("Failed to resolve file path: {e}")),
                });
            }
        };

        // Verify resolved path is within workspace
        let workspace_canon = match std::fs::canonicalize(ctx.workspace_dir()) {
            Ok(p) => p,
            Err(_) => ctx.workspace_dir().to_path_buf(),
        };
        if !resolved_path.starts_with(&workspace_canon) {
            return Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some("Resolved path escapes workspace".into()),
            });
        }

        tracing::debug!("Reading PDF: {}", resolved_path.display());

        match tokio::fs::metadata(&resolved_path).await {
            Ok(meta) => {
                if meta.len() > MAX_PDF_BYTES {
                    return Ok(ToolResult {
                        success: false,
                        output: String::new(),
                        error: Some(format!(
                            "PDF too large: {} bytes (limit: {MAX_PDF_BYTES} bytes)",
                            meta.len()
                        )),
                    });
                }
            }
            Err(e) => {
                return Ok(ToolResult {
                    success: false,
                    output: String::new(),
                    error: Some(format!("Failed to read file metadata: {e}")),
                });
            }
        }

        let bytes = match tokio::fs::read(&resolved_path).await {
            Ok(b) => b,
            Err(e) => {
                return Ok(ToolResult {
                    success: false,
                    output: String::new(),
                    error: Some(format!("Failed to read PDF file: {e}")),
                });
            }
        };

        // pdf_extract is a blocking CPU-bound operation; keep it off the async executor.
        #[cfg(feature = "rag-pdf")]
        {
            let text = match tokio::task::spawn_blocking(move || {
                pdf_extract::extract_text_from_mem(&bytes)
            })
            .await
            {
                Ok(Ok(t)) => t,
                Ok(Err(e)) => {
                    return Ok(ToolResult {
                        success: false,
                        output: String::new(),
                        error: Some(format!("PDF extraction failed: {e}")),
                    });
                }
                Err(e) => {
                    return Ok(ToolResult {
                        success: false,
                        output: String::new(),
                        error: Some(format!("PDF extraction task panicked: {e}")),
                    });
                }
            };

            if text.trim().is_empty() {
                return Ok(ToolResult {
                    success: true,
                    output: "PDF contains no extractable text (may be image-only or encrypted)"
                        .into(),
                    error: None,
                });
            }

            let output = if text.chars().count() > max_chars {
                let mut truncated: String = text.chars().take(max_chars).collect();
                use std::fmt::Write as _;
                let _ = write!(truncated, "\n\n... [truncated at {max_chars} chars]");
                truncated
            } else {
                text
            };

            return Ok(ToolResult {
                success: true,
                output,
                error: None,
            });
        }

        #[cfg(not(feature = "rag-pdf"))]
        {
            let _ = bytes;
            let _ = max_chars;
            Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some(
                    "PDF extraction is not enabled. \
                     Rebuild with: cargo build --features rag-pdf"
                        .into(),
                ),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name_is_pdf_read() {
        let tool = PdfReadTool::new();
        assert_eq!(tool.name(), "pdf_read");
    }

    #[test]
    fn description_not_empty() {
        let tool = PdfReadTool::new();
        assert!(!tool.description().is_empty());
    }

    #[test]
    fn schema_has_path_required() {
        let tool = PdfReadTool::new();
        let schema = tool.parameters_schema();
        assert!(schema["properties"]["path"].is_object());
        assert!(schema["properties"]["max_chars"].is_object());
        let required = schema["required"].as_array().unwrap();
        assert!(required.contains(&json!("path")));
    }
}
