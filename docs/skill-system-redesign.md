# Skill System Redesign

> **Status:** Draft
> **Scope:** `crates/zeroclaw-runtime/src/skills/`, `crates/zeroclaw-config/src/schema.rs`
> **Motivation:** Current design conflates skills and tools; this document proposes a clean separation with on-demand loading, system-prompt injection, and KV-cache–aware segmentation.

---

## 1. Problem Statement

### 1.1 ZeroClaw Current Design

ZeroClaw's skill system has two pipelines:

1. **Prompt injection** — `skills_to_prompt_with_mode()` writes all skill instructions into the system prompt at startup.
2. **Tool registration** — `skills_to_tools()` converts a skill's `[[tools]]` entries into `SkillShellTool` / `SkillHttpTool` and registers them in the same `Vec<Box<dyn Tool>>` as built-in tools like `Bash` and `Read`.

Problems:

| Problem | Detail |
|---------|--------|
| **Skill-Tool conflation** | `skills_to_tools()` treats a multi-step workflow as a callable tool. Skills are workflow orchestrators, not atomic operations. |
| **Dead code** | All 14 skills in `zeroclaw-skills` are instruction-only (SKILL.md + manifest.toml). Zero define `[[tools]]`. The `SkillShellTool` / `SkillHttpTool` / `register_skill_tools()` pipeline is unused. |
| **Context waste** | Full mode injects all skill instructions into the system prompt at startup, even if none are used. |
| **Wrong relationship direction** | `skills_to_tools()` makes skills *produce* tools. The actual relationship is the opposite: skills *consume* tools (declared via `permissions`). |

### 1.2 Claw Code Design (for reference)

Claw Code implements `Skill` as a single built-in tool that reads a SKILL.md file and returns its content as a `tool_result`.

| Strength | Weakness |
|----------|----------|
| Clean Skill-Tool boundary | LLM doesn't know which skills exist (no index) |
| On-demand loading (efficient) | Skill content lives in conversation history — may be lost to context compaction |
| No dead code paths | No `permissions` declaration or runtime checking |

### 1.3 Core Insight

Skill and Tool are different abstraction layers:

```
Tool  = atomic operation, stateless, one call → one result
Skill = stateful workflow, multi-step, ordered, with confirmation points
```

Skills *orchestrate* Tools. Skills should never *become* Tools.

---

## 2. Design Principles

1. **Skill is a workflow orchestrator, not a tool.** Skills declare which tools they need (`permissions`), they do not register new tools.
2. **Index is always visible; content is loaded on demand.** The LLM sees a compact skill index in every turn. Full instructions are loaded only when a skill is activated.
3. **Loaded skill content persists in the system prompt.** Not in conversation history (avoids compaction loss). Not by modifying existing prompt sections (avoids KV-cache invalidation).
4. **Append-only system prompt segments.** Each loaded skill becomes a new `cache_control`–marked segment. Existing segments are never modified, preserving KV cache for Anthropic-native providers. For OpenAI-compatible providers, the same append-only strategy minimizes reprocessing.
5. **No `skills_to_tools()` pipeline.** Remove `SkillShellTool`, `SkillHttpTool`, and `register_skill_tools()` entirely.

---

## 3. Architecture

### 3.1 Data Structures

```rust
/// Compact entry for the skill index (always in system prompt).
/// ~30-50 tokens per skill.
struct SkillIndexEntry {
    name: String,
    description: String,
    trigger_phrases: Vec<String>,   // from manifest.toml [skill].triggers
    permissions: Vec<String>,       // from manifest.toml [skill].permissions
}

/// Full skill definition (loaded on demand).
struct Skill {
    name: String,
    description: String,
    version: String,
    author: Option<String>,
    tags: Vec<String>,
    permissions: Vec<String>,
    prompts: Vec<String>,           // SKILL.md body content
    location: PathBuf,
}

/// Agent state for skills.
struct Agent {
    tools: Vec<Box<dyn Tool>>,              // built-in tools only
    skill_index: Vec<SkillIndexEntry>,      // always populated at init
    active_skills: Vec<Skill>,              // populated on demand
    // ... other fields
}
```

### 3.2 Skill Manifest Format

**manifest.toml** (metadata + permissions, no `[[tools]]`):

```toml
[skill]
name = "auto-coder"
version = "0.3.0"
author = "zeroclaw-labs"
description = "Autonomous code generation agent. Reads context, writes code, runs tests."
category = "coding"
tags = ["Official", "Featured"]
license = "MIT"
permissions = ["file_read", "file_write", "shell_exec"]
triggers = ["write code", "implement feature", "make code changes"]
```

Key changes from current format:

| Field | Current | New |
|-------|---------|-----|
| `permissions` | Present but unused at runtime | Used for permission checking at load time |
| `triggers` | Not in manifest; embedded in SKILL.md frontmatter `description` | Explicit field, also surfaced in skill index |
| `[[tools]]` | Supported (but unused in practice) | **Removed** |

**SKILL.md** (instruction content only):

```markdown
---
name: auto-coder
description: "Autonomous code generation agent..."
---

# Auto Coder

You are an autonomous coding agent...

## Workflow
1. **Understand the task.** ...
2. **Read before you write.** Use `file_read` to examine...
3. **Plan your changes.** ...
...
```

No `[[tools]]` section. Skill instructions reference built-in tools by their canonical permission names (`file_read`, `shell_exec`, etc.) or by their runtime tool names (`Read`, `Bash`, etc.).

### 3.3 System Prompt Structure

The system prompt is built as a `Vec<SystemMessage>`, where each message can carry its own `cache_control` marker:

```
System Message 0 (always present, cached):
  [Identity] [Tool List] [Safety Rules]
  <available_skills>
    <skill name="auto-coder" triggers="write code, implement feature">
      Autonomous code generation. Reads context, writes code, runs tests.
    </skill>
    <skill name="web-researcher" triggers="research, find information">
      Deep web research with source citation.
    </skill>
    ...
  </available_skills>

System Message 1 (present when auto-coder is active, cached):
  <active_skill name="auto-coder">
    [full SKILL.md content]
  </active_skill>

System Message 2 (present when web-researcher is also active, cached):
  <active_skill name="web-researcher">
    [full SKILL.md content]
  </active_skill>
```

**Rules:**

- System Message 0 is built once at agent init and never modified.
- System Messages 1..N are appended when skills are activated and never modified after.
- Each message carries `cache_control: { "type": "ephemeral" }` for Anthropic-native providers.
- For providers without cache control support, the messages are concatenated into a single system prompt string (degraded mode — cache invalidates on skill load, but re-stabilizes on subsequent turns).

### 3.4 Skill Tool

A single built-in tool for loading skills:

```rust
ToolSpec {
    name: "Skill",
    description: "Load a skill's full instructions by name. \
                  Use when a user request matches a skill from <available_skills>.",
    input_schema: json!({
        "type": "object",
        "properties": {
            "skill": {
                "type": "string",
                "description": "Exact skill name from <available_skills>"
            },
            "args": {
                "type": "string",
                "description": "Optional arguments to pass to the skill"
            }
        },
        "required": ["skill"]
    }),
    required_permission: PermissionMode::ReadOnly,
}
```

Execution:

```rust
fn execute_skill(input: SkillInput, agent: &mut Agent) -> Result<SkillOutput, String> {
    // 1. Check if already active
    if agent.active_skills.iter().any(|s| s.name == input.skill) {
        return Ok(SkillOutput {
            status: "already_active".into(),
            skill: input.skill,
            message: "Skill instructions are already in your system prompt.".into(),
        });
    }

    // 2. Resolve and load
    let skill = load_skill_by_name(&input.skill)?;

    // 3. Permission check
    for perm in &skill.permissions {
        if !agent.tools.iter().any(|t| t.satisfies_permission(perm)) {
            return Err(format!(
                "Skill '{}' requires permission '{}' which is not available.",
                skill.name, perm
            ));
        }
    }

    // 4. Activate: append to active_skills (triggers system prompt rebuild)
    agent.active_skills.push(skill);

    Ok(SkillOutput {
        status: "activated".into(),
        skill: input.skill,
        message: "Skill instructions have been added to your system prompt. \
                  Follow the workflow steps using the available tools.".into(),
    })
}
```

### 3.5 System Prompt Builder

```rust
fn build_system_messages(&self, provider: &dyn Provider) -> Vec<SystemMessage> {
    let mut messages = Vec::new();

    // Segment 0: base prompt + skill index (never changes after init)
    let mut segment_0 = String::new();
    segment_0.push_str(&self.base_prompt());
    segment_0.push_str(&self.render_skill_index());
    messages.push(SystemMessage {
        text: segment_0,
        cache_control: provider.supports_cache_control()
            .then_some(CacheControl::Ephemeral),
    });

    // Segments 1..N: one per active skill (append-only, never modified)
    for skill in &self.active_skills {
        messages.push(SystemMessage {
            text: format!(
                "<active_skill name=\"{}\">\n{}\n</active_skill>",
                skill.name,
                skill.prompts.join("\n\n")
            ),
            cache_control: provider.supports_cache_control()
                .then_some(CacheControl::Ephemeral),
        });
    }

    messages
}
```

### 3.6 Skill Index Rendering

```rust
fn render_skill_index(&self) -> String {
    if self.skill_index.is_empty() {
        return String::new();
    }

    let mut s = String::from(
        "## Available Skills\n\n\
         Skill summaries are listed below. To use a skill, call `Skill({\"skill\": \"<name>\"})` \
         to load its full instructions into your system prompt.\n\n\
         <available_skills>\n"
    );

    for entry in &self.skill_index {
        let triggers = entry.trigger_phrases.join(", ");
        let _ = writeln!(
            s,
            "  <skill name=\"{}\" triggers=\"{}\">",
            entry.name, triggers
        );
        let _ = writeln!(s, "    {}", entry.description);
        let _ = writeln!(s, "  </skill>");
    }

    s.push_str("</available_skills>");
    s
}
```

---

## 4. Execution Flow

### 4.1 Agent Initialization

```
1. load_skill_index(workspace_dir, config)
     → scan skills/ directories
     → read manifest.toml from each
     → extract name, description, triggers, permissions
     → return Vec<SkillIndexEntry>

2. active_skills = Vec::new()

3. build_system_messages()
     → [Segment 0: base + index]     (~500-800 tokens for 14 skills)
```

### 4.2 Skill Activation (first use)

```
User: "帮我实现一个功能"
  │
  ▼
LLM sees index → matches "implement feature" → auto-coder
  │
  ▼
tool_call: Skill({ "skill": "auto-coder" })
  │
  ▼
execute_skill():
  1. Check already active? → No
  2. load_skill_by_name("auto-coder") → read SKILL.md + manifest.toml
  3. Check permissions ["file_read", "file_write", "shell_exec"] → all satisfied
  4. active_skills.push(auto-coder)
  5. Return: "Skill activated. Instructions added to system prompt."
  │
  ▼
build_system_messages():
  → [Segment 0: base + index]           ← cache HIT (Anthropic)
  → [Segment 1: auto-coder full text]   ← cache MISS, processed fresh
  │
  ▼
LLM now sees full auto-coder instructions in system prompt
LLM follows workflow: file_read → file_write → shell_exec → ...
```

### 4.3 Subsequent Turns (skill already active)

```
build_system_messages():
  → [Segment 0]   ← cache HIT
  → [Segment 1]   ← cache HIT

LLM sees full instructions every turn.
No cache invalidation. No context compaction risk.
```

### 4.4 Second Skill Activation

```
User: "再查一下这个技术"
  │
  ▼
LLM → Skill({ "skill": "web-researcher" })
  │
  ▼
active_skills.push(web-researcher)
  │
  ▼
build_system_messages():
  → [Segment 0: base + index]              ← cache HIT
  → [Segment 1: auto-coder full text]      ← cache HIT
  → [Segment 2: web-researcher full text]  ← cache MISS, processed fresh
  │
  ▼
Next turn:
  → [Segment 0]  ← cache HIT
  → [Segment 1]  ← cache HIT
  → [Segment 2]  ← cache HIT
```

### 4.5 Complete Flow Diagram

```
┌─ Agent Init ────────────────────────────────────────────────┐
│                                                              │
│  load_skill_index()  →  Vec<SkillIndexEntry>                │
│  active_skills       =  []                                   │
│  system_messages     =  [Seg0: base + index]                 │
│                                                              │
└──────────────────────────────────────────────────────────────┘
                          │
                          ▼
┌─ Agent Loop ────────────────────────────────────────────────┐
│                                                              │
│  1. build_system_messages()  →  [Seg0] + [Seg1..N]          │
│                                                              │
│  2. provider.chat(messages, tool_specs)                      │
│     tool_specs = built-in tools + Skill tool                 │
│                                                              │
│  3. Parse response → (text, Vec<ParsedToolCall>)             │
│                                                              │
│  4. For each tool_call:                                      │
│     ├─ "Skill" → execute_skill()                             │
│     │    ├─ load SKILL.md + manifest.toml                    │
│     │    ├─ check permissions                                │
│     │    ├─ active_skills.push(skill)                        │
│     │    └─ return activation confirmation                   │
│     │                                                        │
│     └─ other → tools_registry.find(name).execute(args)       │
│          (Bash, Read, Write, WebFetch, etc.)                 │
│                                                              │
│  5. Format results → append to history                       │
│                                                              │
│  6. Loop until no tool_calls or max_iterations               │
│                                                              │
└──────────────────────────────────────────────────────────────┘
```

---

## 5. KV Cache Behavior

### 5.1 Anthropic Native API (with `cache_control`)

| Event | Cache Effect |
|-------|-------------|
| Agent init | Seg0 cached |
| Skill activated | Seg0 HIT, new SegN MISS (only skill tokens processed) |
| Subsequent turns | Seg0..N all HIT |
| Second skill activated | Seg0..N HIT, new SegN+1 MISS |
| Context compaction | System prompt segments preserved (not in conversation history) |

**Cost of activating a skill:** Only the token count of that skill's SKILL.md content. Existing segments are served from cache.

### 5.2 OpenAI-Compatible Providers (no `cache_control`)

| Event | Cache Effect |
|-------|-------------|
| Agent init | System prompt cached by provider's automatic prefix caching |
| Skill activated | System prompt changed → cache MISS (full reprocess) |
| Subsequent turns | System prompt unchanged → cache re-stabilizes |
| Second skill activated | Cache MISS again |
| Context compaction | System prompt preserved |

**Cost of activating a skill:** Full system prompt reprocessing on the activation turn only. Subsequent turns are cached again.

**Mitigation:** The append-only strategy ensures the system prompt only grows, never changes structure. Providers with prefix-based caching may partially preserve the cache for the unchanged prefix portion.

### 5.3 Comparison with Alternatives

| Approach | Anthropic Cache | OpenAI Cache | Compaction Risk |
|----------|----------------|--------------|-----------------|
| **ZeroClaw current (Full mode)** | All skills cached at init | All skills cached at init | None (in system prompt) |
| **ZeroClaw current (Compact mode)** | Index cached, `read_skill` on demand | Same | None for index; skill content in tool_result at risk |
| **Claw Code** | N/A (uses Anthropic but no cache_control) | N/A | **High** (skill content in tool_result, compaction can drop it) |
| **This proposal** | Segmented, append-only | Degraded (full reprocess on load) | None (in system prompt) |

---

## 6. Skill Discovery and Loading

### 6.1 Discovery: Multi-Source Lookup

Skill index entries are loaded from multiple roots in priority order (higher priority wins on name collision):

```
Priority 1 — Project-level:
  <project>/.zeroclaw/skills/<name>/
  <project>/.claude/skills/<name>/

Priority 2 — User-level:
  ~/.zeroclaw/workspace/skills/<name>/
  ~/.claude/skills/<name>/

Priority 3 — Registry:
  zeroclaw-skills repo (auto-synced)

Priority 4 — Community:
  open-skills repo (auto-synced if enabled)
```

Each root is scanned for directories containing `SKILL.md` (required) and `manifest.toml` (optional). The index only reads `manifest.toml` for metadata — SKILL.md content is not read until the skill is activated.

### 6.2 Loading: Full Skill Resolution

When `Skill({ skill: "auto-coder" })` is called:

1. **Resolve path** — Search skill roots in priority order for a matching name (directory name or frontmatter `name:` field).
2. **Read manifest.toml** — Parse `[skill]` section for metadata and permissions.
3. **Read SKILL.md** — Parse frontmatter, extract body as instruction content.
4. **Validate** — Check that all declared `permissions` are satisfiable by the current tool registry.
5. **Activate** — Push to `active_skills`, which triggers system prompt rebuild on next turn.

### 6.3 Permission Checking

The `permissions` field in manifest.toml declares which built-in tools the skill needs:

```toml
permissions = ["file_read", "file_write", "shell_exec"]
```

At activation time, the agent checks each permission against the current tool registry:

```rust
fn check_permissions(skill: &Skill, tools: &[Box<dyn Tool>]) -> Result<(), String> {
    let permission_map = [
        ("file_read",     &["Read", "file_read"] as &[&str]),
        ("file_write",    &["Write", "Edit", "file_write"]),
        ("shell_exec",    &["Bash", "shell_exec"]),
        ("web_search",    &["WebSearch", "web_search"]),
        ("web_fetch",     &["WebFetch", "web_fetch"]),
        ("channel_telegram", &["Telegram"]),
        ("channel_slack",    &["Slack"]),
        ("channel_discord",  &["Discord"]),
    ];

    for perm in &skill.permissions {
        let allowed_names = permission_map.iter()
            .find(|(p, _)| p == perm)
            .map(|(_, names)| *names)
            .unwrap_or(&[perm.as_str()]);

        let satisfied = tools.iter()
            .any(|t| allowed_names.contains(&t.name()));

        if !satisfied {
            return Err(format!(
                "Skill '{}' requires '{}' but no matching tool is available.",
                skill.name, perm
            ));
        }
    }
    Ok(())
}
```

---

## 7. Skill Deactivation

Skills can be deactivated to free context:

```rust
fn execute_skill_deactivate(input: SkillDeactivateInput, agent: &mut Agent) -> Result<String, String> {
    let before = agent.active_skills.len();
    agent.active_skills.retain(|s| s.name != input.skill);
    if agent.active_skills.len() == before {
        return Err(format!("Skill '{}' is not active.", input.skill));
    }
    Ok(format!("Skill '{}' deactivated. Its instructions have been removed from the system prompt.", input.skill))
}
```

**Cache consideration:** Deactivation modifies the system prompt structure (removes a segment). This invalidates KV cache for the removed and all subsequent segments. To minimize impact, deactivation should be rare — typically only when the user explicitly switches tasks or context budget is tight.

An alternative is to keep deactivated skills as empty segments (preserving cache structure) and mark them as inactive. This avoids cache invalidation at the cost of wasted segment slots:

```rust
// Soft deactivation: keep segment, mark inactive
struct ActiveSkill {
    skill: Skill,
    active: bool,   // false = instructions not rendered, segment preserved
}
```

---

## 8. Migration from Current Design

### 8.1 Code Removals

| File / Function | Action |
|-----------------|--------|
| `skills/mod.rs::skills_to_tools()` | **Delete** |
| `tools/skill_tool.rs` (SkillShellTool) | **Delete** |
| `tools/skill_http.rs` (SkillHttpTool) | **Delete** |
| `tools/mod.rs::register_skill_tools()` | **Delete** |
| `skills/mod.rs::SkillTool` struct | **Delete** |
| `skills/mod.rs::SkillManifest.tools` field | **Delete** |

### 8.2 Code Additions

| Component | Description |
|-----------|-------------|
| `SkillIndexEntry` struct | Compact index entry (name, description, triggers, permissions) |
| `load_skill_index()` | Scan roots, read manifest.toml only, return `Vec<SkillIndexEntry>` |
| `Skill` tool spec | Built-in tool for on-demand skill activation |
| `execute_skill()` | Load SKILL.md, check permissions, push to `active_skills` |
| `build_system_messages()` | Multi-segment system prompt with `cache_control` |
| `render_skill_index()` | XML index for `<available_skills>` |
| Permission checking | Runtime validation of `permissions` against tool registry |

### 8.3 Code Modifications

| Component | Change |
|-----------|--------|
| `Agent` struct | Add `skill_index: Vec<SkillIndexEntry>`, `active_skills: Vec<Skill>`; remove skill-related fields from `AgentBuilder` that fed `skills_to_tools()` |
| `Agent::from_config()` | Replace `load_skills_with_config()` + `register_skill_tools()` with `load_skill_index()` |
| `build_system_prompt()` | Replace with `build_system_messages()` returning `Vec<SystemMessage>` |
| `skills_to_prompt_with_mode()` | Replace with `render_skill_index()` (compact) + per-skill segments (full, on demand) |
| `SkillsConfig` | Remove `prompt_injection_mode` (no longer needed — always index-first, load-on-demand); add `triggers` field support in manifest parsing |
| Provider trait | Add `supports_cache_control() -> bool` method |

### 8.4 Manifest Format Changes

Current `manifest.toml`:

```toml
[skill]
name = "auto-coder"
version = "0.3.0"
permissions = ["file_read", "file_write", "shell_exec"]

[[tools]]          # ← REMOVE this section
name = "run_lint"
kind = "shell"
command = "cargo clippy"
```

New `manifest.toml`:

```toml
[skill]
name = "auto-coder"
version = "0.3.0"
permissions = ["file_read", "file_write", "shell_exec"]
triggers = ["write code", "implement feature", "make code changes"]
```

### 8.5 Backward Compatibility

- **SKILL.md files:** No change. Existing SKILL.md files work as-is.
- **manifest.toml without `triggers`:** `triggers` defaults to empty. The skill still appears in the index (by name and description), just without explicit trigger phrases.
- **manifest.toml with `[[tools]]`:** Parsed but ignored. A deprecation warning is logged at load time.
- **SKILL.toml format:** Deprecated. Skills should use `manifest.toml` + `SKILL.md`. A compatibility shim reads SKILL.toml and extracts the `[skill]` section as manifest metadata.

---

## 9. Comparison Summary

| Dimension | ZeroClaw (current) | Claw Code | This Proposal |
|-----------|-------------------|-----------|---------------|
| **Skill-Tool boundary** | Blurred (`skills_to_tools()`) | Clean | Clean |
| **Loading timing** | All at startup | On demand | On demand |
| **Context efficiency** | Low (all skills resident) | High | High |
| **LLM discoverability** | Good (all listed in prompt) | Poor (LLM unaware) | Good (index always visible) |
| **Skill content persistence** | Every turn (system prompt) | At risk (tool_result) | Every turn (system prompt) |
| **KV cache (Anthropic)** | Cached at init | N/A | Segmented, append-only |
| **KV cache (OpenAI-compat)** | Cached at init | N/A | MISS on load, re-stabilizes |
| **Dead code** | Yes (`skills_to_tools()`) | No | No |
| **Permissions** | Declared but unused | Not supported | Runtime checking |
| **Skill deactivation** | Not supported | Not supported | Supported |

---

## 10. Open Questions

1. **Skill deactivation cache cost.** Removing an active skill invalidates cache for that and subsequent segments. Should we use soft deactivation (keep empty segment) or accept the cache miss? Likely: accept the miss for now, optimize later if deactivation is frequent.

2. **Multiple skill interaction.** When two skills are active simultaneously, their instructions may conflict (e.g., one says "be concise", another says "provide detailed analysis"). Should the system detect and warn about conflicts, or leave it to the LLM to reconcile?

3. **Skill versioning in index.** Should the index show version numbers? Useful for debugging, but adds tokens. Likely: include in manifest.toml but omit from index rendering.

4. **Auto-activation.** Should the agent auto-activate a skill when the user's message matches trigger phrases, without waiting for the LLM to call the `Skill` tool? This would save one turn but reduces LLM autonomy. Likely: keep LLM-driven activation; auto-activation can be a future enhancement.

5. **Compact mode for active skills.** When context budget is tight, should active skill segments be compressed (e.g., keep only the current phase's instructions)? This would require parsing skill content into phases, which is out of scope for this design but could be added later.
