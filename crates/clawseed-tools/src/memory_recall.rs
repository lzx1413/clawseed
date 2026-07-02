use async_trait::async_trait;
use clawseed_api::memory_traits::{Memory, SearchMode};
use clawseed_api::tool::{Tool, ToolResult};
use clawseed_api::tool_context::ToolContext;
use clawseed_memory::namespaced::PUBLIC_NAMESPACE;
use serde_json::json;
use std::fmt::Write;
use std::sync::Arc;

/// Let the agent search its own memory
pub struct MemoryRecallTool {
    memory: Arc<dyn Memory>,
}

impl MemoryRecallTool {
    pub fn new(memory: Arc<dyn Memory>) -> Self {
        Self { memory }
    }
}

#[async_trait]
impl Tool for MemoryRecallTool {
    fn name(&self) -> &str {
        "memory_recall"
    }

    fn description(&self) -> &str {
        "Search long-term memory for relevant facts, preferences, or context. By default searches memories visible to this identity, including shared public memory. Use scope 'public' to search only shared public memory. Returns scored results ranked by relevance. Supports keyword search, time-only query (since/until), or both."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "Keywords or phrase to search for in memory (optional if since/until provided)"
                },
                "limit": {
                    "type": "integer",
                    "description": "Max results to return (default: 5)"
                },
                "since": {
                    "type": "string",
                    "description": "Filter memories created at or after this time (RFC 3339, e.g. 2025-03-01T00:00:00Z)"
                },
                "until": {
                    "type": "string",
                    "description": "Filter memories created at or before this time (RFC 3339)"
                },
                "search_mode": {
                    "type": "string",
                    "enum": ["bm25", "embedding", "hybrid"],
                    "description": "Search strategy: bm25 (keyword), embedding (semantic), or hybrid (both). Defaults to config value."
                },
                "scope": {
                    "type": "string",
                    "enum": ["visible", "public"],
                    "description": "Memory scope to search. Defaults to 'visible' (private memory plus public memory when available). Use 'public' to search only shared public memory."
                }
            }
        })
    }

    async fn execute(
        &self,
        args: serde_json::Value,
        _ctx: &dyn ToolContext,
    ) -> anyhow::Result<ToolResult> {
        let query = args.get("query").and_then(|v| v.as_str()).unwrap_or("");
        let since = args.get("since").and_then(|v| v.as_str());
        let until = args.get("until").and_then(|v| v.as_str());

        let search_mode: Option<SearchMode> =
            args.get("search_mode")
                .and_then(|v| v.as_str())
                .map(|s| match s {
                    "bm25" => SearchMode::Bm25,
                    "embedding" => SearchMode::Embedding,
                    "hybrid" => SearchMode::Hybrid,
                    _ => SearchMode::Hybrid,
                });

        if query.trim().is_empty() && since.is_none() && until.is_none() {
            return Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some(
                    "Provide at least 'query' (keywords) or time range ('since'/'until')".into(),
                ),
            });
        }

        // Validate date strings
        if let Some(s) = since
            && chrono::DateTime::parse_from_rfc3339(s).is_err()
        {
            return Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some(format!(
                    "Invalid 'since' date: {s}. Expected RFC 3339 format, e.g. 2025-03-01T00:00:00Z"
                )),
            });
        }
        if let Some(u) = until
            && chrono::DateTime::parse_from_rfc3339(u).is_err()
        {
            return Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some(format!(
                    "Invalid 'until' date: {u}. Expected RFC 3339 format, e.g. 2025-03-01T00:00:00Z"
                )),
            });
        }
        if let (Some(s), Some(u)) = (since, until)
            && let (Ok(s_dt), Ok(u_dt)) = (
                chrono::DateTime::parse_from_rfc3339(s),
                chrono::DateTime::parse_from_rfc3339(u),
            )
            && s_dt >= u_dt
        {
            return Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some("'since' must be before 'until'".into()),
            });
        }

        #[allow(clippy::cast_possible_truncation)]
        let limit = args
            .get("limit")
            .and_then(serde_json::Value::as_u64)
            .map_or(5, |v| v as usize);

        let scope = args
            .get("scope")
            .and_then(|v| v.as_str())
            .unwrap_or("visible");
        let recall_result = match scope {
            "visible" => {
                self.memory
                    .recall(query, limit, None, since, until, search_mode)
                    .await
            }
            "public" => {
                self.memory
                    .recall_namespaced(
                        PUBLIC_NAMESPACE,
                        query,
                        limit,
                        None,
                        since,
                        until,
                        search_mode,
                    )
                    .await
            }
            other => {
                return Ok(ToolResult {
                    success: false,
                    output: String::new(),
                    error: Some(format!(
                        "Invalid scope '{other}'. Expected 'visible' or 'public'."
                    )),
                });
            }
        };

        match recall_result {
            Ok(entries) if entries.is_empty() => {
                // Keyword search found nothing — fall back to listing all memories
                // so the LLM can pick relevant ones from the full set.
                match self.memory.list(None, None).await.map(|all| {
                    if scope == "public" {
                        all.into_iter()
                            .filter(|entry| entry.namespace == PUBLIC_NAMESPACE)
                            .collect()
                    } else {
                        all
                    }
                }) {
                    Ok(all) if all.is_empty() => Ok(ToolResult {
                        success: true,
                        output: "No memories found.".into(),
                        error: None,
                    }),
                    Ok(all) => {
                        let shown: Vec<_> = all.iter().take(limit * 2).collect();
                        let mut output = format!(
                            "No keyword matches for '{query}'. Listing all {} memories (showing {}):\n",
                            all.len(),
                            shown.len(),
                        );
                        for entry in &shown {
                            let _ = writeln!(
                                output,
                                "- [{}][{}] {}: {}",
                                entry.category, entry.namespace, entry.key, entry.content
                            );
                        }
                        Ok(ToolResult {
                            success: true,
                            output,
                            error: None,
                        })
                    }
                    Err(_) => Ok(ToolResult {
                        success: true,
                        output: "No memories found.".into(),
                        error: None,
                    }),
                }
            }
            Ok(entries) => {
                let mut output = format!("Found {} memories:\n", entries.len());
                for entry in &entries {
                    let score = entry
                        .score
                        .map_or_else(String::new, |s| format!(" [{:.0}%]", s * 100.0));
                    let _ = writeln!(
                        output,
                        "- [{}][{}] {}: {}{score}",
                        entry.category, entry.namespace, entry.key, entry.content
                    );
                }
                Ok(ToolResult {
                    success: true,
                    output,
                    error: None,
                })
            }
            Err(e) => Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some(format!("Memory recall failed: {e}")),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clawseed_api::memory_traits::MemoryCategory;
    use clawseed_memory::sqlite::SqliteMemory;
    use tempfile::TempDir;

    fn seeded_mem() -> (TempDir, Arc<dyn Memory>) {
        let tmp = TempDir::new().unwrap();
        let mem = SqliteMemory::new(tmp.path()).unwrap();
        (tmp, Arc::new(mem))
    }

    fn test_ctx() -> impl ToolContext {
        struct DummyCtx;
        impl ToolContext for DummyCtx {
            fn workspace_dir(&self) -> &std::path::Path {
                std::path::Path::new("/tmp")
            }
        }
        DummyCtx
    }

    #[tokio::test]
    async fn recall_empty() {
        let (_tmp, mem) = seeded_mem();
        let tool = MemoryRecallTool::new(mem);
        let result = tool
            .execute(json!({"query": "anything"}), &test_ctx())
            .await
            .unwrap();
        assert!(result.success);
        assert!(result.output.contains("No memories found"));
    }

    #[tokio::test]
    async fn recall_finds_match() {
        let (_tmp, mem) = seeded_mem();
        mem.store("lang", "User prefers Rust", MemoryCategory::Core, None)
            .await
            .unwrap();
        mem.store("tz", "Timezone is EST", MemoryCategory::Core, None)
            .await
            .unwrap();

        let tool = MemoryRecallTool::new(mem);
        let result = tool
            .execute(json!({"query": "Rust"}), &test_ctx())
            .await
            .unwrap();
        assert!(result.success);
        assert!(result.output.contains("Rust"));
        assert!(result.output.contains("Found 1"));
    }

    #[tokio::test]
    async fn recall_respects_limit() {
        let (_tmp, mem) = seeded_mem();
        for i in 0..10 {
            mem.store(
                &format!("k{i}"),
                &format!("Rust fact {i}"),
                MemoryCategory::Core,
                None,
            )
            .await
            .unwrap();
        }

        let tool = MemoryRecallTool::new(mem);
        let result = tool
            .execute(json!({"query": "Rust", "limit": 3}), &test_ctx())
            .await
            .unwrap();
        assert!(result.success);
        assert!(result.output.contains("Found 3"));
    }

    #[tokio::test]
    async fn recall_requires_query_or_time() {
        let (_tmp, mem) = seeded_mem();
        let tool = MemoryRecallTool::new(mem);
        let result = tool.execute(json!({}), &test_ctx()).await.unwrap();
        assert!(!result.success);
        assert!(result.error.as_ref().unwrap().contains("at least"));
    }

    #[tokio::test]
    async fn recall_time_only_returns_entries() {
        let (_tmp, mem) = seeded_mem();
        mem.store("lang", "User prefers Rust", MemoryCategory::Core, None)
            .await
            .unwrap();
        let tool = MemoryRecallTool::new(mem);
        // Time-only: since far in past
        let result = tool
            .execute(
                json!({"since": "2020-01-01T00:00:00Z", "limit": 5}),
                &test_ctx(),
            )
            .await
            .unwrap();
        assert!(result.success);
        assert!(result.output.contains("Found 1"));
        assert!(result.output.contains("Rust"));
    }

    #[test]
    fn name_and_schema() {
        let (_tmp, mem) = seeded_mem();
        let tool = MemoryRecallTool::new(mem);
        assert_eq!(tool.name(), "memory_recall");
        assert!(tool.parameters_schema()["properties"]["query"].is_object());
    }

    #[test]
    fn score_formatted_as_percent() {
        let score: Option<f64> = Some(0.63);
        let formatted = score.map_or_else(String::new, |s| format!(" [{:.0}%]", s * 100.0));
        assert_eq!(formatted, " [63%]");

        let score: Option<f64> = Some(0.42);
        let formatted = score.map_or_else(String::new, |s| format!(" [{:.0}%]", s * 100.0));
        assert_eq!(formatted, " [42%]");

        let score: Option<f64> = Some(1.0);
        let formatted = score.map_or_else(String::new, |s| format!(" [{:.0}%]", s * 100.0));
        assert_eq!(formatted, " [100%]");

        let score: Option<f64> = Some(0.0);
        let formatted = score.map_or_else(String::new, |s| format!(" [{:.0}%]", s * 100.0));
        assert_eq!(formatted, " [0%]");

        let score: Option<f64> = None;
        let formatted = score.map_or_else(String::new, |s| format!(" [{:.0}%]", s * 100.0));
        assert_eq!(formatted, "");
    }

    #[test]
    fn schema_includes_search_mode_parameter() {
        let (_tmp, mem) = seeded_mem();
        let tool = MemoryRecallTool::new(mem);
        let schema = tool.parameters_schema();
        let search_mode = &schema["properties"]["search_mode"];
        assert_eq!(search_mode["type"], "string");
        let enum_values = search_mode["enum"].as_array().unwrap();
        assert_eq!(enum_values.len(), 3);
        assert!(enum_values.contains(&json!("bm25")));
        assert!(enum_values.contains(&json!("embedding")));
        assert!(enum_values.contains(&json!("hybrid")));
    }
}
