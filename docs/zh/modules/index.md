# 模块

ClawSeed 采用 Cargo workspace 组织，各 crate 之间依赖关系清晰。

| Crate | 角色 |
|-------|------|
| [clawseed-api](api.md) | 核心 trait 定义（纯接口，无实现） |
| [clawseed-agent](agent.md) | Agent 循环、Hook、分发、运行时组装 |
| [clawseed-tools](tools.md) | 25+ 内置工具实现 |
| [clawseed-providers](providers.md) | LLM Provider 实现 |
| [clawseed-memory](memory.md) | SQLite 记忆 + 向量搜索 |
| [clawseed-config](config.md) | TOML 配置 schema 与加载 |
| [clawseed-gateway](gateway.md) | Axum HTTP/WS 服务器 + 远程工具桥 |
