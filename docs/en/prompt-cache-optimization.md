# Prompt Cache Optimization — Implementation Summary

## Context

ClawSeed's multi-turn conversations with Anthropic had **~0% cache hit rate** because the system prompt changes every turn (datetime with second precision at position 0), and it was sent as a single `SystemBlock`. Anthropic's prompt caching relies on exact prefix matching — any change at any position breaks the entire cache chain. This wastes ~2–4 KB / 1–3k tokens per turn in multi-turn conversations.

## Root Cause

Three fatal cache-breaking issues:

1. **`DateTimeSection` at position 0** — content changes every second, diverging at the very start of the prefix
2. **Single `SystemBlock`** — any character change forces reprocessing of the entire block
3. **Full rebuild on stable changes** — Core memory / skill / tool changes replace the entire system message content

## Implementation

### Phase 0 — Reduce DateTime Precision

**File**: `crates/clawseed-agent/src/prompt.rs`

Changed `DateTimeSection::build()` from second-precision to minute-precision:

```rust
// Before:
"Date: {year:04}-{month:02}-{day:02}\nTime: {hour:02}:{minute:02}:{second:02} ({tz})"
// After:
"Date: {year:04}-{month:02}-{day:02}\nTime: {hour:02}:{minute:02} ({tz})"
```

**Trade-off**: Minute precision means cache hits within the same minute. Multi-turn conversations typically stay within the 5-minute Anthropic cache TTL, so the cache benefit is retained while preserving useful time granularity. The model loses second-level precision but still knows the current time to the minute — sufficient for all practical tasks.

**Benefit**: Broad — improves cache hit rate for all providers including OpenAI/DeepSeek server-side implicit prefix caching, not just Anthropic.

### Phase 1 — Stable/Dynamic Partitioning

#### 1. CacheClass + PartitionedSystemPrompt (`prompt.rs`)

- `CacheClass` enum: `Stable` / `Dynamic`
- `PromptSection::cache_class()` default method → `Stable`
- `DateTimeSection::cache_class()` overridden → `Dynamic`
- `PartitionedSystemPrompt { stable, dynamic, full }` struct
- `SystemPromptBuilder::build_partitioned()` — routes `Stable` → `stable_buf`, `Dynamic` → `dynamic_buf`
- `SystemPromptBuilder::build_dynamic()` — rebuilds only `Dynamic` sections (for per-turn refresh)

**Preamble**: When both halves are non-empty, a preamble `⚠️ THE CURRENT TIME BELOW APPLIES TO ALL ABOVE INSTRUCTIONS.` is appended to the END of the stable buffer. This bridges the semantic gap caused by moving datetime from position 0 to the end of the prompt. The preamble lives inside the stable block so it's part of the cacheable prefix and never changes.

**Section order in `full`**:
```
// Before partitioning (legacy build()):
[DateTime] → [Identity] → [Platform] → [Workspace] → ... → [Skills]

// After partitioning (build_partitioned().full):
[Identity] → [Platform] → [Workspace] → ... → [Skills] → [preamble] → [DateTime]
```

The preamble makes the model treat the time as applying to all preceding instructions, mitigating the semantic impact of moving datetime to the end.

#### 2. stable_prefix on ChatMessage (`clawseed-api/src/provider.rs`)

```rust
pub struct ChatMessage {
    pub role: String,
    pub content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stable_prefix: Option<String>,
}
```

- `ChatMessage::system_partitioned(stable, dynamic, full)` — sets `content = full`, `stable_prefix = Some(stable)`
- All other constructors (`system`, `user`, `assistant`, `tool`) set `stable_prefix: None`
- `stable_prefix` is not persisted in session storage — it's rebuilt by `seed_history` on resume
- Serde: `#[serde(default)]` for backward compat with JSON lacking the field; `#[serde(skip_serializing_if)]` to omit when `None`

#### 3. Agent uses partitioned prompts (`agent.rs`)

New fields on `Agent`:
- `stable_system_content: String` — cached stable portion (not rebuilt per-turn)
- `dynamic_system_content: String` — cached dynamic portion (rebuilt per-turn)

New methods:
- `build_system_prompt_partitioned()` — full partitioned build, stores both halves on Agent
- `build_dynamic_system_content()` — only rebuilds Dynamic sections (datetime)
- `refresh_dynamic_system_content()` — calls `build_dynamic_system_content()`, reconstructs `full` from cached `stable_system_content` + preamble + new dynamic, updates system ChatMessage in history

Modified methods:
- `prepare_turn()` — first turn: `build_system_prompt_partitioned()`, push `ChatMessage::system_partitioned(stable, dynamic, full)`
- `rebuild_system_prompt()` — full rebuild on stable changes (skill/memory/tool), uses partitioned build
- `seed_history()` — discards old system messages, rebuilds from current context via partitioned build
- `turn()` / `turn_streamed()` — calls `refresh_dynamic_system_content()` after `prepare_turn()` on subsequent turns

**Per-turn cost**: Only 1 Dynamic section (DateTimeSection) is rebuilt, not the full 11-section pipeline. The `stable_system_content` field is read from the cached value — not rebuilt.

#### 4. Anthropic provider (`anthropic.rs`)

In `convert_messages()`:
- Captures `msg.stable_prefix` alongside `msg.content` for system messages
- When `stable_prefix` is `Some(stable)` and content starts with `stable`:
  - Emits `SystemPrompt::Blocks([stable_block(cache_control), dynamic_block(no_cache)])`
  - Strips the preamble from the dynamic portion (preamble is only needed for non-Anthropic providers that read `content` directly)
- Otherwise: single block with `cache_control` (legacy path)
- Defense: `text.starts_with(&stable)` and `!stable.is_empty()` guards prevent incorrect slicing if content/stable_prefix become desynchronized

#### 5. Bedrock provider (`bedrock.rs`)

In `convert_messages()`:
- When `stable_prefix` is `Some(stable)` and content starts with `stable`:
  - Emits `SystemBlock::Text(stable)` + `SystemBlock::CachePoint` + `SystemBlock::Text(dynamic)`
  - Strips preamble from dynamic portion
- Otherwise: single `SystemBlock::Text(content)` (legacy path)
- Defense: same `starts_with` and `is_empty` guards
- In `chat()`: skips redundant post-hoc `CachePoint` insertion when partition already provided one

### Cache Breakpoint Budget

Anthropic caps at 4 breakpoints per request. Phase 1 does not increase the count:

| Position | Before | After Phase 1 |
|---|---|---|
| OAuth prefix block | 0 or 1 | 0 or 1 |
| System prompt | 1 (single block) | 1 (on stable block only) |
| Last conversation message | 0 or 1 | 0 or 1 |
| Tool results | 0 or 1 | 0 or 1 |
| **Max total** | **4** | **4** |

## Known Limitations

1. **Stable block rebuilds break cache that turn**. Triggered by: `memory_store` adding Core memory, skill activation/deactivation, remote tool registration. The next turn re-caches. In steady sessions this is rare.

2. **Minimum cacheable prefix is 1024 tokens** (Sonnet/Opus). Compact configurations (minimal personality, no skills, no Core memories) may fall below the threshold and won't be cached.

3. **Provider coverage**: Anthropic + Bedrock wired. Gemini, OpenAI, DeepSeek, Ollama have `prompt_caching: false` and ignore `stable_prefix`. Server-side implicit caching on OpenAI/DeepSeek still benefits from Phase 0 (minute precision).

4. **Datetime position**: In the partitioned `full` string, datetime appears at the end instead of position 0. The preamble bridges this semantic gap. Non-Anthropic providers see `content` with datetime at the end — the preamble makes this explicit.

5. **Minute precision trade-off**: The model no longer knows second-level time. Tasks requiring exact timestamps should use tool calls (e.g., `shell_exec date`). This trade-off is acceptable for cache benefit.

6. **Prompt-guided tool injection**: The default Provider `chat()` method appends tool instructions to system `content` when `native_tool_calling: false`. If the system message has `stable_prefix: Some(...)`, appending to `content` breaks the partition invariant. Currently no provider has `native_tool_calling: false` AND `prompt_caching: true`, so this does not arise.

## Expected Behavior

| Turn | System Prompt Shape | Cache Result |
|------|---------------------|-------------|
| Turn 1 | `[stable_block(cache_control)] + [preamble(in stable)] + [dynamic_block]` | Full system processed; stable prefix cached if ≥1024 tokens |
| Turn 2 | Same stable block, new dynamic (datetime refreshed to current minute) | Stable prefix matches → **cache hit**; only dynamic block reprocessed |
| Turn N (no stable change) | Same | Stable hits cache every turn within 5-min TTL |
| Stable change (skill/memory/tool) | New stable block content | Cache miss for that turn; new cache established for subsequent turns |

**Estimated savings**: Stable input tokens billed at ~10% of normal on cache hit. For a typical 3k-token stable prefix with ~50-token dynamic block, steady-state cost reduction on system tokens is ~80–85%. Conversation-level cache also benefits because the system prefix is now stable.

## Verification

1. `cargo test -p clawseed-agent` — datetime minute precision, cache class defaults, partitioned build, build_dynamic
2. `cargo test -p clawseed-api` — ChatMessage serde roundtrip, system_partitioned
3. `cargo test -p clawseed-providers` — Anthropic/Bedrock partitioned conversion
4. `cargo build` — full workspace compiles
5. `./tools/ci_local.sh` — fmt/clippy/test pass
6. Manual: `clawseed chat` against Anthropic, 2 turns within same minute → Turn 2 `cache_read_input_tokens > 0`
