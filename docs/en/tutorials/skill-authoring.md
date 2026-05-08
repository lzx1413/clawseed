# Skill Authoring Tutorial

This tutorial walks through creating, testing, and deploying custom skills for ClawSeed.

## What is a Skill?

A skill is a directory containing a `manifest.toml` (metadata) and a `SKILL.md` (workflow instructions). When the LLM encounters a user request matching a skill's trigger phrases, it calls the `Skill` tool to load the full instructions into its system prompt.

Skills orchestrate tools — they don't define new ones. A skill tells the LLM *how* to use existing tools to accomplish a multi-step workflow.

## Step 1: Create the Skill Directory

Place skills in one of these locations (higher priority wins on name collision):

```
<workspace>/.clawseed/skills/<skill-name>/     # Project-level (recommended)
<workspace>/.claude/skills/<skill-name>/        # Claude Code compatible
~/.clawseed/skills/<skill-name>/               # User-level
```

For this tutorial, create a project-level skill:

```bash
mkdir -p .clawseed/skills/code-reviewer
```

## Step 2: Write manifest.toml

Create `.clawseed/skills/code-reviewer/manifest.toml`:

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

### Field Reference

| Field | Required | Description |
|-------|----------|-------------|
| `name` | Yes | Unique identifier. Must match the name used in `Skill({ "skill": "..." })`. |
| `version` | No | Semantic version (default: `"0.1.0"`). |
| `description` | No | One-line summary shown in the skill index. |
| `author` | No | Author name. |
| `category` | No | Category for organization. |
| `tags` | No | Tags for filtering. |
| `license` | No | License identifier. |
| `permissions` | No | Required tool permissions. Checked at activation time. |
| `triggers` | No | Phrases that help the LLM decide when to activate this skill. |

## Step 3: Write SKILL.md

Create `.clawseed/skills/code-reviewer/SKILL.md`:

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

### Writing Effective Instructions

- **Be specific about tool usage.** Refer to tools by their clawseed names (`file_read`, `shell`).
- **Define a clear step order.** Numbered steps guide the LLM through the workflow.
- **Include decision criteria.** Tell the LLM when to proceed, when to ask the user, and when to stop.
- **Set scope boundaries.** State what the skill does NOT do to prevent scope creep.
- **Keep it concise.** Every token in SKILL.md is added to the system prompt when active.

## Step 4: Add Frontmatter (Optional)

If you prefer a single-file skill, you can embed metadata in SKILL.md frontmatter instead of using manifest.toml:

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

When both `manifest.toml` and SKILL.md frontmatter exist, `manifest.toml` takes precedence.

## Step 5: Verify the Skill

Start a chat session and check that the skill appears in the index:

```bash
clawseed chat
```

The skill index is included in the system prompt. You should see something like:

```xml
<available_skills>
  <skill name="code-reviewer" triggers="review code, code review, check my code">
    Systematic code review workflow. Analyzes changes, identifies issues, suggests improvements.
  </skill>
</available_skills>
```

If the skill doesn't appear, check:
- The directory is in a recognized skill root
- `manifest.toml` has a valid `[skill]` section with a `name` field
- The directory is not listed in `skills.excluded` in `config.toml`

## Step 6: Activate the Skill

In a chat session, ask the LLM to review code. The LLM will match the trigger phrase and call:

```json
{ "skill": "code-reviewer" }
```

Or you can explicitly request it:

```
Please review the code in src/main.rs
```

The LLM should activate the skill and follow the workflow steps defined in SKILL.md.

## Step 7: Deactivate the Skill

Skills remain active across turns. To deactivate:

```json
{ "skill": "code-reviewer", "action": "deactivate" }
```

Or just start a new conversation — skills are not persisted across sessions.

## Advanced: Permission Checking

If a skill requires a permission that isn't available (e.g., `web_search` when no search provider is configured), activation will fail:

```
Failed to activate skill 'my-skill': Skill 'my-skill' requires permission 'web_search' but no matching tool is available.
```

This prevents skills from being activated in environments where they can't function correctly.

### Available Permissions

| Permission | What it needs |
|-----------|---------------|
| `file_read` | File reading capability |
| `file_write` | File writing/editing capability |
| `shell_exec` | Shell command execution |
| `web_search` | Web search tool |
| `web_fetch` | URL fetching tool |
| `http_request` | HTTP client tool |
| `memory` | Memory store/recall tools |
| `glob_search` | File glob search |
| `content_search` | Content/grep search |
| `llm_task` | Sub-LLM task delegation |

## Advanced: Multi-Source Skills

You can share skills across projects by placing them in user-level directories or configuring extra roots:

```toml
# ~/.clawseed/config.toml
[skills]
extra_roots = ["/opt/team-skills"]
```

Skills in higher-priority roots override those with the same name in lower-priority roots. This lets you override a team-wide skill with a project-specific version.

## Advanced: Directory Name vs Skill Name

The directory name doesn't have to match the skill's effective name. The effective name comes from the `name` field in manifest.toml (or SKILL.md frontmatter):

```
.clawseed/skills/my-reviewer/     # Directory name: "my-reviewer"
  manifest.toml                   # name = "code-reviewer"  ← This is the effective name
  SKILL.md
```

In this case, the LLM would call `Skill({ "skill": "code-reviewer" })`, not `"my-reviewer"`.

## Advanced: Max Active Skills

The default limit is 5 concurrent active skills. Adjust in config:

```toml
[skills]
max_active = 3  # Reduce for smaller context windows
```

When the limit is reached, the LLM must deactivate a skill before activating another.

## Troubleshooting

| Problem | Solution |
|---------|----------|
| Skill not in index | Check directory path, manifest.toml `[skill]` section, and `name` field |
| Activation fails with permission error | Add the required tool to your config or remove the permission from the skill |
| Activation fails with max_active limit | Deactivate an active skill first, or increase `max_active` in config |
| Skill instructions not followed | Improve SKILL.md — be more specific about steps and decision criteria |
| Skill content lost after many turns | This shouldn't happen — active skills persist in the system prompt, not conversation history |
| `[[tools]]` warning in logs | Remove the deprecated `[[tools]]` section from manifest.toml — it's ignored |
