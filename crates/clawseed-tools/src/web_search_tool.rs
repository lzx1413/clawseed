use super::web_search_provider_routing::{WebSearchProviderRoute, resolve_web_search_provider};
use async_trait::async_trait;
use clawseed_api::tool::{Tool, ToolResult};
use clawseed_api::tool_context::ToolContext;
use regex::Regex;
use serde_json::json;
use std::time::Duration;

/// Web search tool for searching the internet.
/// Supports multiple providers: DuckDuckGo (free), Brave (requires API key),
/// SearXNG (self-hosted, requires instance URL).
pub struct WebSearchTool {
    /// Provider selector as configured by user.
    provider: String,
    /// Brave API key.
    brave_api_key: Option<String>,
    /// SearXNG instance base URL.
    searxng_instance_url: Option<String>,
    /// Tavily API key.
    tavily_api_key: Option<String>,
    max_results: usize,
    timeout_secs: u64,
}

impl WebSearchTool {
    pub fn new(
        provider: String,
        brave_api_key: Option<String>,
        max_results: usize,
        timeout_secs: u64,
    ) -> Self {
        Self {
            provider: provider.trim().to_lowercase(),
            brave_api_key,
            searxng_instance_url: None,
            tavily_api_key: None,
            max_results: max_results.clamp(1, 10),
            timeout_secs: timeout_secs.max(1),
        }
    }

    /// Create a `WebSearchTool` with full configuration.
    pub fn new_with_config(
        provider: String,
        brave_api_key: Option<String>,
        searxng_instance_url: Option<String>,
        tavily_api_key: Option<String>,
        max_results: usize,
        timeout_secs: u64,
    ) -> Self {
        Self {
            provider: provider.trim().to_lowercase(),
            brave_api_key,
            searxng_instance_url,
            tavily_api_key,
            max_results: max_results.clamp(1, 10),
            timeout_secs: timeout_secs.max(1),
        }
    }

    /// Resolve the Brave API key from configuration.
    fn resolve_brave_api_key(&self) -> anyhow::Result<String> {
        self.brave_api_key
            .as_ref()
            .filter(|k| !k.is_empty())
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("Brave API key not configured"))
    }

    fn resolve_tavily_api_key(&self) -> anyhow::Result<String> {
        self.tavily_api_key
            .as_ref()
            .filter(|k| !k.is_empty())
            .cloned()
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "Tavily API key not configured. Get a free key at https://tavily.com"
                )
            })
    }

    async fn search_duckduckgo(&self, query: &str) -> anyhow::Result<String> {
        let encoded_query = urlencoding::encode(query);
        let search_url = format!("https://html.duckduckgo.com/html/?q={}", encoded_query);

        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(self.timeout_secs))
            .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
            .build()?;

        let response = client.get(&search_url).send().await?;

        if !response.status().is_success() {
            anyhow::bail!(
                "DuckDuckGo search failed with status: {}",
                response.status()
            );
        }

        let html = response.text().await?;
        self.parse_duckduckgo_results(&html, query)
    }

    fn parse_duckduckgo_results(&self, html: &str, query: &str) -> anyhow::Result<String> {
        let link_regex = Regex::new(
            r#"<a[^>]*class="[^"]*result__a[^"]*"[^>]*href="([^"]+)"[^>]*>([\s\S]*?)</a>"#,
        )?;

        let snippet_regex = Regex::new(r#"<a class="result__snippet[^"]*"[^>]*>([\s\S]*?)</a>"#)?;

        let link_matches: Vec<_> = link_regex
            .captures_iter(html)
            .take(self.max_results + 2)
            .collect();

        let snippet_matches: Vec<_> = snippet_regex
            .captures_iter(html)
            .take(self.max_results + 2)
            .collect();

        if link_matches.is_empty() {
            return Ok(format!("No results found for: {}", query));
        }

        let mut lines = vec![format!("Search results for: {} (via DuckDuckGo)", query)];

        let count = link_matches.len().min(self.max_results);

        for i in 0..count {
            let caps = &link_matches[i];
            let url_str = decode_ddg_redirect_url(&caps[1]);
            let title = strip_tags(&caps[2]);

            lines.push(format!("{}. {}", i + 1, title.trim()));
            lines.push(format!("   {}", url_str.trim()));

            if i < snippet_matches.len() {
                let snippet = strip_tags(&snippet_matches[i][1]);
                let snippet = snippet.trim();
                if !snippet.is_empty() {
                    lines.push(format!("   {}", snippet));
                }
            }
        }

        Ok(lines.join("\n"))
    }

    async fn search_brave(&self, query: &str) -> anyhow::Result<String> {
        let api_key = self.resolve_brave_api_key()?;

        let encoded_query = urlencoding::encode(query);
        let search_url = format!(
            "https://api.search.brave.com/res/v1/web/search?q={}&count={}",
            encoded_query, self.max_results
        );

        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(self.timeout_secs))
            .build()?;

        let response = client
            .get(&search_url)
            .header("Accept", "application/json")
            .header("X-Subscription-Token", &api_key)
            .send()
            .await?;

        if !response.status().is_success() {
            anyhow::bail!("Brave search failed with status: {}", response.status());
        }

        let json: serde_json::Value = response.json().await?;
        self.parse_brave_results(&json, query)
    }

    fn parse_brave_results(&self, json: &serde_json::Value, query: &str) -> anyhow::Result<String> {
        let results = json
            .get("web")
            .and_then(|w| w.get("results"))
            .and_then(|r| r.as_array())
            .ok_or_else(|| anyhow::anyhow!("Invalid Brave API response"))?;

        if results.is_empty() {
            return Ok(format!("No results found for: {}", query));
        }

        let mut lines = vec![format!("Search results for: {} (via Brave)", query)];

        for (i, result) in results.iter().take(self.max_results).enumerate() {
            let title = result
                .get("title")
                .and_then(|t| t.as_str())
                .unwrap_or("No title");
            let url = result.get("url").and_then(|u| u.as_str()).unwrap_or("");
            let description = result
                .get("description")
                .and_then(|d| d.as_str())
                .unwrap_or("");

            lines.push(format!("{}. {}", i + 1, title));
            lines.push(format!("   {}", url));
            if !description.is_empty() {
                lines.push(format!("   {}", description));
            }
        }

        Ok(lines.join("\n"))
    }

    async fn search_bing(&self, query: &str) -> anyhow::Result<String> {
        let encoded_query = urlencoding::encode(query);
        let search_url = format!(
            "https://www.bing.com/search?q={}&count={}",
            encoded_query, self.max_results
        );

        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(self.timeout_secs))
            .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
            .build()?;

        let response = client.get(&search_url).send().await?;

        if !response.status().is_success() {
            anyhow::bail!("Bing search failed with status: {}", response.status());
        }

        let html = response.text().await?;
        self.parse_bing_results(&html, query)
    }

    fn parse_bing_results(&self, html: &str, query: &str) -> anyhow::Result<String> {
        let result_regex = Regex::new(r#"<li[^>]*class="b_algo"[^>]*>([\s\S]*?)</li>"#)?;
        let link_regex = Regex::new(r#"<a[^>]*href="(https?://[^"]+)"[^>]*>([\s\S]*?)</a>"#)?;
        let snippet_regex = Regex::new(r#"<p[^>]*>([\s\S]*?)</p>"#)?;

        let results: Vec<_> = result_regex
            .captures_iter(html)
            .take(self.max_results)
            .collect();

        if results.is_empty() {
            return Ok(format!("No results found for: {}", query));
        }

        let mut lines = vec![format!("Search results for: {} (via Bing)", query)];

        for (i, cap) in results.iter().enumerate() {
            let block = &cap[1];
            if let Some(link_cap) = link_regex.captures(block) {
                let url = &link_cap[1];
                let title = strip_tags(&link_cap[2]);
                lines.push(format!("{}. {}", i + 1, title.trim()));
                lines.push(format!("   {}", url));

                if let Some(snippet_cap) = snippet_regex.captures(block) {
                    let snippet = strip_tags(&snippet_cap[1]);
                    let snippet = snippet.trim();
                    if !snippet.is_empty() {
                        lines.push(format!("   {}", snippet));
                    }
                }
            }
        }

        if lines.len() == 1 {
            return Ok(format!("No results found for: {}", query));
        }

        Ok(lines.join("\n"))
    }

    async fn search_tavily(&self, query: &str) -> anyhow::Result<String> {
        let api_key = self.resolve_tavily_api_key()?;

        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(self.timeout_secs))
            .build()?;

        let body = json!({
            "api_key": api_key,
            "query": query,
            "max_results": self.max_results,
            "search_depth": "basic",
            "include_answer": false
        });

        let response = client
            .post("https://api.tavily.com/search")
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            anyhow::bail!("Tavily search failed with status {}: {}", status, text);
        }

        let json: serde_json::Value = response.json().await?;
        self.parse_tavily_results(&json, query)
    }

    fn parse_tavily_results(
        &self,
        json: &serde_json::Value,
        query: &str,
    ) -> anyhow::Result<String> {
        let results = json
            .get("results")
            .and_then(|r| r.as_array())
            .ok_or_else(|| anyhow::anyhow!("Invalid Tavily API response"))?;

        if results.is_empty() {
            return Ok(format!("No results found for: {}", query));
        }

        let mut lines = vec![format!("Search results for: {} (via Tavily)", query)];

        for (i, result) in results.iter().take(self.max_results).enumerate() {
            let title = result
                .get("title")
                .and_then(|t| t.as_str())
                .unwrap_or("No title");
            let url = result.get("url").and_then(|u| u.as_str()).unwrap_or("");
            let content = result.get("content").and_then(|c| c.as_str()).unwrap_or("");

            lines.push(format!("{}. {}", i + 1, title));
            lines.push(format!("   {}", url));
            if !content.is_empty() {
                lines.push(format!("   {}", content));
            }
        }

        Ok(lines.join("\n"))
    }

    /// Resolve the SearXNG instance URL from configuration.
    fn resolve_searxng_instance_url(&self) -> anyhow::Result<String> {
        self.searxng_instance_url
            .as_ref()
            .filter(|u| !u.is_empty())
            .cloned()
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "SearXNG instance URL not configured. Set the searxng_instance_url parameter \
                     or the SEARXNG_INSTANCE_URL environment variable."
                )
            })
    }

    async fn search_searxng(&self, query: &str) -> anyhow::Result<String> {
        let instance_url = self.resolve_searxng_instance_url()?;
        let base_url = instance_url.trim_end_matches('/');

        let encoded_query = urlencoding::encode(query);
        let search_url = format!(
            "{}/search?q={}&format=json&pageno=1",
            base_url, encoded_query
        );

        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(self.timeout_secs))
            .user_agent("ClawSeed/1.0")
            .build()?;

        let response = client
            .get(&search_url)
            .header("Accept", "application/json")
            .send()
            .await?;

        if !response.status().is_success() {
            anyhow::bail!("SearXNG search failed with status: {}", response.status());
        }

        let json: serde_json::Value = response.json().await?;
        self.parse_searxng_results(&json, query)
    }

    fn parse_searxng_results(
        &self,
        json: &serde_json::Value,
        query: &str,
    ) -> anyhow::Result<String> {
        let results = json
            .get("results")
            .and_then(|r| r.as_array())
            .ok_or_else(|| anyhow::anyhow!("Invalid SearXNG API response"))?;

        if results.is_empty() {
            return Ok(format!("No results found for: {}", query));
        }

        let mut lines = vec![format!("Search results for: {} (via SearXNG)", query)];

        for (i, result) in results.iter().take(self.max_results).enumerate() {
            let title = result
                .get("title")
                .and_then(|t| t.as_str())
                .unwrap_or("No title");
            let url = result.get("url").and_then(|u| u.as_str()).unwrap_or("");
            let content = result.get("content").and_then(|c| c.as_str()).unwrap_or("");

            lines.push(format!("{}. {}", i + 1, title));
            lines.push(format!("   {}", url));
            if !content.is_empty() {
                lines.push(format!("   {}", content));
            }
        }

        Ok(lines.join("\n"))
    }
}

fn decode_ddg_redirect_url(raw_url: &str) -> String {
    if let Some(index) = raw_url.find("uddg=") {
        let encoded = &raw_url[index + 5..];
        let encoded = encoded.split('&').next().unwrap_or(encoded);
        if let Ok(decoded) = urlencoding::decode(encoded) {
            return decoded.into_owned();
        }
    }

    raw_url.to_string()
}

fn strip_tags(content: &str) -> String {
    let re = Regex::new(r"<[^>]+>").unwrap();
    re.replace_all(content, "").to_string()
}

#[async_trait]
impl Tool for WebSearchTool {
    fn name(&self) -> &str {
        "web_search_tool"
    }

    fn description(&self) -> &str {
        "Search the web for information. Returns relevant search results with titles, URLs, and descriptions. Use this to find current information, news, or research topics."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "The search query. Be specific for better results."
                }
            },
            "required": ["query"]
        })
    }

    async fn execute(
        &self,
        args: serde_json::Value,
        _ctx: &dyn ToolContext,
    ) -> anyhow::Result<ToolResult> {
        let query = args
            .get("query")
            .and_then(|q| q.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: query"))?;

        if query.trim().is_empty() {
            anyhow::bail!("Search query cannot be empty");
        }

        tracing::info!("Searching web for: {}", query);

        let resolution = resolve_web_search_provider(&self.provider);
        if resolution.used_fallback {
            tracing::warn!(
                "Unknown web search provider '{}'; falling back to '{}'",
                self.provider,
                resolution.canonical_provider
            );
        }

        let result = match resolution.route {
            WebSearchProviderRoute::DuckDuckGo => self.search_duckduckgo(query).await?,
            WebSearchProviderRoute::Tavily => self.search_tavily(query).await?,
            WebSearchProviderRoute::Brave => self.search_brave(query).await?,
            WebSearchProviderRoute::SearXNG => self.search_searxng(query).await?,
            WebSearchProviderRoute::Bing => self.search_bing(query).await?,
        };

        Ok(ToolResult {
            success: true,
            output: result,
            error: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_name() {
        let tool = WebSearchTool::new("duckduckgo".to_string(), None, 5, 15);
        assert_eq!(tool.name(), "web_search_tool");
    }

    #[test]
    fn test_tool_description() {
        let tool = WebSearchTool::new("duckduckgo".to_string(), None, 5, 15);
        assert!(tool.description().contains("Search the web"));
    }

    #[test]
    fn test_parameters_schema() {
        let tool = WebSearchTool::new("duckduckgo".to_string(), None, 5, 15);
        let schema = tool.parameters_schema();
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["query"].is_object());
    }

    #[test]
    fn test_strip_tags() {
        let html = "<b>Hello</b> <i>World</i>";
        assert_eq!(strip_tags(html), "Hello World");
    }

    #[test]
    fn test_parse_duckduckgo_results_empty() {
        let tool = WebSearchTool::new("duckduckgo".to_string(), None, 5, 15);
        let result = tool
            .parse_duckduckgo_results("<html>No results here</html>", "test")
            .unwrap();
        assert!(result.contains("No results found"));
    }

    #[test]
    fn test_parse_duckduckgo_results_with_data() {
        let tool = WebSearchTool::new("duckduckgo".to_string(), None, 5, 15);
        let html = r#"
            <a class="result__a" href="https://example.com">Example Title</a>
            <a class="result__snippet">This is a description</a>
        "#;
        let result = tool.parse_duckduckgo_results(html, "test").unwrap();
        assert!(result.contains("Example Title"));
        assert!(result.contains("https://example.com"));
    }

    #[test]
    fn test_parse_duckduckgo_results_decodes_redirect_url() {
        let tool = WebSearchTool::new("duckduckgo".to_string(), None, 5, 15);
        let html = r#"
            <a class="result__a" href="https://duckduckgo.com/l/?uddg=https%3A%2F%2Fexample.com%2Fpath%3Fa%3D1&amp;rut=test">Example Title</a>
            <a class="result__snippet">This is a description</a>
        "#;
        let result = tool.parse_duckduckgo_results(html, "test").unwrap();
        assert!(result.contains("https://example.com/path?a=1"));
    }

    #[test]
    fn test_constructor_clamps_web_search_limits() {
        let tool = WebSearchTool::new("duckduckgo".to_string(), None, 0, 0);
        let html = r#"
            <a class="result__a" href="https://example.com">Example Title</a>
            <a class="result__snippet">This is a description</a>
        "#;
        let result = tool.parse_duckduckgo_results(html, "test").unwrap();
        assert!(result.contains("Example Title"));
    }

    #[tokio::test]
    async fn test_execute_missing_query() {
        let tool = WebSearchTool::new("duckduckgo".to_string(), None, 5, 15);
        struct DummyCtx;
        impl ToolContext for DummyCtx {
            fn workspace_dir(&self) -> &std::path::Path {
                std::path::Path::new("/tmp")
            }
        }
        let result = tool.execute(json!({}), &DummyCtx).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_execute_brave_without_api_key() {
        let tool = WebSearchTool::new("brave".to_string(), None, 5, 15);
        struct DummyCtx;
        impl ToolContext for DummyCtx {
            fn workspace_dir(&self) -> &std::path::Path {
                std::path::Path::new("/tmp")
            }
        }
        let result = tool.execute(json!({"query": "test"}), &DummyCtx).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("API key"));
    }

    #[test]
    fn test_resolve_brave_api_key_uses_configured_key() {
        let tool = WebSearchTool::new(
            "brave".to_string(),
            Some("sk-plaintext-key".to_string()),
            5,
            15,
        );
        let key = tool.resolve_brave_api_key().unwrap();
        assert_eq!(key, "sk-plaintext-key");
    }

    #[test]
    fn test_parse_searxng_results_with_data() {
        let tool = WebSearchTool::new("searxng".to_string(), None, 5, 15);
        let json = serde_json::json!({
            "results": [
                {
                    "title": "SearXNG Example",
                    "url": "https://example.com",
                    "content": "A privacy-respecting metasearch engine"
                }
            ]
        });
        let result = tool.parse_searxng_results(&json, "test").unwrap();
        assert!(result.contains("SearXNG Example"));
        assert!(result.contains("via SearXNG"));
    }

    #[test]
    fn test_parse_searxng_results_invalid_response() {
        let tool = WebSearchTool::new("searxng".to_string(), None, 5, 15);
        let json = serde_json::json!({"error": "bad request"});
        let result = tool.parse_searxng_results(&json, "test");
        assert!(result.is_err());
    }
}
