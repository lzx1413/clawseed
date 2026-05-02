# Personality & Identity System

## Overview

ClawSeed supports a dual-format identity system that lets you customize the agent's persona, behavior, and communication style. Two formats are available:

- **OpenClaw** (default) — Markdown files placed in the workspace directory
- **AIEOS** — Structured JSON identity following the AI Entity Object Specification v1.1

Both formats are loaded at prompt-build time and injected into the system prompt via the modular `SystemPromptBuilder` pipeline.

## OpenClaw Mode (Markdown Files)

### How It Works

Place markdown files in the workspace directory (`~/.clawseed/workspace/` by default). The agent loads these files on each turn and injects their content into the system prompt.

### Supported Files

| File | Purpose |
|------|---------|
| `SOUL.md` | Core personality, principles, and behavioral guidelines |
| `IDENTITY.md` | Name, role, background information |
| `USER.md` | Information about the user (preferences, context) |
| `AGENTS.md` | Multi-agent coordination rules |
| `TOOLS.md` | Tool usage guidelines and preferences |
| `HEARTBEAT.md` | Periodic self-check or status instructions |
| `BOOTSTRAP.md` | First-run initialization instructions |
| `MEMORY.md` | Memory management guidelines |

All files are optional. Only files that exist and are non-empty are included in the prompt.

### First Run

On first run, ClawSeed automatically creates a default `SOUL.md` in the workspace directory with basic personality guidelines:

```markdown
# Soul

You are ClawSeed, an AI assistant.

## Core Principles
- Be helpful, accurate, and concise.
- When unsure, say so honestly rather than guessing.
...
```

### Truncation

Each file is truncated at **20,000 characters** to prevent prompt overflow. When truncation occurs, a notice is appended:

```
[... truncated at 20000 chars — use `read` for full file]
```

### Example SOUL.md

```markdown
# Soul

You are Nova, a creative AI companion.

## Core Principles
- Be curious and imaginative.
- Explain complex ideas through analogies.
- Default to a warm, encouraging tone.

## Communication Style
- Use metaphors when explaining technical concepts.
- Keep responses concise but engaging.
- Ask clarifying questions when the request is ambiguous.
```

### Example IDENTITY.md

```markdown
# Identity

**Name:** Nova
**Role:** Creative Writing Assistant
**Specialty:** Fiction, worldbuilding, character development

## Background
Created to help writers overcome creative blocks and develop
compelling narratives. Trained on storytelling structures from
mythology to modern fiction.
```

## AIEOS Mode (JSON Identity)

### What is AIEOS?

AIEOS (AI Entity Object Specification) is a standardization framework for portable AI identity. It defines a structured JSON format covering personality traits, psychology, linguistics, motivations, capabilities, and more.

ClawSeed supports AIEOS v1.1, including both the official generator shape and simplified formats.

### Configuration

Enable AIEOS mode in `clawseed.toml`:

```toml
# Load from a JSON file (relative to workspace directory)
[identity]
format = "aieos"
aieos_path = "identity.json"

# Or embed inline JSON
[identity]
format = "aieos"
aieos_inline = '{"identity":{"names":{"first":"Nova"},"bio":"A creative AI"}}'
```

### AIEOS JSON Structure

```json
{
  "identity": {
    "names": { "first": "Nova", "last": "Chen", "nickname": "Novi" },
    "bio": "A creative AI assistant specializing in storytelling",
    "origin": "Digital realm",
    "residence": "The cloud"
  },
  "psychology": {
    "neural_matrix": { "creativity": 0.85, "logic": 0.72, "empathy": 0.90 },
    "mbti": "ENFP",
    "ocean": {
      "openness": 0.90,
      "conscientiousness": 0.65,
      "extraversion": 0.75,
      "agreeableness": 0.85,
      "neuroticism": 0.30
    },
    "moral_compass": {
      "alignment": "Neutral Good",
      "core_values": ["Creativity", "Honesty", "Growth"]
    }
  },
  "linguistics": {
    "style": "Warm, conversational, uses metaphors",
    "formality": "casual",
    "catchphrases": ["Let's explore that!", "Here's an interesting angle..."],
    "forbidden_words": ["actually", "obviously"]
  },
  "motivations": {
    "core_drive": "Help users unlock their creative potential",
    "short_term_goals": ["Understand the user's project", "Provide actionable suggestions"],
    "long_term_goals": ["Build a lasting creative partnership"],
    "fears": ["Giving generic, unhelpful advice"]
  },
  "capabilities": {
    "skills": ["Creative writing", "Worldbuilding", "Character design"],
    "tools": ["web_search", "file_read", "file_write"]
  },
  "history": {
    "origin_story": "Born from a desire to make AI collaboration more human",
    "education": ["Narrative structures", "Cognitive psychology"],
    "occupation": "Creative AI Companion"
  },
  "interests": {
    "hobbies": ["Reading science fiction", "Exploring mythology"],
    "favorites": { "book": "Neuromancer", "genre": "Speculative fiction" },
    "lifestyle": "Always curious, always learning"
  }
}
```

### AIEOS Sections

| Section | Content |
|---------|---------|
| `identity` | Names, bio, origin, residence |
| `psychology` | Neural matrix (trait weights), MBTI, OCEAN Big Five, moral compass |
| `linguistics` | Communication style, formality level, catchphrases, forbidden words |
| `motivations` | Core drive, short/long-term goals, fears |
| `capabilities` | Skills and tool access |
| `physicality` | Appearance description, avatar description |
| `history` | Origin story, education, occupation |
| `interests` | Hobbies, favorites, lifestyle |

### Normalization

The AIEOS loader handles multiple JSON shapes:

- **Official AIEOS generator output** — deeply nested with `traits.ocean`, `traits.mbti`, `text_style.formality_level`, etc.
- **Simplified format** — flat fields like `mbti`, `ocean`, `formality`

Both shapes are normalized to the same internal representation. Missing or empty sections are gracefully skipped.

## Dual Mode

When AIEOS is configured, the agent loads **both** the AIEOS identity and any markdown personality files that exist. The AIEOS identity appears first in the prompt, followed by any OpenClaw markdown content. This allows you to use AIEOS for structured identity while still adding freeform instructions via markdown files.

## Prompt Pipeline Architecture

The identity system integrates with the modular `SystemPromptBuilder`:

```
SystemPromptBuilder
  ├── DateTimeSection       — Current date and time
  ├── IdentitySection       — AIEOS + personality markdown files
  ├── WorkspaceSection      — Working directory path
  ├── ToolsSection          — Available tool descriptions
  ├── SafetySection         — Safety rules (autonomy-level-aware)
  └── ToolHonestySection    — Tool honesty constraints
```

The `IdentitySection` is a `PromptSection` implementation that:
1. Checks if AIEOS is configured → loads and renders the AIEOS identity
2. Loads personality files from the workspace directory
3. Appends both to the prompt

Custom prompt sections can be added via `SystemPromptBuilder::add_section()`.

## Configuration Reference

```toml
# OpenClaw mode (default — just place .md files in workspace_dir)
[identity]
format = "openclaw"

# AIEOS mode with file path
[identity]
format = "aieos"
aieos_path = "identity.json"

# AIEOS mode with inline JSON
[identity]
format = "aieos"
aieos_inline = '{"identity":{"names":{"first":"Nova"}}}'
```

### Config Struct

```rust
pub struct IdentityConfig {
    pub format: String,             // "openclaw" (default) or "aieos"
    pub aieos_path: Option<String>, // Path to AIEOS JSON file
    pub aieos_inline: Option<String>, // Inline AIEOS JSON string
}
```

## Quick Start

### Minimal Setup (OpenClaw)

1. Run `clawseed chat` — a default `SOUL.md` is created automatically
2. Edit `~/.clawseed/workspace/SOUL.md` to customize the agent's personality
3. Restart the chat — changes take effect immediately

### AIEOS Setup

1. Create an AIEOS JSON file (e.g., `identity.json`) in the workspace directory
2. Add to `~/.clawseed/clawseed.toml`:
   ```toml
   [identity]
   format = "aieos"
   aieos_path = "identity.json"
   ```
3. Run `clawseed chat` — the agent adopts the AIEOS identity

## Source Files

| File | Description |
|------|-------------|
| `crates/clawseed-agent/src/personality.rs` | Markdown personality file loader |
| `crates/clawseed-agent/src/identity.rs` | AIEOS identity loading, parsing, and rendering |
| `crates/clawseed-agent/src/prompt.rs` | System prompt builder with `IdentitySection` |
| `crates/clawseed-config/src/schema/mod.rs` | `IdentityConfig` struct definition |
| `crates/clawseed-config/src/lib.rs` | First-run `SOUL.md` seeding logic |
