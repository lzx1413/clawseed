# Skill 编写教程

本教程介绍如何创建、测试和部署 ClawSeed 的自定义 Skill。

## 什么是 Skill？

Skill 是一个包含 `manifest.toml`（元数据）和 `SKILL.md`（工作流指令）的目录。当 LLM 遇到匹配 Skill 触发词的用户请求时，会调用 `Skill` 工具将完整指令加载到系统提示中。

Skill 编排工具 —— 它们不定义新工具。Skill 告诉 LLM *如何*使用现有工具完成多步骤工作流。

## 第 1 步：创建 Skill 目录

将 Skill 放在以下位置之一（名称冲突时高优先级优先）：

```
<workspace>/.clawseed/skills/<skill-name>/     # 项目级（推荐）
<workspace>/.claude/skills/<skill-name>/        # Claude Code 兼容
~/.clawseed/skills/<skill-name>/               # 用户级
```

本教程创建一个项目级 Skill：

```bash
mkdir -p .clawseed/skills/code-reviewer
```

## 第 2 步：编写 manifest.toml

创建 `.clawseed/skills/code-reviewer/manifest.toml`：

```toml
[skill]
name = "code-reviewer"
version = "0.1.0"
description = "Systematic code review workflow. Analyzes changes, identifies issues, suggests improvements."
category = "coding"
tags = ["review", "quality"]
permissions = ["file_read", "shell_exec"]
triggers = ["review code", "code review", "check my code"]
```

### 字段说明

| 字段 | 必填 | 说明 |
|------|------|------|
| `name` | 是 | 唯一标识符。必须与 `Skill({ "skill": "..." })` 中使用的名称匹配。 |
| `version` | 否 | 语义化版本号（默认：`"0.1.0"`）。 |
| `description` | 否 | 在 Skill 索引中显示的一行摘要。 |
| `author` | 否 | 作者名称。 |
| `category` | 否 | 分类，用于组织。 |
| `tags` | 否 | 标签，用于过滤。 |
| `license` | 否 | 许可证标识。 |
| `permissions` | 否 | 所需工具权限。在激活时检查。 |
| `triggers` | 否 | 帮助 LLM 判断何时激活此 Skill 的短语。 |

## 第 3 步：编写 SKILL.md

创建 `.clawseed/skills/code-reviewer/SKILL.md`：

```markdown
# Code Reviewer

You are a systematic code reviewer. Follow this workflow for every review.

## Workflow

1. **Identify the scope.** Use `glob_search` to find the relevant files. Ask the user to clarify if the scope is ambiguous.

2. **Read the code.** Use `file_read` to examine each file. Start with the most recently changed files.

3. **Analyze for issues.** Check for:
   - Logic errors and edge cases
   - Security vulnerabilities (injection, XSS, path traversal)
   - Performance concerns (unnecessary allocations, O(n²) algorithms)
   - Missing error handling
   - Inconsistent style or naming

4. **Run static analysis.** Use `shell` to run linting and type checking:
   ```
   cargo clippy -- -W clippy::all
   cargo check
   ```

5. **Summarize findings.** Organize by severity:
   - **Critical**: Must fix before merge
   - **Warning**: Should fix, may cause issues
   - **Suggestion**: Nice to have improvements

6. **Suggest fixes.** For each finding, provide the specific code change needed.
```

### 编写有效指令的技巧

- **明确工具用法。** 使用 clawseed 工具名（`file_read`、`shell`）引用工具。
- **定义清晰的步骤顺序。** 编号步骤引导 LLM 按工作流执行。
- **包含判断条件。** 告诉 LLM 何时继续、何时询问用户、何时停止。
- **设定范围边界。** 说明 Skill 不做什么，防止范围蔓延。
- **保持简洁。** SKILL.md 中的每个 token 在激活时都会添加到系统提示中。

## 第 4 步：添加 Frontmatter（可选）

如果你更喜欢单文件 Skill，可以将元数据嵌入 SKILL.md frontmatter，而不使用 manifest.toml：

```markdown
---
name: code-reviewer
description: "Systematic code review workflow."
version: 0.1.0
tags: [review, quality]
permissions: [file_read, shell_exec]
triggers: [review code, code review, check my code]
---

# Code Reviewer

You are a systematic code reviewer...
```

当 `manifest.toml` 和 SKILL.md frontmatter 同时存在时，`manifest.toml` 优先。

## 第 5 步：验证 Skill

启动聊天会话，检查 Skill 是否出现在索引中：

```bash
clawseed chat
```

Skill 索引包含在系统提示中。你应该看到类似内容：

```xml
<available_skills>
  <skill name="code-reviewer" triggers="review code, code review, check my code">
    Systematic code review workflow. Analyzes changes, identifies issues, suggests improvements.
  </skill>
</available_skills>
```

如果 Skill 未出现，检查：
- 目录是否在已识别的 Skill 根目录中
- `manifest.toml` 是否有有效的 `[skill]` 部分和 `name` 字段
- 目录是否未被 `config.toml` 中的 `skills.excluded` 排除

## 第 6 步：激活 Skill

在聊天会话中，让 LLM 审查代码。LLM 会匹配触发词并调用：

```json
{ "skill": "code-reviewer" }
```

或者直接请求：

```
Please review the code in src/main.rs
```

LLM 应该会激活 Skill 并按照 SKILL.md 中定义的工作流步骤执行。

## 第 7 步：停用 Skill

Skill 在多轮对话中保持激活。要停用：

```json
{ "skill": "code-reviewer", "action": "deactivate" }
```

或者直接开始新对话 —— Skill 不会跨会话持久化。

## 进阶：权限检查

如果 Skill 需要的权限不可用（例如，没有配置搜索提供商时的 `web_search`），激活会失败：

```
Failed to activate skill 'my-skill': Skill 'my-skill' requires permission 'web_search' but no matching tool is available.
```

这防止了 Skill 在无法正常运行的环境中被激活。

### 可用权限

| 权限 | 所需能力 |
|------|---------|
| `file_read` | 文件读取能力 |
| `file_write` | 文件写入/编辑能力 |
| `shell_exec` | Shell 命令执行 |
| `web_search` | 网络搜索工具 |
| `web_fetch` | URL 抓取工具 |
| `http_request` | HTTP 客户端工具 |
| `memory` | 记忆存储/召回工具 |
| `glob_search` | 文件 glob 搜索 |
| `content_search` | 内容/ grep 搜索 |
| `llm_task` | 子 LLM 任务委托 |

## 进阶：多源 Skill

通过将 Skill 放在用户级目录或配置额外根目录，可以跨项目共享 Skill：

```toml
# ~/.clawseed/clawseed.toml
[skills]
extra_roots = ["/opt/team-skills"]
```

高优先级根目录中的 Skill 会覆盖低优先级中同名的 Skill。这让你可以用项目特定版本覆盖团队通用 Skill。

## 进阶：目录名与 Skill 名

目录名不必与 Skill 的有效名称匹配。有效名称来自 manifest.toml（或 SKILL.md frontmatter）中的 `name` 字段：

```
.clawseed/skills/my-reviewer/     # 目录名: "my-reviewer"
  manifest.toml                   # name = "code-reviewer"  ← 这是有效名称
  SKILL.md
```

在这种情况下，LLM 会调用 `Skill({ "skill": "code-reviewer" })`，而不是 `"my-reviewer"`。

## 进阶：最大激活数

默认限制为同时激活 5 个 Skill。可在配置中调整：

```toml
[skills]
max_active = 3  # 较小的上下文窗口可减小此值
```

达到限制时，LLM 必须先停用一个 Skill 才能激活另一个。

## 常见问题

| 问题 | 解决方案 |
|------|---------|
| Skill 未出现在索引中 | 检查目录路径、manifest.toml `[skill]` 部分和 `name` 字段 |
| 激活失败，权限错误 | 在配置中添加所需工具，或从 Skill 中移除该权限 |
| 激活失败，max_active 限制 | 先停用一个已激活的 Skill，或在配置中增加 `max_active` |
| Skill 指令未被遵循 | 改进 SKILL.md —— 更具体地描述步骤和判断条件 |
| 多轮对话后 Skill 内容丢失 | 这不应该发生 —— 激活的 Skill 持久化在系统提示中，而非对话历史 |
| 日志中出现 `[[tools]]` 警告 | 从 manifest.toml 中移除已弃用的 `[[tools]]` 部分 —— 它会被忽略 |
