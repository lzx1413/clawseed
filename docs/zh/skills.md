# Skill System

ClawSeed 的 Skill 系统让你可以定义可复用的工作流，LLM 可以按需加载。Skill 不是 Tool —— Skill 通过声明的 permissions 来编排 Tool。

## 核心概念

**Skill 与 Tool 的区别**：Tool 是原子操作（读文件、跑命令）。Skill 是多步骤工作流，引导 LLM 按结构化流程执行。Skill *消费* Tool，永远不会 *变成* Tool。

**索引优先，按需加载**：LLM 在每次对话的系统提示中看到一个精简的 Skill 索引。只有当 LLM 调用 `Skill` 工具时才加载完整指令。这保持了基础提示的精简，同时让 Skill 可被发现。

**系统提示持久化**：激活后，Skill 内容注入系统提示（而非对话历史）。这意味着 Skill 指令能存活过上下文压缩 —— LLM 始终能访问工作流。

## 架构

```
┌─ Agent 初始化 ────────────────────────────────────────┐
│                                                         │
│  load_skill_index()  →  Vec<SkillIndexEntry>           │
│  active_skills       =  []                              │
│  system_prompt       =  base + skill_index              │
│                                                         │
└─────────────────────────────────────────────────────────┘
                          │
                          ▼
┌─ Agent 循环 ───────────────────────────────────────────┐
│                                                         │
│  1. build_system_prompt()                               │
│     → base prompt + skill_index + active_skills         │
│                                                         │
│  2. provider.chat(messages, tool_specs)                  │
│     tool_specs 包含 Skill 工具                          │
│                                                         │
│  3. 解析响应 → (text, tool_calls)                       │
│                                                         │
│  4. 对每个 tool_call:                                   │
│     ├─ "Skill" → 在 agent 循环中拦截                    │
│     │    ├─ activate: 加载 + 权限检查 + 注入提示         │
│     │    └─ deactivate: 移除 + 重建提示                  │
│     │                                                    │
│     └─ 其他 → tool_registry.find(name).execute()        │
│                                                         │
│  5. 格式化结果 → 追加到历史                              │
│  6. 循环直到无 tool_calls 或达到最大迭代次数             │
│                                                         │
└─────────────────────────────────────────────────────────┘
```

## 执行流程

### Agent 初始化

```
1. load_skill_index(workspace_dir, config)
     → 按优先级扫描 Skill 根目录
     → 读取每个 Skill 目录的 manifest.toml
     → 提取 name、description、triggers、permissions
     → 按有效名称去重（高优先级根目录优先）
     → 返回 Vec<SkillIndexEntry>

2. active_skills = Vec::new()

3. build_system_prompt()
     → 基础提示 + <available_skills> 索引
```

### Skill 激活

```
用户: "帮我实现一个功能"
  │
  ▼
LLM 看到 <available_skills> → 匹配 "implement feature" 触发词 → auto-coder
  │
  ▼
tool_call: Skill({ "skill": "auto-coder" })
  │
  ▼
Agent 拦截 Skill 工具调用:
  1. 检查是否已激活? → 否
  2. load_skill_by_name("auto-coder") → 读取 manifest.toml + SKILL.md
  3. 检查权限 ["file_read", "file_write", "shell_exec"] → 全部满足
  4. active_skills.push(auto-coder)
  5. rebuild_system_prompt() → base + index + <active_skill> 内容
  6. 返回: "Skill 'auto-coder' activated."
  │
  ▼
LLM 在系统提示中看到完整的 auto-coder 指令
LLM 按工作流步骤使用可用工具执行
```

### 后续对话

系统提示在每次对话中包含已激活的 Skill 内容。LLM 始终能访问工作流指令 —— 无需重新激活。

### Skill 停用

```
tool_call: Skill({ "skill": "auto-coder", "action": "deactivate" })
  │
  ▼
Agent 从 active_skills 移除该 Skill，重建系统提示
返回: "Skill 'auto-coder' deactivated."
```

## Skill 发现

Skill 按优先级从多个根目录中发现：

| 优先级 | 路径 | 作用域 |
|--------|------|--------|
| 1（最高） | `<workspace>/.clawseed/skills/` | 项目级 |
| 2 | `<workspace>/.claude/skills/` | 项目级（Claude Code 兼容） |
| 3 | `~/.clawseed/skills/` | 用户级 |
| 4（最低） | `~/.claude/skills/` | 用户级（Claude Code 兼容） |

可通过 `config.toml` 配置额外根目录：

```toml
[skills]
extra_roots = ["/opt/shared-skills", "/home/user/my-skills"]
```

名称冲突时，高优先级根目录优先。Skill 按其 manifest 中的 `name` 字段标识，而非目录名。

## 权限系统

Skill 通过 manifest.toml 中的 `permissions` 字段声明所需工具。激活时，Agent 逐项检查权限是否与当前工具注册表匹配：

| 权限 | 对应工具名 |
|------|-----------|
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

未知权限会回退到精确工具名匹配。如果 Skill 需要的权限没有可用工具满足，激活会失败并给出清晰的错误信息。

## 配置

```toml
[skills]
enabled = true                # 启用/禁用 Skill 系统（默认: true）
max_active = 5                # 最大同时激活 Skill 数（默认: 5）
excluded = ["legacy-skill"]   # 从索引中排除的 Skill
extra_roots = []              # 额外的 Skill 根目录
```

## Skill 清单格式

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

除 `name` 外所有字段可选。早期设计中的 `[[tools]]` 部分已弃用并被忽略 —— 如果存在会记录警告。

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

SKILL.md 支持以 `---` 分隔的 YAML frontmatter。列表支持行内格式（`[a, b]`）和块格式。当 `manifest.toml` 和 SKILL.md frontmatter 同时存在时，`manifest.toml` 优先。

## 系统提示渲染

### Skill 索引（始终存在）

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

### 已激活 Skill 内容

```xml
<active_skill name="auto-coder">
# Auto Coder

You are an autonomous coding agent.

## Workflow
1. **Understand the task.** ...
</active_skill>
```

## Skill 工具 API

内置 `Skill` 工具接受两个参数：

| 参数 | 类型 | 必填 | 描述 |
|------|------|------|------|
| `skill` | string | 是 | `<available_skills>` 中的精确 Skill 名称 |
| `action` | string | 否 | `"activate"`（默认）或 `"deactivate"` |

Agent 在循环中拦截 Skill 工具调用。激活操作加载完整 Skill 内容、检查权限、重建系统提示。工具结果包含人类可读的确认信息 —— 不会向 LLM、Hooks 或 Observers 暴露内部 JSON。
