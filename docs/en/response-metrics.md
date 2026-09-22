# Provider balances and reply statistics

The Android LLM configuration page queries the selected provider's balance after
opening the page or changing the URL/key (600 ms debounce), then refreshes every
60 seconds while the page is open. A changed provider never inherits the previous
provider's balance. DeepSeek and Moonshot China return available balances;
OpenRouter returns purchased credits minus usage and requires a management key.
Other endpoints display unsupported. Missing credentials, invalid credentials,
insufficient permissions, and temporary failures have distinct display states.
A returned zero is a real zero, never a placeholder for missing data.

`POST /api/provider/balance` accepts `base_url` and optional `api_key`. It requires
gateway authentication. An omitted key reuses a saved credential for the same
base URL; an explicit empty key does not. Billing requests only target recognized
HTTPS endpoints, and neither credentials nor upstream error bodies are returned.
Responses carry `Cache-Control: no-store`.

Enable **Debug query** in Android settings to show reply statistics. New replies
include an optional `metrics` object in the WebSocket `done` frame and in session
history. SQLite stores this separately from model conversation context.
Disabling debug hides the statistics and stops collecting them for new turns.
Older messages with no recorded metrics remain unchanged.

- `input_tokens` and `cached_input_tokens`: provider-reported usage from the latest
  model request. Input is no longer summed across tool-loop requests, so it describes
  the prompt actually sent to the provider for that request.
- `output_tokens`: provider-reported output total across all model requests in the turn,
  including tool iterations. Gemini output includes reported
  thinking tokens.
- Anthropic and Bedrock input totals include uncached input, cache writes, and cache reads.
- `cache_hit_ratio`: cached input / total input for the latest request, as a fraction in
  `[0, 1]`. Cache writes do not count as hits.
- `output_tokens_per_second`: total output / sum of streaming generation times,
  from first text/reasoning/tool-generation event to stream completion for each
  model call. Excludes first-token wait and tool execution. This is a client-side
  streaming measurement, subject to network buffering. Non-streaming calls have
  no measurable generation interval, so speed is unavailable for such turns.
- `elapsed_ms`: elapsed agent turn time, including preparation, model waits, and
  tools. Excludes client transport, queueing before the agent starts, and background
  title generation or learning.

Missing fields stay null and display **Not provided**. If the latest request lacks
input or cache usage, that field is unavailable; if any model request lacks output,
the whole-turn output total is unavailable rather than a partial sum. Prompt token
estimates are not substituted for provider usage.

References: [DeepSeek balance](https://api-docs.deepseek.com/api/get-user-balance/),
[Moonshot balance](https://platform.kimi.com/docs/api/balance),
[OpenRouter credits](https://openrouter.ai/docs/api/api-reference/credits/get-remaining-credits),
[OpenAI prompt caching](https://developers.openai.com/api/docs/guides/prompt-caching),
[Anthropic caching](https://platform.claude.com/docs/en/build-with-claude/prompt-caching),
[Bedrock caching](https://docs.aws.amazon.com/bedrock/latest/userguide/prompt-caching.html),
[Gemini usage](https://ai.google.dev/api/generate-content#UsageMetadata).

## Request prefixes and Debug

Tool specs are sorted by name and shared by system rendering and the native tools payload. Native mode submits complete schemas only through tools; XML mode retains prompt definitions. Skill discovery uses a bounded first-sentence summary; full rules must be loaded through Skill.

Before a request, Debug `estimated_tokens` approximates the complete input of the latest model
call, including native tool definitions. After the provider responds, the same Debug card is
updated with that request's exact input when available. `estimated_tool_tokens` exposes the
tool-definition portion separately. The next pre-request estimate uses the previous exact input
as a baseline and projects prompt changes forward. The message snapshot omits duplicate internal
stable_prefix metadata. Estimates are not used as cache-ratio denominators; reply input/cache
metrics describe the latest request, while output and generation speed cover the whole tool loop.
