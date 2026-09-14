use async_trait::async_trait;
use clawseed_api::tool::{Tool, ToolResult};
use clawseed_api::tool_context::ToolContext;
use ego_tree::NodeRef;
use futures_util::StreamExt;
use scraper::{ElementRef, Html, Selector, node::Node};
use serde::Serialize;
use serde_json::json;
use std::time::Duration;

/// Default maximum response size (1 MB).
const _DEFAULT_MAX_RESPONSE_SIZE: usize = 1_048_576;
/// Default timeout in seconds.
const _DEFAULT_TIMEOUT_SECS: u64 = 30;
const DEFAULT_PAGE_CHARS: usize = 12_000;
const MAX_PAGE_CHARS: usize = 50_000;
const MIN_ARTICLE_CHARS: usize = 80;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FetchMode {
    Article,
    Text,
}

impl FetchMode {
    fn parse(value: Option<&str>) -> Result<Self, FetchFailure> {
        match value.unwrap_or("article") {
            "article" => Ok(Self::Article),
            "text" => Ok(Self::Text),
            value => Err(FetchFailure::new(
                "parse_failed",
                format!("Unsupported mode '{value}'; expected 'article' or 'text'"),
            )),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Article => "article",
            Self::Text => "text",
        }
    }
}

#[derive(Debug)]
struct FetchFailure {
    code: &'static str,
    message: String,
}

impl FetchFailure {
    fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    fn into_tool_result(self) -> ToolResult {
        ToolResult {
            success: false,
            output: String::new(),
            error: Some(
                json!({
                    "code": self.code,
                    "message": self.message,
                })
                .to_string(),
            ),
            presentation: None,
        }
    }
}

#[derive(Debug, Serialize)]
struct FetchOutput<'a> {
    content: &'a str,
    title: Option<&'a str>,
    mode: &'static str,
    start_index: usize,
    next_start_index: usize,
    has_more: bool,
}

#[derive(Debug)]
struct ExtractedPage {
    content: String,
    title: Option<String>,
    mode: FetchMode,
}

/// Web fetch tool: fetches a web page and converts HTML to plain text for LLM consumption.
///
/// Unlike `http_request` (an API client returning raw responses), this tool:
/// - Only supports GET
/// - Follows redirects (up to 10)
/// - Parses HTML into a DOM and extracts article or visible text
/// - Passes through text/plain, text/markdown, and application/json as-is
/// - Sets a descriptive User-Agent
pub struct WebFetchTool {
    allowed_domains: Vec<String>,
    blocked_domains: Vec<String>,
    allowed_private_hosts: Vec<String>,
    max_response_size: usize,
    timeout_secs: u64,
}

impl WebFetchTool {
    pub fn new(
        allowed_domains: Vec<String>,
        blocked_domains: Vec<String>,
        max_response_size: usize,
        timeout_secs: u64,
        allowed_private_hosts: Vec<String>,
    ) -> Self {
        Self {
            allowed_domains: normalize_allowed_domains(allowed_domains),
            blocked_domains: normalize_allowed_domains(blocked_domains),
            allowed_private_hosts: normalize_allowed_domains(allowed_private_hosts),
            max_response_size,
            timeout_secs,
        }
    }

    fn validate_url(&self, raw_url: &str) -> anyhow::Result<String> {
        validate_target_url(
            raw_url,
            &self.allowed_domains,
            &self.blocked_domains,
            &self.allowed_private_hosts,
            "web_fetch",
        )
    }

    async fn read_response_text_limited(
        &self,
        response: reqwest::Response,
    ) -> Result<String, FetchFailure> {
        if response
            .content_length()
            .is_some_and(|length| length > self.max_response_size as u64)
        {
            return Err(FetchFailure::new(
                "response_too_large",
                format!(
                    "Response exceeds the configured {} byte download limit",
                    self.max_response_size
                ),
            ));
        }

        let mut bytes_stream = response.bytes_stream();
        let hard_cap = self.max_response_size.saturating_add(1);
        let mut bytes = Vec::new();

        while let Some(chunk_result) = bytes_stream.next().await {
            let chunk = chunk_result.map_err(|error| {
                FetchFailure::new("network", format!("Failed to read response body: {error}"))
            })?;
            if append_chunk_with_cap(&mut bytes, &chunk, hard_cap) {
                break;
            }
        }

        if bytes.len() > self.max_response_size {
            return Err(FetchFailure::new(
                "response_too_large",
                format!(
                    "Response exceeds the configured {} byte download limit",
                    self.max_response_size
                ),
            ));
        }

        Ok(String::from_utf8_lossy(&bytes).into_owned())
    }

    async fn standard_fetch(
        &self,
        client: &reqwest::Client,
        url: &str,
        requested_mode: FetchMode,
        start_index: usize,
        max_chars: usize,
    ) -> ToolResult {
        let response = match client.get(url).send().await {
            Ok(r) => r,
            Err(e) => {
                let error = e.to_string();
                let code = if error.contains("Blocked redirect target") {
                    "blocked_url"
                } else {
                    "network"
                };
                return FetchFailure::new(code, format!("HTTP request failed: {e}"))
                    .into_tool_result();
            }
        };

        let status = response.status();
        if !status.is_success() {
            return FetchFailure::new(
                "http_status",
                format!(
                    "HTTP {} {}",
                    status.as_u16(),
                    status.canonical_reason().unwrap_or("Unknown")
                ),
            )
            .into_tool_result();
        }

        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_lowercase();

        let body_mode = if content_type.contains("text/html") || content_type.is_empty() {
            "html"
        } else if content_type.contains("text/plain")
            || content_type.contains("text/markdown")
            || content_type.contains("application/json")
        {
            "plain"
        } else {
            return FetchFailure::new(
                "unsupported_content",
                format!(
                    "Unsupported content type: {content_type}. \
                     web_fetch supports text/html, text/plain, text/markdown, and application/json."
                ),
            )
            .into_tool_result();
        };

        let body = match self.read_response_text_limited(response).await {
            Ok(t) => t,
            Err(error) => return error.into_tool_result(),
        };

        let extracted = if body_mode == "html" {
            match extract_html(&body, requested_mode) {
                Ok(page) => page,
                Err(error) => return error.into_tool_result(),
            }
        } else {
            let content = clean_text(&body);
            if content.is_empty() {
                return FetchFailure::new("parse_failed", "Response contained no readable text")
                    .into_tool_result();
            }
            ExtractedPage {
                content,
                title: None,
                mode: FetchMode::Text,
            }
        };

        let (content, next_start_index, has_more) =
            match paginate_text(&extracted.content, start_index, max_chars) {
                Ok(page) => page,
                Err(error) => return error.into_tool_result(),
            };

        let output = FetchOutput {
            content: &content,
            title: extracted.title.as_deref(),
            mode: extracted.mode.as_str(),
            start_index,
            next_start_index,
            has_more,
        };

        ToolResult {
            success: true,
            output: serde_json::to_string(&output).expect("web_fetch output is serializable"),
            error: None,
            presentation: None,
        }
    }
}

#[async_trait]
impl Tool for WebFetchTool {
    fn name(&self) -> &str {
        "web_fetch"
    }

    fn description(&self) -> &str {
        "Fetch a web page as extracted article content or cleaned visible text. \
         Results use stable character-based pagination and include the title, actual mode, and next index. \
         Article extraction falls back to text mode when no reliable article is found. \
         Only GET requests; follows redirects. \
         Security: allowlist-only domains, no local/private hosts."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "url": {
                    "type": "string",
                    "description": "The HTTP or HTTPS URL to fetch"
                },
                "mode": {
                    "type": "string",
                    "enum": ["article", "text"],
                    "default": "article",
                    "description": "article extracts the main content and falls back to text; text returns cleaned visible page text"
                },
                "start_index": {
                    "type": "integer",
                    "minimum": 0,
                    "default": 0,
                    "description": "Unicode character index in the cleaned text"
                },
                "max_chars": {
                    "type": "integer",
                    "minimum": 1,
                    "maximum": MAX_PAGE_CHARS,
                    "default": DEFAULT_PAGE_CHARS,
                    "description": "Maximum Unicode characters to return"
                }
            },
            "additionalProperties": false,
            "required": ["url"]
        })
    }

    async fn execute(
        &self,
        args: serde_json::Value,
        _ctx: &dyn ToolContext,
    ) -> anyhow::Result<ToolResult> {
        let url = args
            .get("url")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing 'url' parameter"))?;

        let url = match self.validate_url(url) {
            Ok(v) => v,
            Err(e) => {
                return Ok(FetchFailure::new("blocked_url", e.to_string()).into_tool_result());
            }
        };

        let mode_arg = match args.get("mode") {
            Some(value) => match value.as_str() {
                Some(value) => Some(value),
                None => {
                    return Ok(FetchFailure::new(
                        "parse_failed",
                        "'mode' must be 'article' or 'text'",
                    )
                    .into_tool_result());
                }
            },
            None => None,
        };
        let requested_mode = match FetchMode::parse(mode_arg) {
            Ok(mode) => mode,
            Err(error) => return Ok(error.into_tool_result()),
        };
        let start_index = match parse_usize_arg(&args, "start_index", 0, usize::MAX) {
            Ok(value) => value,
            Err(error) => return Ok(error.into_tool_result()),
        };
        let max_chars =
            match parse_usize_arg(&args, "max_chars", DEFAULT_PAGE_CHARS, MAX_PAGE_CHARS) {
                Ok(0) => {
                    return Ok(FetchFailure::new(
                        "parse_failed",
                        "max_chars must be greater than zero",
                    )
                    .into_tool_result());
                }
                Ok(value) => value,
                Err(error) => return Ok(error.into_tool_result()),
            };

        // Build client: follow redirects, set timeout, set User-Agent
        let timeout_secs = if self.timeout_secs == 0 {
            tracing::warn!("web_fetch: timeout_secs is 0, using safe default of 30s");
            30
        } else {
            self.timeout_secs
        };

        let allowed_domains = self.allowed_domains.clone();
        let blocked_domains = self.blocked_domains.clone();
        let allowed_private_hosts = self.allowed_private_hosts.clone();
        let redirect_policy = reqwest::redirect::Policy::custom(move |attempt| {
            if attempt.previous().len() >= 10 {
                return attempt.error(std::io::Error::other("Too many redirects (max 10)"));
            }

            if let Err(err) = validate_target_url(
                attempt.url().as_str(),
                &allowed_domains,
                &blocked_domains,
                &allowed_private_hosts,
                "web_fetch",
            ) {
                return attempt.error(std::io::Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    format!("Blocked redirect target: {err}"),
                ));
            }

            attempt.follow()
        });

        let builder = reqwest::Client::builder()
            .timeout(Duration::from_secs(timeout_secs))
            .connect_timeout(Duration::from_secs(10))
            .redirect(redirect_policy)
            .user_agent("ClawSeed/0.1 (web_fetch)");
        let builder =
            clawseed_config::schema::apply_runtime_proxy_to_builder(builder, "tool.web_fetch");
        let client = match builder.build() {
            Ok(c) => c,
            Err(e) => {
                return Ok(ToolResult {
                    success: false,
                    output: String::new(),
                    error: Some(format!("Failed to build HTTP client: {e}")),
                    presentation: None,
                });
            }
        };

        let standard_result = self
            .standard_fetch(&client, &url, requested_mode, start_index, max_chars)
            .await;
        Ok(standard_result)
    }
}

// ── Helper functions ──

fn parse_usize_arg(
    args: &serde_json::Value,
    name: &str,
    default: usize,
    maximum: usize,
) -> Result<usize, FetchFailure> {
    let Some(value) = args.get(name) else {
        return Ok(default);
    };
    let Some(value) = value.as_u64() else {
        return Err(FetchFailure::new(
            "parse_failed",
            format!("'{name}' must be a non-negative integer"),
        ));
    };
    let value = usize::try_from(value)
        .map_err(|_| FetchFailure::new("parse_failed", format!("'{name}' is too large")))?;
    if value > maximum {
        return Err(FetchFailure::new(
            "parse_failed",
            format!("'{name}' must not exceed {maximum}"),
        ));
    }
    Ok(value)
}

fn extract_html(html: &str, requested_mode: FetchMode) -> Result<ExtractedPage, FetchFailure> {
    let document = Html::parse_document(html);
    let title = extract_title(&document);
    let root = select_first(&document, "body").unwrap_or_else(|| document.root_element());
    let visible_text = extract_element_text(root);

    if visible_text.is_empty() {
        let script_count = document
            .select(&Selector::parse("script").expect("static selector is valid"))
            .count();
        let message = if script_count > 0 {
            "Page has no readable HTML text and may require JavaScript rendering; browser rendering is not supported"
        } else {
            "Page contained no readable HTML text"
        };
        return Err(FetchFailure::new("parse_failed", message));
    }

    if requested_mode == FetchMode::Article
        && let Some(article) = extract_article(&document)
    {
        return Ok(ExtractedPage {
            content: article,
            title,
            mode: FetchMode::Article,
        });
    }

    Ok(ExtractedPage {
        content: visible_text,
        title,
        mode: FetchMode::Text,
    })
}

fn extract_title(document: &Html) -> Option<String> {
    let metadata = Selector::parse(
        "meta[property='og:title'], meta[name='twitter:title'], meta[name='title']",
    )
    .expect("static selector is valid");
    for element in document.select(&metadata) {
        if let Some(content) = element.value().attr("content") {
            let title = clean_inline_text(content);
            if !title.is_empty() {
                return Some(title);
            }
        }
    }

    for selector in ["title", "h1"] {
        if let Some(element) = select_first(document, selector) {
            let title = clean_inline_text(&extract_element_text(element));
            if !title.is_empty() {
                return Some(title);
            }
        }
    }
    None
}

fn extract_article(document: &Html) -> Option<String> {
    let selector = Selector::parse(
        "article, main, [role='main'], [itemprop='articleBody'], \
         .article-body, .article-content, .post-content, .entry-content, .story-body, \
         #article-body, #article-content, #main-content, \
         [class*='article'], [class*='post-content'], [class*='entry-content'], \
         [id*='article'], [id*='main-content']",
    )
    .expect("static selector is valid");
    let link_selector = Selector::parse("a").expect("static selector is valid");
    let paragraph_selector = Selector::parse("p").expect("static selector is valid");

    document
        .select(&selector)
        .filter_map(|element| {
            let text = extract_element_text(element);
            let text_chars = text.chars().count();
            if text_chars < MIN_ARTICLE_CHARS {
                return None;
            }

            let link_chars = element
                .select(&link_selector)
                .map(extract_element_text)
                .map(|text| text.chars().count())
                .sum::<usize>();
            let paragraphs = element.select(&paragraph_selector).count();
            let marker = format!(
                "{} {}",
                element.value().id().unwrap_or_default(),
                element.value().classes().collect::<Vec<_>>().join(" ")
            )
            .to_ascii_lowercase();
            let boilerplate_penalty = if [
                "comment",
                "footer",
                "nav",
                "recommend",
                "related",
                "share",
                "sidebar",
            ]
            .iter()
            .any(|word| marker.contains(word))
            {
                text_chars
            } else {
                0
            };
            let score = text_chars
                .saturating_add(paragraphs.saturating_mul(80))
                .saturating_sub(link_chars.saturating_mul(2))
                .saturating_sub(boilerplate_penalty);
            Some((score, text))
        })
        .max_by_key(|(score, _)| *score)
        .map(|(_, text)| text)
}

fn select_first<'a>(document: &'a Html, selector: &str) -> Option<ElementRef<'a>> {
    let selector = Selector::parse(selector).expect("static selector is valid");
    document.select(&selector).next()
}

#[derive(Default)]
struct TextCollector {
    output: String,
    pending_space: bool,
}

impl TextCollector {
    fn text(&mut self, value: &str) {
        for character in value.chars() {
            if character.is_whitespace() {
                self.pending_space = true;
            } else {
                if self.pending_space
                    && !self.output.is_empty()
                    && !self.output.ends_with([' ', '\n'])
                {
                    self.output.push(' ');
                }
                self.output.push(character);
                self.pending_space = false;
            }
        }
    }

    fn line_break(&mut self) {
        while self.output.ends_with(' ') {
            self.output.pop();
        }
        if !self.output.is_empty() && !self.output.ends_with('\n') {
            self.output.push('\n');
        }
        self.pending_space = false;
    }
}

fn extract_element_text(element: ElementRef<'_>) -> String {
    let mut collector = TextCollector::default();
    collect_node_text(*element, &mut collector);
    clean_text(&collector.output)
}

fn collect_node_text(node: NodeRef<'_, Node>, collector: &mut TextCollector) {
    if let Some(element) = node.value().as_element() {
        if should_skip_element(element) {
            return;
        }
        let is_block = is_block_element(element.name());
        if is_block {
            collector.line_break();
        }
        if element.name() == "br" {
            collector.line_break();
        } else {
            for child in node.children() {
                collect_node_text(child, collector);
            }
        }
        if is_block {
            collector.line_break();
        }
    } else if let Some(text) = node.value().as_text() {
        collector.text(text);
    } else {
        for child in node.children() {
            collect_node_text(child, collector);
        }
    }
}

fn should_skip_element(element: &scraper::node::Element) -> bool {
    const SKIPPED: &[&str] = &[
        "script", "style", "noscript", "template", "svg", "canvas", "iframe", "object", "nav",
        "footer", "aside", "form",
    ];
    if SKIPPED.contains(&element.name())
        || element.attr("hidden").is_some()
        || element
            .attr("aria-hidden")
            .is_some_and(|value| value.eq_ignore_ascii_case("true"))
    {
        return true;
    }

    element.attr("style").is_some_and(|style| {
        let style = style.to_ascii_lowercase().replace(' ', "");
        style.contains("display:none") || style.contains("visibility:hidden")
    })
}

fn is_block_element(name: &str) -> bool {
    matches!(
        name,
        "address"
            | "article"
            | "blockquote"
            | "dd"
            | "div"
            | "dl"
            | "dt"
            | "figcaption"
            | "figure"
            | "h1"
            | "h2"
            | "h3"
            | "h4"
            | "h5"
            | "h6"
            | "header"
            | "li"
            | "main"
            | "p"
            | "pre"
            | "section"
            | "table"
            | "td"
            | "th"
            | "tr"
            | "ul"
            | "ol"
    )
}

fn clean_inline_text(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn clean_text(text: &str) -> String {
    text.lines()
        .map(clean_inline_text)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

fn paginate_text(
    text: &str,
    start_index: usize,
    max_chars: usize,
) -> Result<(String, usize, bool), FetchFailure> {
    let total_chars = text.chars().count();
    if start_index > total_chars {
        return Err(FetchFailure::new(
            "parse_failed",
            format!("start_index {start_index} exceeds content length {total_chars}"),
        ));
    }

    let end_index = start_index.saturating_add(max_chars).min(total_chars);
    let content = text
        .chars()
        .skip(start_index)
        .take(end_index - start_index)
        .collect();
    Ok((content, end_index, end_index < total_chars))
}

fn validate_target_url(
    raw_url: &str,
    allowed_domains: &[String],
    blocked_domains: &[String],
    allowed_private_hosts: &[String],
    tool_name: &str,
) -> anyhow::Result<String> {
    let url = raw_url.trim();

    if url.is_empty() {
        anyhow::bail!("URL cannot be empty");
    }

    if url.chars().any(char::is_whitespace) {
        anyhow::bail!("URL cannot contain whitespace");
    }

    if !url.starts_with("http://") && !url.starts_with("https://") {
        anyhow::bail!("Only http:// and https:// URLs are allowed");
    }

    if allowed_domains.is_empty() {
        anyhow::bail!(
            "{tool_name} tool is enabled but no allowed_domains are configured. \
             Add [{tool_name}].allowed_domains in config.toml"
        );
    }

    let host = extract_host(url)?;

    // blocked_domains always takes precedence
    if host_matches_allowlist(&host, blocked_domains) {
        anyhow::bail!("Host '{host}' is in {tool_name}.blocked_domains");
    }

    let private_host_allowed =
        is_private_or_local_host(&host) && host_matches_allowlist(&host, allowed_private_hosts);

    if is_private_or_local_host(&host) && !private_host_allowed {
        anyhow::bail!(
            "Blocked local/private host: {host}. \
             To allow this host, add it to {tool_name}.allowed_private_hosts in config.toml"
        );
    }

    if private_host_allowed {
        tracing::warn!(
            "{tool_name}: allowing private/local host '{host}' via allowed_private_hosts"
        );
    }

    if !private_host_allowed && !host_matches_allowlist(&host, allowed_domains) {
        anyhow::bail!("Host '{host}' is not in {tool_name}.allowed_domains");
    }

    if !private_host_allowed {
        validate_resolved_host_is_public(&host)?;
    }

    Ok(url.to_string())
}

fn append_chunk_with_cap(buffer: &mut Vec<u8>, chunk: &[u8], hard_cap: usize) -> bool {
    if buffer.len() >= hard_cap {
        return true;
    }

    let remaining = hard_cap - buffer.len();
    if chunk.len() > remaining {
        buffer.extend_from_slice(&chunk[..remaining]);
        return true;
    }

    buffer.extend_from_slice(chunk);
    buffer.len() >= hard_cap
}

fn normalize_allowed_domains(domains: Vec<String>) -> Vec<String> {
    let mut normalized = domains
        .into_iter()
        .filter_map(|d| normalize_domain(&d))
        .collect::<Vec<_>>();
    normalized.sort_unstable();
    normalized.dedup();
    normalized
}

fn normalize_domain(raw: &str) -> Option<String> {
    let mut d = raw.trim().to_lowercase();
    if d.is_empty() {
        return None;
    }

    if let Some(stripped) = d.strip_prefix("https://") {
        d = stripped.to_string();
    } else if let Some(stripped) = d.strip_prefix("http://") {
        d = stripped.to_string();
    }

    if let Some((host, _)) = d.split_once('/') {
        d = host.to_string();
    }

    d = d.trim_start_matches('.').trim_end_matches('.').to_string();

    if let Some((host, _)) = d.split_once(':') {
        d = host.to_string();
    }

    if d.is_empty() || d.chars().any(char::is_whitespace) {
        return None;
    }

    Some(d)
}

fn extract_host(url: &str) -> anyhow::Result<String> {
    let rest = url
        .strip_prefix("http://")
        .or_else(|| url.strip_prefix("https://"))
        .ok_or_else(|| anyhow::anyhow!("Only http:// and https:// URLs are allowed"))?;

    let authority = rest
        .split(['/', '?', '#'])
        .next()
        .ok_or_else(|| anyhow::anyhow!("Invalid URL"))?;

    if authority.is_empty() {
        anyhow::bail!("URL must include a host");
    }

    if authority.contains('@') {
        anyhow::bail!("URL userinfo is not allowed");
    }

    if authority.starts_with('[') {
        anyhow::bail!("IPv6 hosts are not supported in web_fetch");
    }

    let host = authority
        .split(':')
        .next()
        .unwrap_or_default()
        .trim()
        .trim_end_matches('.')
        .to_lowercase();

    if host.is_empty() {
        anyhow::bail!("URL must include a valid host");
    }

    Ok(host)
}

fn host_matches_allowlist(host: &str, allowed_domains: &[String]) -> bool {
    if allowed_domains.iter().any(|domain| domain == "*") {
        return true;
    }

    allowed_domains.iter().any(|domain| {
        host == domain
            || host
                .strip_suffix(domain)
                .is_some_and(|prefix| prefix.ends_with('.'))
    })
}

fn is_private_or_local_host(host: &str) -> bool {
    let bare = host
        .strip_prefix('[')
        .and_then(|h| h.strip_suffix(']'))
        .unwrap_or(host);

    let has_local_tld = bare
        .rsplit('.')
        .next()
        .is_some_and(|label| label == "local");

    if bare == "localhost" || bare.ends_with(".localhost") || has_local_tld {
        return true;
    }

    if let Ok(ip) = bare.parse::<std::net::IpAddr>() {
        return match ip {
            std::net::IpAddr::V4(v4) => is_non_global_v4(v4),
            std::net::IpAddr::V6(v6) => is_non_global_v6(v6),
        };
    }

    false
}

#[cfg(not(test))]
fn validate_resolved_host_is_public(host: &str) -> anyhow::Result<()> {
    use std::net::ToSocketAddrs;

    let ips = (host, 0)
        .to_socket_addrs()
        .map_err(|e| anyhow::anyhow!("Failed to resolve host '{host}': {e}"))?
        .map(|addr| addr.ip())
        .collect::<Vec<_>>();

    validate_resolved_ips_are_public(host, &ips)
}

#[cfg(test)]
fn validate_resolved_host_is_public(_host: &str) -> anyhow::Result<()> {
    // DNS checks are covered by validate_resolved_ips_are_public unit tests.
    Ok(())
}

fn validate_resolved_ips_are_public(host: &str, ips: &[std::net::IpAddr]) -> anyhow::Result<()> {
    if ips.is_empty() {
        anyhow::bail!("Failed to resolve host '{host}'");
    }

    for ip in ips {
        let non_global = match ip {
            std::net::IpAddr::V4(v4) => is_non_global_v4(*v4),
            std::net::IpAddr::V6(v6) => is_non_global_v6(*v6),
        };
        if non_global {
            anyhow::bail!("Blocked host '{host}' resolved to non-global address {ip}");
        }
    }

    Ok(())
}

fn is_non_global_v4(v4: std::net::Ipv4Addr) -> bool {
    let [a, b, c, _] = v4.octets();
    v4.is_loopback()
        || v4.is_private()
        || v4.is_link_local()
        || v4.is_unspecified()
        || v4.is_broadcast()
        || v4.is_multicast()
        || (a == 100 && (64..=127).contains(&b))
        || a >= 240
        || (a == 192 && b == 0 && (c == 0 || c == 2))
        || (a == 198 && b == 51)
        || (a == 203 && b == 0)
        || (a == 198 && (18..=19).contains(&b))
}

fn is_non_global_v6(v6: std::net::Ipv6Addr) -> bool {
    let segs = v6.segments();
    v6.is_loopback()
        || v6.is_unspecified()
        || v6.is_multicast()
        || (segs[0] & 0xfe00) == 0xfc00
        || (segs[0] & 0xffc0) == 0xfe80
        || (segs[0] == 0x2001 && segs[1] == 0x0db8)
        || v6.to_ipv4_mapped().is_some_and(is_non_global_v4)
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::{Mock, MockServer, ResponseTemplate, matchers::method};

    fn test_tool(allowed_domains: Vec<&str>) -> WebFetchTool {
        test_tool_with_blocklist(allowed_domains, vec![])
    }

    fn test_tool_with_blocklist(
        allowed_domains: Vec<&str>,
        blocked_domains: Vec<&str>,
    ) -> WebFetchTool {
        WebFetchTool::new(
            allowed_domains.into_iter().map(String::from).collect(),
            blocked_domains.into_iter().map(String::from).collect(),
            500_000,
            30,
            vec![],
        )
    }

    fn test_tool_with_private_hosts(
        allowed_domains: Vec<&str>,
        blocked_domains: Vec<&str>,
        allowed_private_hosts: Vec<&str>,
    ) -> WebFetchTool {
        WebFetchTool::new(
            allowed_domains.into_iter().map(String::from).collect(),
            blocked_domains.into_iter().map(String::from).collect(),
            500_000,
            30,
            allowed_private_hosts
                .into_iter()
                .map(String::from)
                .collect(),
        )
    }

    // ── Name and schema ──────────────────────────────────────────

    #[test]
    fn name_is_web_fetch() {
        let tool = test_tool(vec!["example.com"]);
        assert_eq!(tool.name(), "web_fetch");
    }

    #[test]
    fn parameters_schema_requires_url() {
        let tool = test_tool(vec!["example.com"]);
        let schema = tool.parameters_schema();
        assert!(schema["properties"]["url"].is_object());
        assert_eq!(schema["properties"]["mode"]["default"], "article");
        assert_eq!(schema["properties"]["start_index"]["minimum"], 0);
        assert_eq!(schema["properties"]["max_chars"]["maximum"], MAX_PAGE_CHARS);
        let required = schema["required"].as_array().unwrap();
        assert!(required.iter().any(|v| v.as_str() == Some("url")));
    }

    // ── HTML to text conversion ──────────────────────────────────

    #[test]
    fn article_mode_extracts_main_content_and_removes_noise() {
        let html = r#"
            <html>
              <head><title>Document title</title><script>tracking()</script></head>
              <body>
                <nav>Home Products Pricing</nav>
                <main>
                  <article>
                    <h1>Article heading</h1>
                    <p>This is the first paragraph with enough useful article text for extraction.</p>
                    <p>This is the second paragraph, with more details and a stable conclusion.</p>
                    <aside>Related advertisement</aside>
                  </article>
                </main>
                <footer>Copyright and privacy links</footer>
              </body>
            </html>
        "#;

        let page = extract_html(html, FetchMode::Article).unwrap();
        assert_eq!(page.mode, FetchMode::Article);
        assert_eq!(page.title.as_deref(), Some("Document title"));
        assert!(page.content.contains("Article heading"));
        assert!(page.content.contains("first paragraph"));
        assert!(!page.content.contains("Home Products"));
        assert!(!page.content.contains("advertisement"));
        assert!(!page.content.contains("Copyright"));
        assert!(!page.content.contains("tracking"));
    }

    #[test]
    fn article_mode_falls_back_to_actual_text_mode() {
        let html = r#"
            <html><head><meta property="og:title" content="Fallback title"></head>
            <body><div>A short page without a reliable article container.</div></body></html>
        "#;

        let page = extract_html(html, FetchMode::Article).unwrap();
        assert_eq!(page.mode, FetchMode::Text);
        assert_eq!(page.title.as_deref(), Some("Fallback title"));
        assert_eq!(
            page.content,
            "A short page without a reliable article container."
        );
    }

    #[test]
    fn text_mode_omits_hidden_and_non_content_elements() {
        let html = r#"
            <body>
              <nav>Navigation</nav>
              <p>Visible <strong>content</strong>.</p>
              <p hidden>Hidden attribute</p>
              <p aria-hidden="true">ARIA hidden</p>
              <p style="display: none">CSS hidden</p>
              <form>Form controls</form>
            </body>
        "#;

        let page = extract_html(html, FetchMode::Text).unwrap();
        assert_eq!(page.mode, FetchMode::Text);
        assert_eq!(page.content, "Visible content.");
    }

    #[test]
    fn javascript_only_page_returns_rendering_hint() {
        let error = extract_html(
            "<html><body><div id='root'></div><script>renderApp()</script></body></html>",
            FetchMode::Article,
        )
        .unwrap_err();
        assert_eq!(error.code, "parse_failed");
        assert!(error.message.contains("JavaScript rendering"));
    }

    #[test]
    fn pagination_is_stable_for_ascii_and_multibyte_text() {
        let (first, next, has_more) = paginate_text("abcdef", 0, 3).unwrap();
        assert_eq!(first, "abc");
        assert_eq!(next, 3);
        assert!(has_more);

        let (second, next, has_more) = paginate_text("abcdef", next, 3).unwrap();
        assert_eq!(second, "def");
        assert_eq!(next, 6);
        assert!(!has_more);

        let (unicode, next, has_more) = paginate_text("甲乙🙂丁", 1, 2).unwrap();
        assert_eq!(unicode, "乙🙂");
        assert_eq!(next, 3);
        assert!(has_more);
    }

    #[test]
    fn pagination_rejects_index_past_content() {
        let error = paginate_text("abc", 4, 1).unwrap_err();
        assert_eq!(error.code, "parse_failed");
        assert!(error.message.contains("content length 3"));
    }

    // ── URL validation ───────────────────────────────────────────

    #[test]
    fn validate_accepts_exact_domain() {
        let tool = test_tool(vec!["example.com"]);
        let got = tool.validate_url("https://example.com/page").unwrap();
        assert_eq!(got, "https://example.com/page");
    }

    #[test]
    fn validate_accepts_subdomain() {
        let tool = test_tool(vec!["example.com"]);
        assert!(tool.validate_url("https://docs.example.com/guide").is_ok());
    }

    #[test]
    fn validate_accepts_wildcard() {
        let tool = test_tool(vec!["*"]);
        assert!(tool.validate_url("https://news.ycombinator.com").is_ok());
    }

    #[test]
    fn validate_rejects_empty_url() {
        let tool = test_tool(vec!["example.com"]);
        let err = tool.validate_url("").unwrap_err().to_string();
        assert!(err.contains("empty"));
    }

    #[test]
    fn validate_rejects_missing_url() {
        let tool = test_tool(vec!["example.com"]);
        let err = tool.validate_url("  ").unwrap_err().to_string();
        assert!(err.contains("empty"));
    }

    #[test]
    fn validate_rejects_ftp_scheme() {
        let tool = test_tool(vec!["example.com"]);
        let err = tool
            .validate_url("ftp://example.com")
            .unwrap_err()
            .to_string();
        assert!(err.contains("http://") || err.contains("https://"));
    }

    #[test]
    fn validate_rejects_allowlist_miss() {
        let tool = test_tool(vec!["example.com"]);
        let err = tool
            .validate_url("https://google.com")
            .unwrap_err()
            .to_string();
        assert!(err.contains("allowed_domains"));
    }

    #[test]
    fn validate_requires_allowlist() {
        let tool = WebFetchTool::new(vec![], vec![], 500_000, 30, vec![]);
        let err = tool
            .validate_url("https://example.com")
            .unwrap_err()
            .to_string();
        assert!(err.contains("allowed_domains"));
    }

    // ── SSRF protection ──────────────────────────────────────────

    #[test]
    fn ssrf_blocks_localhost() {
        let tool = test_tool(vec!["localhost"]);
        let err = tool
            .validate_url("https://localhost:8080")
            .unwrap_err()
            .to_string();
        assert!(err.contains("local/private"));
    }

    #[test]
    fn ssrf_blocks_private_ipv4() {
        let tool = test_tool(vec!["192.168.1.5"]);
        let err = tool
            .validate_url("https://192.168.1.5")
            .unwrap_err()
            .to_string();
        assert!(err.contains("local/private"));
    }

    #[test]
    fn ssrf_blocks_loopback() {
        assert!(is_private_or_local_host("127.0.0.1"));
        assert!(is_private_or_local_host("127.0.0.2"));
    }

    #[test]
    fn ssrf_blocks_rfc1918() {
        assert!(is_private_or_local_host("10.0.0.1"));
        assert!(is_private_or_local_host("172.16.0.1"));
        assert!(is_private_or_local_host("192.168.1.1"));
    }

    #[test]
    fn ssrf_wildcard_still_blocks_private() {
        let tool = test_tool(vec!["*"]);
        let err = tool
            .validate_url("https://localhost:8080")
            .unwrap_err()
            .to_string();
        assert!(err.contains("local/private"));
    }

    #[test]
    fn redirect_target_validation_allows_permitted_host() {
        let allowed = vec!["example.com".to_string()];
        let blocked = vec![];
        assert!(
            validate_target_url(
                "https://docs.example.com/page",
                &allowed,
                &blocked,
                &[],
                "web_fetch"
            )
            .is_ok()
        );
    }

    #[test]
    fn redirect_target_validation_blocks_private_host() {
        let allowed = vec!["example.com".to_string()];
        let blocked = vec![];
        let err = validate_target_url(
            "https://127.0.0.1/admin",
            &allowed,
            &blocked,
            &[],
            "web_fetch",
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("local/private"));
    }

    #[test]
    fn redirect_target_validation_blocks_blocklisted_host() {
        let allowed = vec!["*".to_string()];
        let blocked = vec!["evil.com".to_string()];
        let err = validate_target_url(
            "https://evil.com/phish",
            &allowed,
            &blocked,
            &[],
            "web_fetch",
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("blocked_domains"));
    }

    // ── Response limits and structured output ───────────────────

    #[tokio::test]
    async fn response_larger_than_download_limit_is_rejected() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/plain")
                    .set_body_string("0123456789"),
            )
            .mount(&server)
            .await;
        let tool = WebFetchTool::new(vec!["*".into()], vec![], 5, 30, vec![]);
        let response = reqwest::Client::new()
            .get(server.uri())
            .send()
            .await
            .unwrap();

        let error = tool.read_response_text_limited(response).await.unwrap_err();
        assert_eq!(error.code, "response_too_large");
    }

    #[tokio::test]
    async fn standard_fetch_returns_structured_page_metadata() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_bytes(
                        "<html><head><title>Page</title></head><body><p>甲乙🙂丁戊</p></body></html>"
                            .as_bytes(),
                    )
                    .insert_header("content-type", "text/html"),
            )
            .mount(&server)
            .await;
        let tool = test_tool(vec!["*"]);

        let result = tool
            .standard_fetch(
                &reqwest::Client::new(),
                &server.uri(),
                FetchMode::Text,
                1,
                2,
            )
            .await;
        assert!(result.success);
        let output: serde_json::Value = serde_json::from_str(&result.output).unwrap();
        assert_eq!(output["content"], "乙🙂");
        assert_eq!(output["title"], "Page");
        assert_eq!(output["mode"], "text");
        assert_eq!(output["start_index"], 1);
        assert_eq!(output["next_start_index"], 3);
        assert_eq!(output["has_more"], true);
    }

    #[tokio::test]
    async fn http_status_uses_fixed_error_code() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(503))
            .mount(&server)
            .await;
        let tool = test_tool(vec!["*"]);

        let result = tool
            .standard_fetch(
                &reqwest::Client::new(),
                &server.uri(),
                FetchMode::Article,
                0,
                DEFAULT_PAGE_CHARS,
            )
            .await;
        assert!(!result.success);
        let error: serde_json::Value =
            serde_json::from_str(result.error.as_deref().unwrap()).unwrap();
        assert_eq!(error["code"], "http_status");
    }

    // ── Domain normalization ─────────────────────────────────────

    #[test]
    fn normalize_domain_strips_scheme_and_case() {
        let got = normalize_domain("  HTTPS://Docs.Example.com/path ").unwrap();
        assert_eq!(got, "docs.example.com");
    }

    #[test]
    fn normalize_deduplicates() {
        let got = normalize_allowed_domains(vec![
            "example.com".into(),
            "EXAMPLE.COM".into(),
            "https://example.com/".into(),
        ]);
        assert_eq!(got, vec!["example.com".to_string()]);
    }

    // ── Blocked domains ──────────────────────────────────────────

    #[test]
    fn blocklist_rejects_exact_match() {
        let tool = test_tool_with_blocklist(vec!["*"], vec!["evil.com"]);
        let err = tool
            .validate_url("https://evil.com/page")
            .unwrap_err()
            .to_string();
        assert!(err.contains("blocked_domains"));
    }

    #[test]
    fn blocklist_rejects_subdomain() {
        let tool = test_tool_with_blocklist(vec!["*"], vec!["evil.com"]);
        let err = tool
            .validate_url("https://api.evil.com/v1")
            .unwrap_err()
            .to_string();
        assert!(err.contains("blocked_domains"));
    }

    #[test]
    fn blocklist_wins_over_allowlist() {
        let tool = test_tool_with_blocklist(vec!["evil.com"], vec!["evil.com"]);
        let err = tool
            .validate_url("https://evil.com")
            .unwrap_err()
            .to_string();
        assert!(err.contains("blocked_domains"));
    }

    #[test]
    fn blocklist_allows_non_blocked() {
        let tool = test_tool_with_blocklist(vec!["*"], vec!["evil.com"]);
        assert!(tool.validate_url("https://example.com").is_ok());
    }

    #[test]
    fn append_chunk_with_cap_truncates_and_stops() {
        let mut buffer = Vec::new();
        assert!(!append_chunk_with_cap(&mut buffer, b"hello", 8));
        assert!(append_chunk_with_cap(&mut buffer, b"world", 8));
        assert_eq!(buffer, b"hellowor");
    }

    #[test]
    fn resolved_private_ip_is_rejected() {
        let ips = vec!["127.0.0.1".parse().unwrap()];
        let err = validate_resolved_ips_are_public("example.com", &ips)
            .unwrap_err()
            .to_string();
        assert!(err.contains("non-global address"));
    }

    #[test]
    fn resolved_mixed_ips_are_rejected() {
        let ips = vec![
            "93.184.216.34".parse().unwrap(),
            "10.0.0.1".parse().unwrap(),
        ];
        let err = validate_resolved_ips_are_public("example.com", &ips)
            .unwrap_err()
            .to_string();
        assert!(err.contains("non-global address"));
    }

    #[test]
    fn resolved_public_ips_are_allowed() {
        let ips = vec!["93.184.216.34".parse().unwrap(), "1.1.1.1".parse().unwrap()];
        assert!(validate_resolved_ips_are_public("example.com", &ips).is_ok());
    }

    // ── Allowed private hosts ─────────────────────────────────────

    #[test]
    fn allowed_private_host_bypasses_ssrf_block() {
        let tool = test_tool_with_private_hosts(vec!["*"], vec![], vec!["192.168.1.5"]);
        assert!(tool.validate_url("https://192.168.1.5/api").is_ok());
    }

    #[test]
    fn unallowed_private_host_still_blocked() {
        let tool = test_tool_with_private_hosts(vec!["*"], vec![], vec!["192.168.1.5"]);
        let err = tool
            .validate_url("https://10.0.0.1/admin")
            .unwrap_err()
            .to_string();
        assert!(err.contains("local/private"));
        assert!(err.contains("allowed_private_hosts"));
    }

    #[test]
    fn blocklist_overrides_allowed_private_host() {
        let tool =
            test_tool_with_private_hosts(vec!["*"], vec!["192.168.1.5"], vec!["192.168.1.5"]);
        let err = tool
            .validate_url("https://192.168.1.5/secret")
            .unwrap_err()
            .to_string();
        assert!(err.contains("blocked_domains"));
    }

    #[test]
    fn allowed_private_host_with_port() {
        let tool = test_tool_with_private_hosts(vec!["*"], vec![], vec!["192.168.1.5"]);
        assert!(tool.validate_url("https://192.168.1.5:8080/api").is_ok());
    }
}
