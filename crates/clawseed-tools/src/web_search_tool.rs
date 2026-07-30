use super::web_search_provider_routing::{WebSearchProviderRoute, resolve_web_search_provider};
use async_trait::async_trait;
use clawseed_api::tool::{
    ContentBlock, MediaReference, SearchResultItem, Tool, ToolPresentation, ToolResult,
};
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

/// Provider-normalized search results. This is the single source for both the
/// LLM-facing text and the client-facing presentation payload.
struct SearchResponse {
    provider: &'static str,
    items: Vec<SearchResultItem>,
    media: Vec<ContentBlock>,
}

impl SearchResponse {
    fn empty(provider: &'static str) -> Self {
        Self {
            provider,
            items: Vec::new(),
            media: Vec::new(),
        }
    }

    fn render_text(&self, query: &str) -> String {
        let media_candidates: Vec<_> = self.media.iter().filter_map(media_candidate_text).collect();
        if self.items.is_empty() && media_candidates.is_empty() {
            return format!("No results found for: {query}");
        }

        let mut lines = vec![format!(
            "Search results for: {query} (via {})",
            self.provider
        )];
        if self.items.is_empty() {
            lines.push("No text search results found.".to_string());
        } else {
            for (index, item) in self.items.iter().enumerate() {
                lines.push(format!("{}. {}", index + 1, item.title));
                lines.push(format!("   {}", item.url));
                if let Some(description) =
                    item.description.as_deref().filter(|text| !text.is_empty())
                {
                    lines.push(format!("   {description}"));
                }
            }
        }
        if !media_candidates.is_empty() {
            lines.push(String::new());
            lines.push(
                "Optional media candidates. These are not shown automatically. If the user asked for images, photos, pictures, or visual examples, select up to 3 relevant image candidates and display each selected image on its own line as `![description](https://...)`. Do not answer only with search result links when image candidates are directly requested."
                    .to_string(),
            );
            for (index, candidate) in media_candidates.iter().enumerate() {
                lines.push(format!("M{} {candidate}", index + 1));
            }
        }
        lines.join("\n")
    }

    fn into_presentation(self, query: &str) -> ToolPresentation {
        // Search media is kept as model-visible candidate data in `output`.
        // Only the agent's final answer should decide whether any media is
        // displayed, so the automatic presentation stays text/source focused.
        let items = self
            .items
            .into_iter()
            .filter(|item| item.url.starts_with("https://"))
            .collect();
        ToolPresentation::new(vec![ContentBlock::SearchResults {
            query: query.to_string(),
            items,
        }])
    }
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

    async fn search_duckduckgo(&self, query: &str) -> anyhow::Result<SearchResponse> {
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

    fn parse_duckduckgo_results(&self, html: &str, _query: &str) -> anyhow::Result<SearchResponse> {
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
            return Ok(SearchResponse::empty("DuckDuckGo"));
        }

        let mut items = Vec::new();

        let count = link_matches.len().min(self.max_results);

        for i in 0..count {
            let caps = &link_matches[i];
            let url_str = decode_ddg_redirect_url(&caps[1]);
            let title = strip_tags(&caps[2]);

            let description = if i < snippet_matches.len() {
                let snippet = strip_tags(&snippet_matches[i][1]);
                let snippet = snippet.trim();
                if !snippet.is_empty() {
                    Some(snippet.to_string())
                } else {
                    None
                }
            } else {
                None
            };
            items.push(SearchResultItem {
                id: Some(format!("ddg-{}", i + 1)),
                title: title.trim().to_string(),
                url: url_str.trim().to_string(),
                description,
                source: Some("DuckDuckGo".into()),
                thumbnail: None,
            });
        }

        Ok(SearchResponse {
            provider: "DuckDuckGo",
            items,
            media: Vec::new(),
        })
    }

    async fn search_brave(&self, query: &str) -> anyhow::Result<SearchResponse> {
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

    fn parse_brave_results(
        &self,
        json: &serde_json::Value,
        _query: &str,
    ) -> anyhow::Result<SearchResponse> {
        let results = json
            .get("web")
            .and_then(|w| w.get("results"))
            .and_then(|r| r.as_array())
            .ok_or_else(|| anyhow::anyhow!("Invalid Brave API response"))?;

        if results.is_empty() {
            return Ok(SearchResponse::empty("Brave"));
        }

        let mut items = Vec::new();

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

            items.push(SearchResultItem {
                id: Some(format!("brave-{}", i + 1)),
                title: title.to_string(),
                url: url.to_string(),
                description: (!description.is_empty()).then(|| description.to_string()),
                source: Some("Brave".into()),
                thumbnail: None,
            });
        }

        Ok(SearchResponse {
            provider: "Brave",
            items,
            media: Vec::new(),
        })
    }

    async fn search_bing(&self, query: &str) -> anyhow::Result<SearchResponse> {
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
        let mut result = self.parse_bing_results(&html, query)?;

        // Bing's normal web results do not include image URLs. Fetch the
        // dedicated image-result page separately so the presentation can show
        // real image media rather than inventing previews for web links.
        match self.search_bing_images(query).await {
            Ok(media) => result.media = media,
            Err(error) => {
                tracing::warn!(%error, "Bing image search failed; returning web results only")
            }
        }

        Ok(result)
    }

    async fn search_bing_images(&self, query: &str) -> anyhow::Result<Vec<ContentBlock>> {
        let encoded_query = urlencoding::encode(query);
        let image_url = format!(
            "https://www.bing.com/images/search?q={}&first=1&count={}",
            encoded_query, MAX_SEARCH_MEDIA_BLOCKS
        );
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(self.timeout_secs))
            .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
            .build()?;
        let response = client.get(image_url).send().await?;
        if !response.status().is_success() {
            anyhow::bail!(
                "Bing image search failed with status: {}",
                response.status()
            );
        }
        self.parse_bing_image_results(&response.text().await?)
    }

    fn parse_bing_results(&self, html: &str, _query: &str) -> anyhow::Result<SearchResponse> {
        let result_regex = Regex::new(r#"<li[^>]*class="b_algo"[^>]*>([\s\S]*?)</li>"#)?;
        let link_regex = Regex::new(r#"<a[^>]*href="(https?://[^"]+)"[^>]*>([\s\S]*?)</a>"#)?;
        let snippet_regex = Regex::new(r#"<p[^>]*>([\s\S]*?)</p>"#)?;

        let results: Vec<_> = result_regex
            .captures_iter(html)
            .take(self.max_results)
            .collect();

        if results.is_empty() {
            return Ok(SearchResponse::empty("Bing"));
        }

        let mut items = Vec::new();

        for (i, cap) in results.iter().enumerate() {
            let block = &cap[1];
            if let Some(link_cap) = link_regex.captures(block) {
                let url = &link_cap[1];
                let title = strip_tags(&link_cap[2]);
                let description = if let Some(snippet_cap) = snippet_regex.captures(block) {
                    let snippet = strip_tags(&snippet_cap[1]);
                    let snippet = snippet.trim();
                    if !snippet.is_empty() {
                        Some(snippet.to_string())
                    } else {
                        None
                    }
                } else {
                    None
                };
                items.push(SearchResultItem {
                    id: Some(format!("bing-{}", i + 1)),
                    title: title.trim().to_string(),
                    url: url.to_string(),
                    description,
                    source: Some("Bing".into()),
                    thumbnail: None,
                });
            }
        }

        if items.is_empty() {
            return Ok(SearchResponse::empty("Bing"));
        }

        Ok(SearchResponse {
            provider: "Bing",
            items,
            media: Vec::new(),
        })
    }

    fn parse_bing_image_results(&self, html: &str) -> anyhow::Result<Vec<ContentBlock>> {
        // Bing serializes result metadata in an HTML-escaped JSON `m` attribute.
        // Parsing that JSON keeps source image and thumbnail URLs distinct.
        let metadata_regex = Regex::new(r#"(?is)\bm=\"([^\"]+)\""#)?;
        let mut media = Vec::new();
        for capture in metadata_regex.captures_iter(html) {
            let decoded = decode_html_attribute(&capture[1]);
            let Ok(metadata) = serde_json::from_str::<serde_json::Value>(&decoded) else {
                continue;
            };
            let Some(url) = json_https_url(metadata.get("murl")) else {
                continue;
            };
            let title = metadata
                .get("t")
                .and_then(|value| value.as_str())
                .filter(|value| !value.trim().is_empty())
                .map(ToOwned::to_owned);
            let thumbnail_url = json_https_url(metadata.get("turl")).map(ToOwned::to_owned);
            push_media_block(
                &mut media,
                ContentBlock::Image {
                    media: MediaReference {
                        asset_id: None,
                        url: Some(url.to_owned()),
                        mime_type: None,
                        thumbnail_url,
                        width: None,
                        height: None,
                        duration_ms: None,
                    },
                    alt: title,
                },
            );
        }
        Ok(media)
    }

    async fn search_tavily(&self, query: &str) -> anyhow::Result<SearchResponse> {
        let api_key = self.resolve_tavily_api_key()?;

        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(self.timeout_secs))
            .build()?;

        let body = json!({
            "api_key": api_key,
            "query": query,
            "max_results": self.max_results,
            "search_depth": "basic",
            "include_answer": false,
            "include_images": true,
            "include_image_descriptions": true
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
        _query: &str,
    ) -> anyhow::Result<SearchResponse> {
        let results = json
            .get("results")
            .and_then(|r| r.as_array())
            .ok_or_else(|| anyhow::anyhow!("Invalid Tavily API response"))?;

        if results.is_empty() {
            return Ok(SearchResponse::empty("Tavily"));
        }

        let mut items = Vec::new();
        let mut media = Vec::new();

        for (i, result) in results.iter().take(self.max_results).enumerate() {
            let title = result
                .get("title")
                .and_then(|t| t.as_str())
                .unwrap_or("No title");
            let url = result.get("url").and_then(|u| u.as_str()).unwrap_or("");
            let content = result.get("content").and_then(|c| c.as_str()).unwrap_or("");
            items.push(SearchResultItem {
                id: Some(format!("tavily-{}", i + 1)),
                title: title.to_string(),
                url: url.to_string(),
                description: (!content.is_empty()).then(|| content.to_string()),
                source: Some("Tavily".into()),
                thumbnail: None,
            });

            if let Some(images) = result.get("images").and_then(|images| images.as_array()) {
                for image in images {
                    if let Some(url) = json_https_url(Some(image)) {
                        push_media_block(
                            &mut media,
                            ContentBlock::Image {
                                media: media_reference(url),
                                alt: json_image_description(image).map(ToOwned::to_owned),
                            },
                        );
                    }
                }
            }
        }

        if let Some(images) = json.get("images").and_then(|images| images.as_array()) {
            for image in images {
                if let Some(url) = json_https_url(Some(image)) {
                    push_media_block(
                        &mut media,
                        ContentBlock::Image {
                            media: media_reference(url),
                            alt: json_image_description(image).map(ToOwned::to_owned),
                        },
                    );
                }
            }
        }

        Ok(SearchResponse {
            provider: "Tavily",
            items,
            media,
        })
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

    async fn search_searxng(&self, query: &str) -> anyhow::Result<SearchResponse> {
        let instance_url = self.resolve_searxng_instance_url()?;
        let base_url = instance_url.trim_end_matches('/');

        let encoded_query = urlencoding::encode(query);
        let search_url = format!(
            "{}/search?q={}&format=json&pageno=1&categories=general%2Cimages",
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
        _query: &str,
    ) -> anyhow::Result<SearchResponse> {
        let results = json
            .get("results")
            .and_then(|r| r.as_array())
            .ok_or_else(|| anyhow::anyhow!("Invalid SearXNG API response"))?;

        if results.is_empty() {
            return Ok(SearchResponse::empty("SearXNG"));
        }

        let mut items = Vec::new();
        let mut media = Vec::new();

        for (i, result) in results.iter().take(self.max_results).enumerate() {
            let title = result
                .get("title")
                .and_then(|t| t.as_str())
                .unwrap_or("No title");
            let url = result.get("url").and_then(|u| u.as_str()).unwrap_or("");
            let content = result.get("content").and_then(|c| c.as_str()).unwrap_or("");
            items.push(SearchResultItem {
                id: Some(format!("searxng-{}", i + 1)),
                title: title.to_string(),
                url: url.to_string(),
                description: (!content.is_empty()).then(|| content.to_string()),
                source: Some("SearXNG".into()),
                thumbnail: None,
            });

            if let Some(image_url) = json_https_url(result.get("img_src")) {
                push_media_block(
                    &mut media,
                    ContentBlock::Image {
                        media: media_reference(image_url),
                        alt: Some(title.to_string()),
                    },
                );
            }
            if let Some(audio_url) = json_https_url(result.get("audio_src")) {
                push_media_block(
                    &mut media,
                    ContentBlock::Audio {
                        media: media_reference(audio_url),
                        title: Some(title.to_string()),
                    },
                );
            }
            // Embedded iframe pages are deliberately excluded: Media3 needs a
            // direct media resource rather than a third-party browser embed.
            if let Some(video_url) = ["video_src", "content_url", "media_url"]
                .iter()
                .find_map(|name| json_https_url(result.get(*name)))
            {
                push_media_block(
                    &mut media,
                    ContentBlock::Video {
                        media: media_reference(video_url),
                        title: Some(title.to_string()),
                    },
                );
            }
        }

        Ok(SearchResponse {
            provider: "SearXNG",
            items,
            media,
        })
    }
}

const MAX_SEARCH_MEDIA_BLOCKS: usize = 6;

fn media_reference(url: &str) -> MediaReference {
    MediaReference {
        asset_id: None,
        url: Some(url.to_owned()),
        mime_type: None,
        thumbnail_url: None,
        width: None,
        height: None,
        duration_ms: None,
    }
}

fn json_https_url(value: Option<&serde_json::Value>) -> Option<&str> {
    let value = value?;
    let url = value
        .as_str()
        .or_else(|| value.get("url").and_then(|value| value.as_str()))?;
    url.trim().starts_with("https://").then_some(url.trim())
}

fn json_image_description(value: &serde_json::Value) -> Option<&str> {
    value
        .get("description")
        .and_then(|value| value.as_str())
        .filter(|value| !value.trim().is_empty())
}

fn push_media_block(media: &mut Vec<ContentBlock>, block: ContentBlock) {
    if media.len() < MAX_SEARCH_MEDIA_BLOCKS && !media.contains(&block) {
        media.push(block);
    }
}

fn media_candidate_text(block: &ContentBlock) -> Option<String> {
    let (kind, media, title) = match block {
        ContentBlock::Image { media, alt } => ("image", media, alt.as_deref()),
        ContentBlock::Audio { media, title } => ("audio", media, title.as_deref()),
        ContentBlock::Video { media, title } => ("video", media, title.as_deref()),
        _ => return None,
    };
    let url = media.url.as_deref()?;
    let mut text = format!("{kind}: {url}");
    if let Some(title) = title.filter(|title| !title.trim().is_empty()) {
        text.push_str(" - ");
        text.push_str(title.trim());
    }
    Some(text)
}

fn decode_html_attribute(value: &str) -> String {
    value
        .replace("&quot;", "\"")
        .replace("&#34;", "\"")
        .replace("&amp;", "&")
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
        "Search the web for current information, news, research, or image requests. Returns relevant titles, HTTPS source URLs, summaries, and optional HTTPS media candidates. Media candidates are not shown automatically; when the user explicitly asks for images, photos, pictures, or visual examples, select up to 3 relevant image candidates and display them with Markdown image syntax. Do not infer or fabricate media URLs."
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

        let output = result.render_text(query);
        let presentation = result.into_presentation(query);

        Ok(ToolResult {
            success: true,
            output,
            error: None,
            presentation: Some(presentation),
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
        assert!(
            tool.description()
                .contains("Media candidates are not shown automatically")
        );
        assert!(
            tool.description()
                .contains("display them with Markdown image syntax")
        );
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
        assert!(result.items.is_empty());
        assert!(result.render_text("test").contains("No results found"));
    }

    #[test]
    fn test_parse_duckduckgo_results_with_data() {
        let tool = WebSearchTool::new("duckduckgo".to_string(), None, 5, 15);
        let html = r#"
            <a class="result__a" href="https://example.com">Example Title</a>
            <a class="result__snippet">This is a description</a>
        "#;
        let result = tool.parse_duckduckgo_results(html, "test").unwrap();
        assert_eq!(result.items.len(), 1);
        assert_eq!(result.items[0].title, "Example Title");
        assert_eq!(result.items[0].url, "https://example.com");
    }

    #[test]
    fn test_parse_duckduckgo_results_decodes_redirect_url() {
        let tool = WebSearchTool::new("duckduckgo".to_string(), None, 5, 15);
        let html = r#"
            <a class="result__a" href="https://duckduckgo.com/l/?uddg=https%3A%2F%2Fexample.com%2Fpath%3Fa%3D1&amp;rut=test">Example Title</a>
            <a class="result__snippet">This is a description</a>
        "#;
        let result = tool.parse_duckduckgo_results(html, "test").unwrap();
        assert_eq!(result.items[0].url, "https://example.com/path?a=1");
    }

    #[test]
    fn test_constructor_clamps_web_search_limits() {
        let tool = WebSearchTool::new("duckduckgo".to_string(), None, 0, 0);
        let html = r#"
            <a class="result__a" href="https://example.com">Example Title</a>
            <a class="result__snippet">This is a description</a>
        "#;
        let result = tool.parse_duckduckgo_results(html, "test").unwrap();
        assert_eq!(result.items[0].title, "Example Title");
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
        assert_eq!(result.provider, "SearXNG");
        assert_eq!(result.items[0].title, "SearXNG Example");
    }

    #[test]
    fn test_parse_bing_image_results_returns_direct_https_media_urls() {
        let tool = WebSearchTool::new("bing".to_string(), None, 5, 15);
        let html = r#"
            <a class="iusc" m="{&quot;murl&quot;:&quot;https://cdn.example.com/full.jpg&quot;,&quot;turl&quot;:&quot;https://cdn.example.com/thumb.jpg&quot;,&quot;t&quot;:&quot;Example image&quot;}"></a>
        "#;

        let media = tool.parse_bing_image_results(html).unwrap();
        let ContentBlock::Image { media, alt } = &media[0] else {
            panic!("expected image content block");
        };
        assert_eq!(
            media.url.as_deref(),
            Some("https://cdn.example.com/full.jpg")
        );
        assert_eq!(
            media.thumbnail_url.as_deref(),
            Some("https://cdn.example.com/thumb.jpg")
        );
        assert_eq!(alt.as_deref(), Some("Example image"));
    }

    #[test]
    fn test_parse_tavily_results_keeps_images_as_media_candidates() {
        let tool = WebSearchTool::new("tavily".to_string(), None, 5, 15);
        let json = serde_json::json!({
            "results": [{
                "title": "Tavily Example",
                "url": "https://example.com/article",
                "content": "A result with an image",
                "images": [{
                    "url": "https://cdn.example.com/article.jpg",
                    "description": "Article image"
                }]
            }],
            "images": ["https://cdn.example.com/query.jpg"]
        });

        let result = tool.parse_tavily_results(&json, "test").unwrap();
        assert_eq!(result.items[0].thumbnail, None);
        assert_eq!(result.media.len(), 2);
    }

    #[test]
    fn test_render_text_marks_media_as_optional_candidates() {
        let response = SearchResponse {
            provider: "Test",
            items: vec![SearchResultItem {
                id: Some("result-1".into()),
                title: "Example".into(),
                url: "https://example.com".into(),
                description: Some("A result".into()),
                source: Some("Test".into()),
                thumbnail: None,
            }],
            media: vec![ContentBlock::Image {
                media: media_reference("https://cdn.example.com/image.jpg"),
                alt: Some("Example image".into()),
            }],
        };

        let text = response.render_text("example");

        assert!(text.contains("Optional media candidates"));
        assert!(text.contains("select up to 3 relevant image candidates"));
        assert!(text.contains("![description](https://...)"));
        assert!(text.contains("M1 image: https://cdn.example.com/image.jpg - Example image"));
        assert!(!text.contains("Image media:"));
    }

    #[test]
    fn test_render_text_keeps_media_candidates_without_text_results() {
        let response = SearchResponse {
            provider: "Test",
            items: Vec::new(),
            media: vec![ContentBlock::Image {
                media: media_reference("https://cdn.example.com/image.jpg"),
                alt: Some("Example image".into()),
            }],
        };

        let text = response.render_text("example images");

        assert!(text.contains("No text search results found."));
        assert!(text.contains("Optional media candidates"));
        assert!(text.contains("M1 image: https://cdn.example.com/image.jpg - Example image"));
        assert!(!text.contains("No results found for"));
    }

    #[test]
    fn test_search_presentation_excludes_unselected_media_candidates() {
        let response = SearchResponse {
            provider: "Test",
            items: vec![SearchResultItem {
                id: Some("result-1".into()),
                title: "Example".into(),
                url: "https://example.com".into(),
                description: Some("A result".into()),
                source: Some("Test".into()),
                thumbnail: None,
            }],
            media: vec![ContentBlock::Image {
                media: media_reference("https://cdn.example.com/image.jpg"),
                alt: Some("Example image".into()),
            }],
        };

        let presentation = response.into_presentation("example");

        assert_eq!(presentation.blocks.len(), 1);
        assert!(matches!(
            presentation.blocks[0],
            ContentBlock::SearchResults { .. }
        ));
    }

    #[test]
    fn test_parse_searxng_results_returns_direct_media_urls() {
        let tool = WebSearchTool::new("searxng".to_string(), None, 5, 15);
        let json = serde_json::json!({
            "results": [{
                "title": "Media result",
                "url": "https://example.com/article",
                "content": "An image, audio, and video result",
                "img_src": "https://cdn.example.com/image.jpg",
                "thumbnail_src": "https://cdn.example.com/thumb.jpg",
                "audio_src": "https://cdn.example.com/audio.mp3",
                "video_src": "https://cdn.example.com/video.mp4"
            }]
        });

        let result = tool.parse_searxng_results(&json, "test").unwrap();
        assert_eq!(result.media.len(), 3);
        assert!(matches!(result.media[0], ContentBlock::Image { .. }));
        assert!(matches!(result.media[1], ContentBlock::Audio { .. }));
        assert!(matches!(result.media[2], ContentBlock::Video { .. }));
    }

    #[test]
    fn test_parse_searxng_results_invalid_response() {
        let tool = WebSearchTool::new("searxng".to_string(), None, 5, 15);
        let json = serde_json::json!({"error": "bad request"});
        let result = tool.parse_searxng_results(&json, "test");
        assert!(result.is_err());
    }
}
