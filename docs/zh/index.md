# ClawSeed

Rust AI Agent 运行时，支持远程工具调用。

## 为什么选择 ClawSeed？

ClawSeed 是一个**运行时，而非应用**。它提供基于 trait 的 crate，应用自行组装。

- **多 Provider 支持**：Anthropic、Gemini、Bedrock、OpenAI 兼容、DeepSeek、Ollama、Groq
- **25+ 内置工具**：Shell、文件操作、记忆、Web 搜索等
- **远程工具调用**：移动端通过 WebSocket 注册并执行工具
- **混合记忆**：SQLite + BM25 关键词搜索 + 向量搜索，RRF 融合排序
- **安全模型**：自治级别、命令白名单、路径防护、速率限制
- **技能系统**：可复用的按需加载工作流

## 快速链接

- [架构概览](architecture.md) — 了解 crate 结构与设计原则
- [构建与测试](build-and-test.md) — 快速开始构建项目
- [模块](modules/index.md) — 深入每个 crate 的内部机制
- [教程](tutorials/index.md) — 扩展 ClawSeed 的实践指南
- [Android 示例](android-demo.md) — 在设备上运行完整的 Agent 栈

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
