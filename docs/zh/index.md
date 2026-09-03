---
description: 面向 Android 的开源 Rust AI Agent 运行时，支持端内运行、分身、用户画像、语音播报，并通过 CETP 调用其他 App 的工具。
---

# ClawSeed：Rust 移动端 AI Agent 运行时

ClawSeed 是面向 Android、移动端与边缘设备的开源 Rust AI Agent 运行时。它把完整的
Android 端内应用、分身系统、结构化用户画像、语音播报和跨应用工具调用组合在一起。

## 下载

[下载最新 ClawSeed 版本](https://github.com/lzx1413/clawseed/releases/latest){ .md-button .md-button--primary }

进入 Release 页面后，选择适合设备的 Android `.apk` 文件。所有历史版本和更新说明可在
[GitHub Releases 页面](https://github.com/lzx1413/clawseed/releases)查看。

## 完整的移动端 Agent 能力

ClawSeed 是一个**运行时，而非应用**。它提供基于 trait 的 crate，应用自行组装。

- **Android 端内运行**：Rust Gateway、Agent 循环、记忆和工具系统直接运行在手机上
- **分身系统**：为不同任务设置专属 Soul、模型与思考模式、独立记忆、工具权限和技能
- **结构化用户画像**：在成功回复后学习长期偏好，同时保留人工编辑结果并过滤敏感数据
- **语音播报**：通过 Android TTS 自动朗读完整回复，也可在会话中单独播放或停止
- **跨应用工具**：通过 CETP 发现其他 Android App 暴露的能力，并纳入同一个 Agent 工具循环
- **完整能力集**：25+ 内置工具、按需技能、定时任务、混合记忆和扩展思考模式
- **多 Provider 支持**：Anthropic、Gemini、Bedrock、OpenAI 兼容、DeepSeek、Ollama、Groq 等
- **安全控制**：自治级别、命令白名单、路径防护、审批 Hook 和速率限制

## 让 Android App 成为 Agent 工具

ClawSeed 可以作为手机上的统一 AI 交互层。兼容的 Android App 通过 ClawSeed External
Tool Protocol（CETP）暴露自身能力；ClawSeed 发现这些工具后，将它们与内置工具、设备工具
一起提供给模型调用。

```text
用户 -> ClawSeed 分身 -> CETP 工具调用 -> Android App -> 结构化结果 -> 最终回复
```

[九财花](https://github.com/lzx1413/jiucaihua-android)已经完成真实接入。ClawSeed 可以在一次
对话中组合调用九财花的投资组合、持仓、行情、K 线、资讯、交易流水和收益分析工具。安装方式、
示例提问，以及 CETP v1 只读工具与预警管理扩展之间的边界，参见
[九财花联动指南](integrations/jiucaihua.md)。

## 快速链接

- [架构概览](architecture.md) — 了解 crate 结构与设计原则
- [构建与测试](build-and-test.md) — 快速开始构建项目
- [模块](modules/index.md) — 深入每个 crate 的内部机制
- [教程](tutorials/index.md) — 扩展 ClawSeed 的实践指南
- [Android 示例](android-demo.md) — 在设备上运行完整的 Agent 栈
- [CETP v1](external-tool-protocol.md) / [v2 实验规范](external-tool-protocol-v2.md) — Android 跨应用工具协议
- [九财花联动](integrations/jiucaihua.md) — 通过 CETP 调用另一个 Android App 的投资工具
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
