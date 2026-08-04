---
description: Open-source Rust runtime for Android, mobile, and on-device AI agents, with a complete Android app and remote device tools over WebSocket.
---

# ClawSeed: Rust Mobile AI Agent Runtime

ClawSeed is an open-source Rust AI agent runtime for Android, mobile, and edge
devices. It includes a complete on-device Android reference app, a modular agent
stack, and remote device tool execution over WebSocket.

## Download

[Download the latest ClawSeed release](https://github.com/lzx1413/clawseed/releases/latest){ .md-button .md-button--primary }

Open the release page and download the Android `.apk` asset for your device. All
published versions and release notes are available on the
[GitHub Releases page](https://github.com/lzx1413/clawseed/releases).

## Why ClawSeed?

ClawSeed is a **runtime, not an application**. It provides crates with stable traits; applications compose them.

- **Multi-provider**: Anthropic, Gemini, Bedrock, OpenAI-compatible, DeepSeek, Ollama, Groq
- **25+ built-in tools**: Shell, file operations, memory, web search, and more
- **Remote tool execution**: Mobile clients register and execute tools over WebSocket
- **On-device Android app**: The Rust gateway and complete agent stack run on the phone
- **Hybrid memory**: SQLite-backed with BM25 + vector search, RRF fusion
- **Security model**: Autonomy levels, command allowlists, path guards, rate limiting
- **Skill system**: Reusable workflows loaded on-demand

## Quick Links

- [Architecture Overview](architecture.md) — understand the crate structure and design principles
- [Build and Test](build-and-test.md) — get started with building the project
- [Modules](modules/index.md) — dive into each crate's internals
- [Tutorials](tutorials/index.md) — hands-on guides for extending ClawSeed
- [Android Demo](android-demo.md) — run the full agent stack on-device
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
