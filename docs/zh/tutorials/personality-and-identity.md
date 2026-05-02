# 人格与身份系统

## 概述

ClawSeed 支持双格式身份系统，允许你自定义 Agent 的人格、行为和沟通风格。提供两种格式：

- **OpenClaw**（默认）— 放置在工作区目录中的 Markdown 文件
- **AIEOS** — 遵循 AI Entity Object Specification v1.1 的结构化 JSON 身份

两种格式在构建提示时加载，通过模块化的 `SystemPromptBuilder` 管线注入系统提示。

## OpenClaw 模式（Markdown 文件）

### 工作方式

将 Markdown 文件放置在工作区目录（默认为 `~/.clawseed/workspace/`）。Agent 在每次对话时加载这些文件并将其内容注入系统提示。

### 支持的文件

| 文件 | 用途 |
|------|------|
| `SOUL.md` | 核心人格、原则和行为准则 |
| `IDENTITY.md` | 名称、角色、背景信息 |
| `USER.md` | 用户信息（偏好、上下文） |
| `AGENTS.md` | 多 Agent 协调规则 |
| `TOOLS.md` | 工具使用指南和偏好 |
| `HEARTBEAT.md` | 定期自检或状态指令 |
| `BOOTSTRAP.md` | 首次运行初始化指令 |
| `MEMORY.md` | 记忆管理指南 |

所有文件均为可选。只有存在且非空的文件才会被包含在提示中。

### 首次运行

首次运行时，ClawSeed 会在工作区目录自动创建默认的 `SOUL.md`，包含基本的人格指南：

```markdown
# Soul

You are ClawSeed, an AI assistant.

## Core Principles
- Be helpful, accurate, and concise.
- When unsure, say so honestly rather than guessing.
...
```

### 截断

每个文件在 **20,000 字符** 处截断，防止提示溢出。截断时会附加提示：

```
[... truncated at 20000 chars — use `read` for full file]
```

### SOUL.md 示例

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

### IDENTITY.md 示例

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

## AIEOS 模式（JSON 身份）

### 什么是 AIEOS？

AIEOS（AI Entity Object Specification）是一个可移植 AI 身份的标准化框架。它定义了一种结构化 JSON 格式，涵盖人格特质、心理学、语言学、动机、能力等维度。

ClawSeed 支持 AIEOS v1.1，兼容官方生成器输出和简化格式。

### 配置

在 `clawseed.toml` 中启用 AIEOS 模式：

```toml
# 从 JSON 文件加载（相对于工作区目录）
[identity]
format = "aieos"
aieos_path = "identity.json"

# 或嵌入内联 JSON
[identity]
format = "aieos"
aieos_inline = '{"identity":{"names":{"first":"Nova"},"bio":"A creative AI"}}'
```

### AIEOS JSON 结构

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

### AIEOS 分节

| 分节 | 内容 |
|------|------|
| `identity` | 名称、简介、出身、居住地 |
| `psychology` | 神经矩阵（特质权重）、MBTI、OCEAN 大五人格、道德罗盘 |
| `linguistics` | 沟通风格、正式程度、口头禅、禁止用词 |
| `motivations` | 核心驱动力、短期/长期目标、恐惧 |
| `capabilities` | 技能和工具访问 |
| `physicality` | 外貌描述、头像描述 |
| `history` | 起源故事、教育背景、职业 |
| `interests` | 爱好、偏好、生活方式 |

### 归一化

AIEOS 加载器处理多种 JSON 结构：

- **官方 AIEOS 生成器输出** — 深度嵌套，如 `traits.ocean`、`traits.mbti`、`text_style.formality_level` 等
- **简化格式** — 扁平字段如 `mbti`、`ocean`、`formality`

两种结构都被归一化为相同的内部表示。缺失或为空的分节会被优雅地跳过。

## 双模式

当配置了 AIEOS 时，Agent 会**同时**加载 AIEOS 身份和任何存在的 Markdown 人格文件。AIEOS 身份在提示中优先出现，随后是 OpenClaw Markdown 内容。这允许你使用 AIEOS 定义结构化身份，同时通过 Markdown 文件添加自由格式的指令。

## 提示管线架构

身份系统与模块化的 `SystemPromptBuilder` 集成：

```
SystemPromptBuilder
  ├── DateTimeSection       — 当前日期和时间
  ├── IdentitySection       — AIEOS + 人格 Markdown 文件
  ├── WorkspaceSection      — 工作目录路径
  ├── ToolsSection          — 可用工具描述
  ├── SafetySection         — 安全规则（感知自主等级）
  └── ToolHonestySection    — 工具诚实性约束
```

`IdentitySection` 是 `PromptSection` 的实现，执行流程：
1. 检查是否配置了 AIEOS → 加载并渲染 AIEOS 身份
2. 从工作区目录加载人格文件
3. 将两者追加到提示中

可通过 `SystemPromptBuilder::add_section()` 添加自定义提示分节。

## 配置参考

```toml
# OpenClaw 模式（默认 — 只需将 .md 文件放入 workspace_dir）
[identity]
format = "openclaw"

# AIEOS 模式，指定文件路径
[identity]
format = "aieos"
aieos_path = "identity.json"

# AIEOS 模式，内联 JSON
[identity]
format = "aieos"
aieos_inline = '{"identity":{"names":{"first":"Nova"}}}'
```

### 配置结构体

```rust
pub struct IdentityConfig {
    pub format: String,             // "openclaw"（默认）或 "aieos"
    pub aieos_path: Option<String>, // AIEOS JSON 文件路径
    pub aieos_inline: Option<String>, // 内联 AIEOS JSON 字符串
}
```

## 快速开始

### 最简设置（OpenClaw）

1. 运行 `clawseed chat` — 自动创建默认 `SOUL.md`
2. 编辑 `~/.clawseed/workspace/SOUL.md` 自定义 Agent 人格
3. 重新启动聊天 — 更改立即生效

### AIEOS 设置

1. 在工作区目录中创建 AIEOS JSON 文件（如 `identity.json`）
2. 在 `~/.clawseed/clawseed.toml` 中添加：
   ```toml
   [identity]
   format = "aieos"
   aieos_path = "identity.json"
   ```
3. 运行 `clawseed chat` — Agent 采用 AIEOS 身份

## 源文件

| 文件 | 说明 |
|------|------|
| `crates/clawseed-agent/src/personality.rs` | Markdown 人格文件加载器 |
| `crates/clawseed-agent/src/identity.rs` | AIEOS 身份加载、解析和渲染 |
| `crates/clawseed-agent/src/prompt.rs` | 系统提示构建器与 `IdentitySection` |
| `crates/clawseed-config/src/schema/mod.rs` | `IdentityConfig` 结构体定义 |
| `crates/clawseed-config/src/lib.rs` | 首次运行 `SOUL.md` 生成逻辑 |
