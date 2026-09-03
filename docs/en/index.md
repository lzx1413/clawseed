---
description: Open-source Rust runtime for Android AI agents with on-device execution, personas, user profiles, speech output, and cross-app tools through CETP.
---

# ClawSeed: Rust Mobile AI Agent Runtime

ClawSeed is an open-source Rust AI agent runtime for Android, mobile, and edge
devices. It combines a complete on-device Android app with personas, a structured
user profile, speech output, and tools supplied by the device or other apps.

## Download

[Download the latest ClawSeed release](https://github.com/lzx1413/clawseed/releases/latest){ .md-button .md-button--primary }

Open the release page and download the Android `.apk` asset for your device. All
published versions and release notes are available on the
[GitHub Releases page](https://github.com/lzx1413/clawseed/releases).

## A Complete Mobile Agent Stack

ClawSeed is a **runtime, not an application**. It provides crates with stable traits; applications compose them.

- **On-device Android runtime**: The Rust gateway, agent loop, memory, and tool system run on the phone
- **Personas**: Create focused assistants with their own Soul, model and thinking settings, private memory, tool access, and skills
- **Structured user profile**: Learn durable preferences after successful turns while preserving manual edits and filtering sensitive data
- **Speech output**: Read completed AI replies aloud with Android Text-to-Speech and per-chat playback controls
- **Cross-app tools**: Discover tools exposed by other Android apps through CETP and use them in the same agent loop
- **Built-in capabilities**: 25+ tools, on-demand skills, scheduled tasks, hybrid memory, and extended thinking
- **Provider choice**: Anthropic, Gemini, Bedrock, OpenAI-compatible, DeepSeek, Ollama, Groq, and more
- **Security controls**: Autonomy levels, command allowlists, path guards, approval hooks, and rate limiting

## Android Apps Become Agent Tools

ClawSeed is the unified AI interaction layer on the phone. A compatible Android
app can expose focused capabilities through the ClawSeed External Tool Protocol
(CETP); ClawSeed discovers those tools and presents them to the model alongside
its built-in and device tools.

```text
User -> ClawSeed persona -> CETP tool call -> Android app -> structured result -> answer
```

[Jiucaihua](https://github.com/lzx1413/jiucaihua-android) is a working integration.
ClawSeed can combine its portfolio, holdings, market quotes, K-line data, news,
transactions, and performance tools in one conversation. See the
[Jiucaihua integration guide](integrations/jiucaihua.md) for setup, example prompts,
and the boundary between read-only CETP v1 tools and alert-management extensions.

## Quick Links

- [Architecture Overview](architecture.md) — understand the crate structure and design principles
- [Build and Test](build-and-test.md) — get started with building the project
- [Modules](modules/index.md) — dive into each crate's internals
- [Tutorials](tutorials/index.md) — hands-on guides for extending ClawSeed
- [Android Demo](android-demo.md) — run the full agent stack on-device
- [CETP v1](external-tool-protocol.md) / [experimental v2 specification](external-tool-protocol-v2.md) — Android cross-app tool protocol
- [Jiucaihua Integration](integrations/jiucaihua.md) — use investment data from another Android app through CETP
- [Download Releases](https://github.com/lzx1413/clawseed/releases) — get the latest Android APK and release notes

## Architecture

```
clawseed-api (traits only, no impls)
  ← clawseed-agent (orchestration + runtime assembly)
    ← clawseed-tools (25+ built-in tools)
    ← clawseed-providers (LLM backends)
    ← clawseed-memory (SQLite + vector/keyword search)
      ← clawseed-gateway (Axum HTTP/WS server, remote tool bridge)
  ← clawseed-config (TOML config)
  ← clawseed (CLI binary)

clients/android (Kotlin/Compose demo app)
```
