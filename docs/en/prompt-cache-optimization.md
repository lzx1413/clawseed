# Prompt Cache Optimization — Implementation Summary

## Context

ClawSeed's multi-turn conversations with Anthropic had **~0% cache hit rate** because the system prompt changes every turn (datetime with second precision at position 0), and it was sent as a single `SystemBlock`. Anthropic's prompt caching relies on exact prefix matching — any change at any position breaks the entire cache chain. This wastes ~2–4 KB / 1–3k tokens per turn in multi-turn conversations.

## Root Cause

Three fatal cache-breaking issues:

1. **`DateTimeSection` at position 0** — content changes every second, diverging at the very start of the prefix
2. **Single `SystemBlock`** — any character change forces reprocessing of the entire block
3. **Full rebuild on stable changes** — Core memory / skill / tool changes replace the entire system message content

## Implementation

### Phase 0 → Minute Precision (superseded by Phase 2)

Initially reduced `DateTimeSection::build()` from second-precision to minute-precision. This improved cache hits within the same minute, but the system prompt still changed every minute — insufficient for long sessions.

### Phase 1 — Stable/Dynamic Partitioning (superseded by Phase 2)

Introduced `CacheClass` (Stable/Dynamic) and `PartitionedSystemPrompt { stable, dynamic, full }` to split the system prompt into a cacheable prefix and a per-turn dynamic suffix. `DateTimeSection` was marked as `Dynamic` and moved to the end, with a preamble bridge (`⚠️ THE CURRENT TIME BELOW APPLIES TO ALL ABOVE INSTRUCTIONS.`) appended to the stable block.

This achieved Anthropic prefix caching, but required per-turn dynamic rebuilds and added complexity (preamble, split logic, `dynamic_system_content` field, `refresh_dynamic_system_content()` method).

### Phase 2 — Full Stability (current implementation)

**Key insight**: If the system prompt is 100% stable across turns (zero per-turn changes), automatic prefix caching works without any message-level transformation. Only Anthropic and Bedrock need explicit `cache_control: ephemeral` markers; all other providers benefit from the stable prefix automatically.

#### 1. Remove DateTimeSection from system prompt (`prompt.rs`)

`DateTimeSection` is no longer included in `SystemPromptBuilder::with_defaults()`. Current time is provided via the **user message timestamp prefix** instead:

```
[2024-06-14 15:42:00 CST] What is the weather today?
```

The gateway and CLI both prepend this `[YYYY-MM-DD HH:MM:SS TZ]` prefix to every user message before sending it to the agent. This keeps time context available to the model without injecting it into the system prompt.

**Benefits**:
- The entire system message is byte-identical across all turns → 100% stable prefix
- Works for **all** providers with automatic prefix caching (DeepSeek, OpenAI, Groq, etc.)
- No per-turn rebuild cost — eliminates `refresh_dynamic_system_content()`, `build_dynamic_system_content()`, and `build_dynamic()`

#### 2. Simplify PartitionedSystemPrompt (`prompt.rs`)

With no Dynamic sections, `PartitionedSystemPrompt` simplifies:

```rust
pub struct PartitionedSystemPrompt {
    pub stable: String,   // Full system prompt content (all sections)
    pub dynamic: String,  // Always empty — no Dynamic sections currently exist
    pub full: String,     // Equals stable when dynamic is empty
}
```

- `build_partitioned()` no longer appends the preamble — it's removed (`DYNAMIC_PREAMBLE` constant deleted)
- The `else` branch (stable + dynamic concatenation) is retained for future dynamic sections but currently never executed
- `build_dynamic()` method removed — no dynamic sections to build separately

#### 3. Remove dynamic content from Agent (`agent.rs`)

Removed fields and methods:
- `dynamic_system_content` field — no longer needed
- `refresh_dynamic_system_content()` — no dynamic content to refresh per-turn
- `build_dynamic_system_content()` — no dynamic sections to build

`Agent` now only has `stable_system_content` — the full system prompt content, which is rebuilt only when stable content changes (Core memory updates, skill activation/deactivation, tool changes).

#### 4. CacheStrategy enum replaces `prompt_caching: bool` (`clawseed-api/src/provider.rs`)

```rust
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum CacheStrategy {
    /// No explicit caching. Automatic prefix caching works because
    /// the entire system prompt is stable.
    #[default]
    None,
    /// Anthropic-style explicit `cache_control: ephemeral` markers or
    /// Bedrock-style `CachePoint` blocks within system messages.
    ExplicitAnthropic,
}

pub struct ProviderCapabilities {
    pub native_tool_calling: bool,
    pub vision: bool,
    pub cache_strategy: CacheStrategy,  // Was: prompt_caching: bool
}
```

**Provider assignments**:

| Provider | CacheStrategy | Reason |
|----------|--------------|--------|
| Anthropic | `ExplicitAnthropic` | Requires `cache_control: ephemeral` markers on system message blocks |
| Bedrock | `ExplicitAnthropic` | Requires `CachePoint` blocks within system messages |
| OpenAI-compatible (DeepSeek, Groq, Ollama, etc.) | `None` | Automatic server-side prefix caching works with stable prompts |
| Gemini | `None` | No explicit cache markers needed |

The `CacheStrategy::None` default means new providers automatically get correct behavior — they benefit from the stable system prompt without needing explicit cache markers.

#### 5. DeepSeek Anthropic-compatible endpoint (`factory.rs`)

New `DeepSeekAnthropicFactory` wraps `AnthropicProvider` with DeepSeek's Anthropic-compatible base URL (`https://api.deepseek.com/anthropic`). This endpoint supports `cache_control: ephemeral` markers, giving DeepSeek users explicit prompt caching the same way Anthropic users get it.

- Provider name: `deepseek-anthropic` (aliases: `deepseek-claude`)
- Uses `AnthropicProvider::with_base_url()` — same conversion logic, same `stable_prefix` handling, same `cache_control` injection
- Registered alongside other factories in `default_provider_factory_registry()`

**Why**: DeepSeek's OpenAI-compatible endpoint (`/v1/chat/completions`) only supports automatic prefix caching. The `/anthropic` endpoint supports explicit `cache_control`, giving finer control and guaranteed cache hits for Anthropic-style clients.

#### 6. Cached input tokens parsing (`compatible/parsing.rs`, `provider_impl.rs`)

`TokenUsage.cached_input_tokens` is now populated from provider-specific response fields:

- **DeepSeek** (`/v1/chat/completions`): `prompt_cache_hit_tokens` field
- **OpenAI**: `prompt_tokens_details.cached_tokens` sub-field
- Extraction via `UsageInfo::extract_cached_tokens()` helper method (shared between `chat()` and `stream_chat()` paths)

```rust
impl UsageInfo {
    pub(super) fn extract_cached_tokens(&self) -> Option<u64> {
        self.prompt_cache_hit_tokens
            .or_else(|| self.prompt_tokens_details.as_ref()?.cached_tokens)
    }
}
```

### Anthropic / Bedrock Integration (unchanged from Phase 1)

Anthropic and Bedrock providers still use `stable_prefix` to split system messages into cacheable blocks:

- **Anthropic**: `SystemPrompt::Blocks([stable_block(cache_control: ephemeral), dynamic_block(no_cache)])`
- **Bedrock**: `SystemBlock::Text(stable)` + `CachePoint` + `SystemBlock::Text(dynamic)`

Since `dynamic` is always empty now, the "dynamic block" is effectively empty or absent. The stable block contains the entire system prompt with a single `cache_control` marker, which Anthropic caches as a whole.

### Cache Breakpoint Budget

Anthropic caps at 4 breakpoints per request. Phase 2 does not increase the count:

| Position | Before | Phase 2 |
|---|---|---|
| OAuth prefix block | 0 or 1 | 0 or 1 |
| System prompt | 1 (single block) | 1 (entire prompt with `cache_control: ephemeral`) |
| Last conversation message | 0 or 1 | 0 or 1 |
| Tool results | 0 or 1 | 0 or 1 |
| **Max total** | **4** | **4** |

## Known Limitations

1. **Stable block rebuilds break cache that turn**. Triggered by: `memory_store` adding Core memory, skill activation/deactivation, remote tool registration. The next turn re-caches. In steady sessions this is rare.

2. **Minimum cacheable prefix is 1024 tokens** (Sonnet/Opus). Compact configurations (minimal personality, no skills, no Core memories) may fall below the threshold and won't be cached.

3. **Provider coverage**: Anthropic + Bedrock use `CacheStrategy::ExplicitAnthropic` (explicit markers). DeepSeek-anthropic endpoint also supports explicit markers. All other providers use `CacheStrategy::None` (automatic prefix caching via stable prompts). Server-side implicit caching on OpenAI/DeepSeek/Groq benefits from the fully stable system prompt.

4. **Time context**: No longer in the system prompt. The `[YYYY-MM-DD HH:MM:SS TZ]` prefix on each user message provides time context. This means:
   - The model knows the current time on each turn from the user message
   - The time is not cached (changes each turn) but only adds ~30 bytes to the user message, not to the system prompt
   - Tasks requiring exact timestamps can use tool calls (e.g., `shell_exec date`)

5. **Prompt-guided tool injection**: The default Provider `chat()` method appends tool instructions to system `content` when `native_tool_calling: false`. If the system message has `stable_prefix: Some(...)`, appending to `content` breaks the partition invariant. Currently no provider has `native_tool_calling: false` AND `CacheStrategy::ExplicitAnthropic`, so this does not arise.

## Expected Behavior

| Turn | System Prompt Shape | Cache Result |
|------|---------------------|-------------|
| Turn 1 | `[entire_prompt(cache_control: ephemeral)]` (Anthropic/Bedrock) or `[entire_prompt]` (others) | Full system processed; cached if ≥1024 tokens |
| Turn 2 | Same system prompt (byte-identical), user message with updated timestamp prefix | Stable prefix matches → **cache hit** on all providers |
| Turn N (no stable change) | Same | Cache hit every turn (Anthropic within 5-min TTL, others via server-side prefix cache) |
| Stable change (skill/memory/tool) | New system prompt content | Cache miss for that turn; new cache established for subsequent turns |

**Estimated savings**: Stable input tokens billed at ~10% of normal on Anthropic cache hit. For a typical 3k-token stable prefix, steady-state cost reduction on system tokens is ~90% (entire prompt is cached, not just a portion). Other providers benefit from server-side implicit prefix caching at no extra cost.

## Verification

1. `cargo test -p clawseed-agent` — system prompt has no datetime section, all sections are Stable, partitioned build with empty dynamic
2. `cargo test -p clawseed-api` — ChatMessage serde roundtrip, system_partitioned
3. `cargo test -p clawseed-providers` — Anthropic/Bedrock partitioned conversion, DeepSeekAnthropicFactory
4. `cargo build` — full workspace compiles
5. `./tools/ci_local.sh` — fmt/clippy/test pass
6. Manual: `clawseed chat` against Anthropic, 2 turns → Turn 2 `cache_read_input_tokens > 0`
7. Manual: `clawseed chat` against DeepSeek-anthropic → `cache_read_input_tokens > 0`
