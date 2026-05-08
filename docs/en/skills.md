# Skill System

ClawSeed's skill system lets you define reusable workflows that the LLM can load on demand. Skills are not tools — they orchestrate tools through declared permissions.

## Core Concepts

**Skill vs Tool**: A tool is an atomic operation (read a file, run a command). A skill is a multi-step workflow that guides the LLM through a structured process. Skills *consume* tools; they never *become* tools.

**Index-first, load-on-demand**: The LLM sees a compact skill index in every system prompt. Full instructions are loaded only when the LLM calls the `Skill` tool. This keeps the base prompt small while making skills discoverable.

**System prompt persistence**: Once activated, skill content is injected into the system prompt (not conversation history). This means skill instructions survive context compaction — the LLM always has access to the workflow.

## Architecture

```
┌─ Agent Init ──────────────────────────────────────────┐
│                                                         │
│  load_skill_index()  →  Vec<SkillIndexEntry>           │
│  active_skills       =  []                              │
│  system_prompt       =  base + skill_index              │
│                                                         │
└─────────────────────────────────────────────────────────┘
                          │
                          ▼
┌─ Agent Loop ───────────────────────────────────────────┐
│                                                         │
│  1. build_system_prompt()                               │
│     → base prompt + skill_index + active_skills         │
│                                                         │
│  2. provider.chat(messages, tool_specs)                  │
│     tool_specs includes the Skill tool                   │
│                                                         │
│  3. Parse response → (text, tool_calls)                 │
│                                                         │
│  4. For each tool_call:                                 │
│     ├─ "Skill" → intercept in agent loop                │
│     │    ├─ activate: load + permissions + prompt inject │
│     │    └─ deactivate: remove + prompt rebuild          │
│     │                                                    │
│     └─ other → tool_registry.find(name).execute()       │
│                                                         │
│  5. Format results → append to history                  │
│  6. Loop until no tool_calls or max_iterations          │
│                                                         │
└─────────────────────────────────────────────────────────┘
```

## Execution Flow

### Agent Initialization

```
1. load_skill_index(workspace_dir, config)
     → scan skill root directories in priority order
     → read manifest.toml from each skill directory
     → extract name, description, triggers, permissions
     → deduplicate by effective name (higher-priority root wins)
     → return Vec<SkillIndexEntry>

2. active_skills = Vec::new()

3. build_system_prompt()
     → base prompt + <available_skills> index
```

### Skill Activation

```
User: "Help me implement a feature"
  │
  ▼
LLM sees <available_skills> → matches "implement feature" trigger → auto-coder
  │
  ▼
tool_call: Skill({ "skill": "auto-coder" })
  │
  ▼
Agent intercepts the Skill tool call:
  1. Check already active? → No
  2. load_skill_by_name("auto-coder") → read manifest.toml + SKILL.md
  3. Check permissions ["file_read", "file_write", "shell_exec"] → all satisfied
  4. active_skills.push(auto-coder)
  5. rebuild_system_prompt() → base + index + <active_skill> content
  6. Return: "Skill 'auto-coder' activated."
  │
  ▼
LLM now sees full auto-coder instructions in system prompt
LLM follows the workflow steps using available tools
```

### Subsequent Turns

The system prompt includes active skill content on every turn. The LLM always has access to the workflow instructions — no re-activation needed.

### Skill Deactivation

```
tool_call: Skill({ "skill": "auto-coder", "action": "deactivate" })
  │
  ▼
Agent removes skill from active_skills, rebuilds system prompt
Return: "Skill 'auto-coder' deactivated."
```

## Skill Discovery

Skills are discovered from multiple root directories in priority order:

| Priority | Path | Scope |
|----------|------|-------|
| 1 (highest) | `<workspace>/.clawseed/skills/` | Project-level |
| 2 | `<workspace>/.claude/skills/` | Project-level (Claude Code compat) |
| 3 | `~/.clawseed/skills/` | User-level |
| 4 (lowest) | `~/.claude/skills/` | User-level (Claude Code compat) |

Additional roots can be configured via `config.toml`:

```toml
[skills]
extra_roots = ["/opt/shared-skills", "/home/user/my-skills"]
```

On name collision, the higher-priority root wins. Skills are identified by their manifest `name` field, not directory name.

## Permission System

Skills declare which tools they need via the `permissions` field in manifest.toml. At activation time, the agent checks each permission against the current tool registry:

| Permission | Mapped Tool Names |
|-----------|-------------------|
| `file_read` | `file_read` |
| `file_write` | `file_write`, `file_edit` |
| `shell_exec` | `shell` |
| `web_search` | `web_search` |
| `web_fetch` | `web_fetch` |
| `http_request` | `http_request` |
| `memory` | `memory_store`, `memory_recall`, `memory_export`, `memory_forget`, `memory_purge` |
| `knowledge` | `knowledge` |
| `calculator` | `calculator` |
| `git` | `git` |
| `cron` | `cron_add`, `cron_list`, `cron_remove`, `cron_run`, `cron_runs`, `cron_update` |
| `backup` | `backup` |
| `glob_search` | `glob_search` |
| `content_search` | `content_search` |
| `llm_task` | `llm_task` |
| `pdf_read` | `pdf_read` |

Unknown permissions fall through to exact tool name match. If a skill requires a permission that no available tool satisfies, activation fails with a clear error message.

## Configuration

```toml
[skills]
enabled = true                # Enable/disable skill system (default: true)
max_active = 5                # Maximum concurrent active skills (default: 5)
excluded = ["legacy-skill"]   # Skills to exclude from the index
extra_roots = []              # Additional skill root directories
```

## Skill Manifest Format

### manifest.toml

```toml
[skill]
name = "auto-coder"
version = "0.3.0"
author = "your-name"
description = "Autonomous code generation agent. Reads context, writes code, runs tests."
category = "coding"
tags = ["Official", "Featured"]
license = "MIT"
permissions = ["file_read", "file_write", "shell_exec"]
triggers = ["write code", "implement feature", "make code changes"]
```

All fields except `name` are optional. The `[[tools]]` section from earlier designs is deprecated and ignored — a warning is logged if present.

### SKILL.md

```markdown
---
name: auto-coder
description: "Autonomous code generation agent..."
version: 0.3.0
tags: [coding, official]
permissions: [file_read, file_write, shell_exec]
triggers: [write code, implement feature]
---

# Auto Coder

You are an autonomous coding agent.

## Workflow
1. **Understand the task.** Read the user's request carefully.
2. **Read before you write.** Use `file_read` to examine existing code.
3. **Plan your changes.** ...
```

SKILL.md supports YAML frontmatter delimited by `---`. Lists support both inline (`[a, b]`) and block style. When both `manifest.toml` and SKILL.md frontmatter exist, `manifest.toml` takes precedence.

## System Prompt Rendering

### Skill Index (always present)

```xml
<available_skills>
  <skill name="auto-coder" triggers="write code, implement feature">
    Autonomous code generation. Reads context, writes code, runs tests.
  </skill>
  <skill name="web-researcher" triggers="research, find information">
    Deep web research with source citation.
  </skill>
</available_skills>
```

### Active Skill Content (when activated)

```xml
<active_skill name="auto-coder">
# Auto Coder

You are an autonomous coding agent.

## Workflow
1. **Understand the task.** ...
</active_skill>
```

## Skill Tool API

The built-in `Skill` tool accepts two parameters:

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `skill` | string | Yes | Exact skill name from `<available_skills>` |
| `action` | string | No | `"activate"` (default) or `"deactivate"` |

The agent intercepts Skill tool calls in its turn loop. Activation loads the full skill content, checks permissions, and rebuilds the system prompt. The tool result contains a human-readable confirmation — no internal JSON is exposed to the LLM, hooks, or observers.
