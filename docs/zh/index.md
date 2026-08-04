---
description: 面向 Android、移动端与边缘设备的开源 Rust AI Agent 运行时，提供完整 Android 端内应用与 WebSocket 远程设备工具调用。
---

# ClawSeed：Rust 移动端 AI Agent 运行时

ClawSeed 是面向 Android、移动端与边缘设备的开源 Rust AI Agent 运行时。它提供完整的
Android 端内参考应用、模块化 Agent 栈，以及基于 WebSocket 的远程设备工具调用能力。

## 下载

[下载最新 ClawSeed 版本](https://github.com/lzx1413/clawseed/releases/latest){ .md-button .md-button--primary }

进入 Release 页面后，选择适合设备的 Android `.apk` 文件。所有历史版本和更新说明可在
[GitHub Releases 页面](https://github.com/lzx1413/clawseed/releases)查看。

## 为什么选择 ClawSeed？

ClawSeed 是一个**运行时，而非应用**。它提供基于 trait 的 crate，应用自行组装。

- **多 Provider 支持**：Anthropic、Gemini、Bedrock、OpenAI 兼容、DeepSeek、Ollama、Groq
- **25+ 内置工具**：Shell、文件操作、记忆、Web 搜索等
- **远程工具调用**：移动端通过 WebSocket 注册并执行工具
- **Android 端内运行**：Rust Gateway 与完整 Agent 栈直接运行在手机上
- **混合记忆**：SQLite + BM25 关键词搜索 + 向量搜索，RRF 融合排序
- **安全模型**：自治级别、命令白名单、路径防护、速率限制
- **技能系统**：可复用的按需加载工作流

## 快速链接

- [架构概览](architecture.md) — 了解 crate 结构与设计原则
- [构建与测试](build-and-test.md) — 快速开始构建项目
- [模块](modules/index.md) — 深入每个 crate 的内部机制
- [教程](tutorials/index.md) — 扩展 ClawSeed 的实践指南
- [Android 示例](android-demo.md) — 在设备上运行完整的 Agent 栈
- [版本下载](https://github.com/lzx1413/clawseed/releases) — 获取最新 Android APK 与更新说明

## 架构

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
