# ClawSeed 延期功能清单

本文档记录 clawseed 开发中裁剪的 zeroclaw 子系统，供后续按需设计和实现参考。每个子系统包含：功能说明、zeroclaw 源码位置与行数、裁剪原因、依赖关系、重新实现的关键要点。

**架构原则：** 扩展 import 核心，核心不 import 扩展。每个扩展 crate 通过实现 `Hook`、`Tool`、`ContextProvider` trait 注册到 Agent，核心代码零改动。详见 `docs/clawseed-development-plan.md` §2。

---

## 1. 安全子系统（Security）

### 1.1 完整安全策略（SecurityPolicy）

**功能：** 定义和执行 agent 行为约束——工具白名单/黑名单、文件路径沙箱、shell 命令过滤、网络访问控制。

**扩展注册方式：**
- `SecurityHook` 实现 `Hook` trait：before_tool_call 检查工具/路径/命令权限
- `SecurityContextProvider` 实现 `ContextProvider` trait：通过 `ctx.get::<SecurityPolicy>()` 暴露给工具
- 工具中通过 `ctx.get::<SecurityPolicy>()` 查询，有就检查，没有就跳过

**zeroclaw 源码：**

| 文件 | 行数 | 说明 |
|------|------|------|
| `crates/zeroclaw-config/src/policy/mod.rs` | 842 | SecurityPolicy struct 定义 |
| `crates/zeroclaw-config/src/policy/enforcement.rs` | 868 | 策略执行引擎 |
| `crates/zeroclaw-config/src/policy/tests.rs` | 1,974 | 153 个测试 |
| `crates/zeroclaw-config/src/domain_matcher.rs` | 259 | URL/域名匹配 |
| `crates/zeroclaw-config/src/schema/security.rs` | 1,125 | 配置 schema |
| `crates/zeroclaw-config/src/schema/security_runtime.rs` | 578 | 运行时安全配置 |
| **合计** | **5,646** | |

**裁剪原因：** Android 端安全由 ToolContext 的 3 个方法（`is_tool_allowed`/`is_path_allowed`/`build_shell_command`）简化覆盖。

**依赖：** 被 12+ 工具直接引用（file_edit、file_write、file_read、shell、web_fetch、http_request、git_operations、memory_store/recall/forget/export/purge、knowledge_tool、backup_tool）。

**重新实现要点：**
- clawseed 的 ToolContext 用 `ctx.get::<SecurityPolicy>()` 能力袋查询，SecurityPolicy 类型由 clawseed-security crate 定义
- SecurityHook 实现 Hook trait，注册后自动拦截所有工具调用
- SecurityContextProvider 实现 ContextProvider trait，让工具能通过 `ctx.get()` 查询策略
- domain_matcher 放入 clawseed-security crate，不需要独立

### 1.2 沙箱系统（Sandboxing）

**功能：** 在隔离环境中执行 shell 命令和文件操作，防止 agent 越权。

**zeroclaw 源码：**

| 文件 | 行数 | 说明 |
|------|------|------|
| `crates/zeroclaw-runtime/src/security/docker.rs` | 216 | Docker 沙箱 |
| `crates/zeroclaw-runtime/src/security/firejail.rs` | 195 | Firejail 沙箱（Linux） |
| `crates/zeroclaw-runtime/src/security/landlock.rs` | 260 | Landlock LSM（Linux 5.13+） |
| `crates/zeroclaw-runtime/src/security/bubblewrap.rs` | 205 | Bubblewrap 沙箱 |
| `crates/zeroclaw-runtime/src/security/seatbelt.rs` | 416 | macOS Seatbelt |
| `crates/zeroclaw-runtime/src/security/nevis.rs` | 587 | Nevis 沙箱 |
| `crates/zeroclaw-runtime/src/security/mod.rs` | 134 | 沙箱选择逻辑 |
| `crates/zeroclaw-runtime/src/security/traits.rs` | 118 | 沙箱 trait 定义 |
| **合计** | **2,131** | |

**裁剪原因：** Android 用 Linux namespace/seccomp 隔离，不需要桌面沙箱方案。

**重新实现要点：**
- Android 可用 `unshare(CLONE_NEWNS)` 做文件系统隔离
- seccomp-bpf 过滤危险 syscall
- 单一 Android sandbox 实现，不需要多后端 trait

### 1.3 WebAuthn 认证

**功能：** FIDO2/WebAuthn 无密码认证，用于设备配对和操作授权。

**zeroclaw 源码：** `crates/zeroclaw-runtime/src/security/webauthn.rs`（1,374 行）+ `crates/zeroclaw-gateway/src/api_webauthn.rs`（321 行）

**裁剪原因：** Android 端不需要设备间 WebAuthn 配对。

**重新实现要点：** 如需安全认证，Android 有原生 BiometricPrompt API，Rust 侧只需暴露 challenge/response 接口。

### 1.4 OTP 紧急停止

**功能：** 一次性密码紧急停止（e-stop），通过 OTP 码远程终止 agent。

**zeroclaw 源码：** `crates/zeroclaw-runtime/src/security/otp.rs`（318 行）+ `crates/zeroclaw-runtime/src/security/estop.rs`（422 行）

**裁剪原因：** Android 端通过 app 直接停止服务即可。

### 1.5 审计日志（Audit）

**功能：** 记录所有 agent 操作（工具调用、文件修改、shell 命令）的不可篡改审计日志。

**zeroclaw 源码：** `crates/zeroclaw-runtime/src/security/audit.rs`（1,278 行）

**裁剪原因：** 企业合规需求，Android 端暂不需要。

**重新实现要点：** 可简化为 observer 事件的持久化版本，不需要 HMAC 签名链。

### 1.6 其他安全模块

| 模块 | 行数 | 功能 | 裁剪原因 |
|------|------|------|---------|
| `security/leak_detector.rs` | 612 | API key/密码泄露检测 | 可在 provider 层做简单检查 |
| `security/iam_policy.rs` | 479 | IAM 权限策略 | 企业需求 |
| `security/prompt_guard.rs` | 361 | 提示注入检测 | 可作为 observer 插件重新实现 |
| `security/vulnerability.rs` | 397 | 漏洞扫描 | 企业需求 |
| `security/playbook.rs` | 459 | 安全事件响应手册 | 企业需求 |
| `security/detect.rs` | 288 | 威胁检测 | 企业需求 |
| `security/workspace_boundary.rs` | 211 | 工作区边界隔离 | 可通过 ToolContext.is_path_allowed() 实现 |

---

## 2. 技能系统（Skills + SkillForge）

### 2.1 技能管理（Skills）

**功能：** 定义、加载、执行可复用的 agent 技能（prompt 模板 + 工具组合 + 执行策略）。

**zeroclaw 源码：**

| 文件 | 行数 | 说明 |
|------|------|------|
| `crates/zeroclaw-runtime/src/skills/mod.rs` | 1,736 | 技能加载、注册、执行 |
| `crates/zeroclaw-runtime/src/skills/creator.rs` | 908 | 技能创建向导 |
| `crates/zeroclaw-runtime/src/skills/audit.rs` | 890 | 技能审计 |
| `crates/zeroclaw-runtime/src/skills/testing.rs` | 471 | 技能测试框架 |
| `crates/zeroclaw-runtime/src/skills/improver.rs` | 461 | 技能自动改进 |
| `crates/zeroclaw-runtime/src/skills/symlink_tests.rs` | 116 | 符号链接测试 |
| `crates/zeroclaw-runtime/src/tools/skill_tool.rs` | 323 | skill_tool 工具 |
| `crates/zeroclaw-runtime/src/tools/skill_http.rs` | 224 | HTTP 技能桥接 |
| `crates/zeroclaw-runtime/src/tools/read_skill.rs` | 187 | 技能阅读工具 |
| `crates/zeroclaw-config/src/schema/sop.rs` | 102 | 技能配置 |
| **合计** | **5,418** | |

**裁剪原因：** Android 端不需要技能创建/测试/改进工作流，可通过 prompt 直接指导 agent。

**依赖：** skills 依赖 prompt（SystemPromptBuilder 的 SkillsSection）、tools（skill_tool/skill_http）、config（技能目录配置）。

**重新实现要点：**
- 简化为"prompt 模板"系统：YAML 文件定义 name + system_prompt + tools
- 不需要 creator/improver/audit 工作流
- 可作为 clawseed-agent 的可选模块

### 2.2 SkillForge

**功能：** 技能发现、评估、集成——自动从外部源发现可用技能并集成到 agent。

**zeroclaw 源码：** `crates/zeroclaw-runtime/src/skillforge/`（1,118 行：scout 339 + evaluate 272 + integrate 252 + mod 255）

**裁剪原因：** 技能市场的概念，Android 端不需要自动发现。

---

## 3. SOP 引擎（Standard Operating Procedures）

**功能：** 定义和执行结构化操作流程——多步骤任务的条件分支、审批门、指标跟踪。

**zeroclaw 源码：**

| 文件 | 行数 | 说明 |
|------|------|------|
| `crates/zeroclaw-runtime/src/sop/engine.rs` | 2,091 | SOP 执行引擎 |
| `crates/zeroclaw-runtime/src/sop/metrics.rs` | 1,410 | 指标跟踪 |
| `crates/zeroclaw-runtime/src/sop/dispatch.rs` | 753 | 任务分发 |
| `crates/zeroclaw-runtime/src/sop/types.rs` | 639 | 类型定义 |
| `crates/zeroclaw-runtime/src/sop/condition.rs` | 451 | 条件评估 |
| `crates/zeroclaw-runtime/src/sop/mod.rs` | 378 | 模块导出 |
| `crates/zeroclaw-runtime/src/sop/audit.rs` | 254 | SOP 审计 |
| `crates/zeroclaw-runtime/src/tools/sop_execute.rs` | 265 | SOP 执行工具 |
| `crates/zeroclaw-runtime/src/tools/sop_status.rs` | 455 | SOP 状态查询 |
| `crates/zeroclaw-runtime/src/tools/sop_advance.rs` | 451 | SOP 推进工具 |
| `crates/zeroclaw-runtime/src/tools/sop_approve.rs` | 272 | SOP 审批工具 |
| `crates/zeroclaw-runtime/src/tools/sop_list.rs` | 224 | SOP 列表工具 |
| **合计** | **7,643** | |

**裁剪原因：** 企业流程自动化需求，Android 端不需要。

**依赖：** SOP 依赖 approval（审批门）、security（操作权限）、metrics（指标收集）。

**重新实现要点：**
- 可简化为"任务链"——有序的工具调用序列，无条件分支/审批门
- 或作为 agent 的 system prompt 指导多步骤任务
- 如果确实需要结构化流程，engine.rs 的状态机模式可复用

---

## 4. 多 Agent 委派（Delegate）

**功能：** agent-to-agent 任务委派——主 agent 将子任务分配给专门的子 agent，支持并行执行、上下文隔离、结果汇总。

**zeroclaw 源码：** `crates/zeroclaw-runtime/src/tools/delegate.rs`（2,952 行）

**裁剪原因：** Android 端单 agent 足够，多 agent 增加复杂度和资源消耗。

**依赖：** 依赖 Agent struct（创建子 agent 实例）、Provider（子 agent LLM 调用）、CostTracker（共享预算）、SecurityPolicy（子 agent 权限继承）。

**重新实现要点：**
- 最简实现：agent 内部循环，用 system prompt 切换"角色"而非创建新 Agent 实例
- 完整实现：DelegateTool 创建独立 Agent 实例，共享 provider 和 cost_tracker，隔离 history 和 memory namespace
- 关键设计决策：子 agent 的工具集是否与父 agent 相同？是否需要递归委派限制？

---

## 5. 信任系统（Trust）

**功能：** 信任评分——根据 agent 行为历史（正确性、安全性、用户反馈）计算信任等级，影响自主权。

**zeroclaw 源码：** `crates/zeroclaw-runtime/src/trust/`（812 行：types 190 + tests 616 + mod 6）

**裁剪原因：** 信任评分影响自主权级别，clawseed 简化为固定自主权。

**重新实现要点：** 可作为 CostTracker 的扩展——记录每次工具调用的成功/失败，计算滑动窗口信任分数。

---

## 6. 可验证意图（Verifiable Intent）

**功能：** 密码学验证用户意图——对敏感操作（删除文件、执行 shell）生成可验证的授权令牌。

**zeroclaw 源码：**

| 文件 | 行数 | 说明 |
|------|------|------|
| `crates/zeroclaw-runtime/src/verifiable_intent/crypto.rs` | 357 | 密码学原语 |
| `crates/zeroclaw-runtime/src/verifiable_intent/verification.rs` | 738 | 验证逻辑 |
| `crates/zeroclaw-runtime/src/verifiable_intent/issuance.rs` | 501 | 令牌签发 |
| `crates/zeroclaw-runtime/src/verifiable_intent/types.rs` | 374 | 类型定义 |
| `crates/zeroclaw-runtime/src/verifiable_intent/error.rs` | 113 | 错误类型 |
| `crates/zeroclaw-runtime/src/verifiable_intent/mod.rs` | 37 | 模块导出 |
| `crates/zeroclaw-runtime/src/tools/verifiable_intent.rs` | 254 | 工具实现 |
| **合计** | **2,374** | |

**裁剪原因：** 密码学意图验证是企业/合规需求，Android 端用 approval 确认即可。

**重新实现要点：** 如需轻量版本，可简化为 HMAC 签名的操作令牌（不需要完整的公钥体系）。

---

## 7. 隧道系统（Tunnel）

**功能：** 将本地 gateway 暴露到公网——支持 Cloudflare Tunnel、ngrok、Tailscale、OpenVPN 等多种隧道方案。

**zeroclaw 源码：**

| 文件 | 行数 | 说明 |
|------|------|------|
| `crates/zeroclaw-runtime/src/tunnel/mod.rs` | 493 | 隧道选择和管理 |
| `crates/zeroclaw-runtime/src/tunnel/cloudflare.rs` | 215 | Cloudflare Tunnel |
| `crates/zeroclaw-runtime/src/tunnel/ngrok.rs` | 151 | ngrok |
| `crates/zeroclaw-runtime/src/tunnel/tailscale.rs` | 133 | Tailscale |
| `crates/zeroclaw-runtime/src/tunnel/openvpn.rs` | 256 | OpenVPN |
| `crates/zeroclaw-runtime/src/tunnel/pinggy.rs` | 209 | Pinggy |
| `crates/zeroclaw-runtime/src/tunnel/custom.rs` | 217 | 自定义隧道 |
| `crates/zeroclaw-runtime/src/tunnel/none.rs` | 64 | 无隧道 |
| **合计** | **1,738** | |

**裁剪原因：** Android 端通过本地网络访问 gateway，不需要公网暴露。

**重新实现要点：** 如需远程访问，Android 可用 reverse proxy 或 Android 的 NetworkSecurityConfig，不需要 Rust 侧隧道管理。

---

## 8. 守护进程模式（Daemon）

**功能：** 将 agent 作为系统守护进程运行，支持后台常驻、自动重启、系统服务管理。

**zeroclaw 源码：** `crates/zeroclaw-runtime/src/daemon/mod.rs`（1,260 行）

**裁剪原因：** Android 端通过 app Service 管理生命周期，不需要 daemon 模式。

**重新实现要点：** Android 的 ForegroundService 替代，Rust 侧不需要实现。

---

## 9. 诊断系统（Doctor）

**功能：** 自诊断——检查依赖安装、网络连通、provider 可用性、配置完整性。

**zeroclaw 源码：** `crates/zeroclaw-runtime/src/doctor/mod.rs`（1,340 行）

**裁剪原因：** Android 端诊断由 app UI 负责。

**重新实现要点：** 可保留 `GET /api/health` 端点做基本健康检查，不需要完整的诊断套件。

---

## 10. 浏览器工具（Browser）

**功能：** 浏览器自动化——页面导航、内容提取、表单填写、截图。支持 headless 和有头模式。

**zeroclaw 源码：**

| 文件 | 行数 | 说明 |
|------|------|------|
| `crates/zeroclaw-tools/src/browser.rs` | 2,661 | 主浏览器工具 |
| `crates/zeroclaw-tools/src/browser_delegate.rs` | 723 | 委派浏览 |
| `crates/zeroclaw-tools/src/browser_open.rs` | 533 | 打开浏览器 |
| `crates/zeroclaw-tools/src/text_browser.rs` | ~400 | 纯文本浏览 |
| **合计** | **~4,317** | |

**裁剪原因：** Android 端用 web_fetch + WebView 替代，不需要 WebDriver 自动化。

**依赖：** 依赖 `fantoccini` crate（WebDriver 客户端）、`serde_json`。

**重新实现要点：**
- 轻量版：Android WebView 的 JS bridge（Kotlin 侧实现）
- 完整版：通过 remote tool bridge 让 Android 侧执行浏览器操作

---

## 11. 硬件工具（Hardware）

**功能：** 嵌入式硬件交互——Arduino 固件烧录、内存映射读写、传感器数据读取、引脚控制。

**zeroclaw 源码：**

| 文件 | 行数 | 说明 |
|------|------|------|
| `crates/zeroclaw-tools/src/hardware_board_info.rs` | 208 | 开发板信息 |
| `crates/zeroclaw-tools/src/hardware_memory_map.rs` | 208 | 内存映射 |
| `crates/zeroclaw-tools/src/hardware_memory_read.rs` | 183 | 内存读取 |
| `crates/zeroclaw-config/src/schema/hardware.rs` | 613 | 硬件配置 |
| `crates/zeroclaw-runtime/src/platform/wasm.rs` | 687 | WASM 运行时 |
| **合计** | **1,899** | |

**裁剪原因：** Android 端不是嵌入式场景。

**重新实现要点：** 如需 Android 传感器访问，通过 remote tool bridge 调用 Android Sensor API。

---

## 12. MCP 客户端（Model Context Protocol）

**功能：** MCP 协议客户端——连接外部 MCP server，动态发现和调用远程工具。

**zeroclaw 源码：**

| 文件 | 行数 | 说明 |
|------|------|------|
| `crates/zeroclaw-tools/src/mcp_transport.rs` | 1,283 | 传输层（stdio/SSE） |
| `crates/zeroclaw-tools/src/mcp_client.rs` | ~400 | 客户端注册 |
| `crates/zeroclaw-tools/src/mcp_tool.rs` | 230 | MCP 工具包装 |
| `crates/zeroclaw-tools/src/mcp_protocol.rs` | 231 | 协议定义 |
| `crates/zeroclaw-tools/src/mcp_deferred.rs` | 548 | 延迟加载 |
| **合计** | **~2,692** | |

**裁剪原因：** Android 端用 remote tool bridge 替代 MCP，无需额外协议层。

**重新实现要点：**
- remote tool bridge 已覆盖"Android 端调用本地能力"的场景
- 如需连接外部 MCP server（如桌面 IDE），可后续作为独立 crate 实现
- mcp_protocol.rs 的 JSON-RPC 定义可复用

---

## 13. 第三方集成工具

| 工具 | 行数 | 功能 | 重新实现方式 |
|------|------|------|------------|
| composio.rs | 1,942 | Composio API 集成 | 通过 HTTP 工具调用 |
| linkedin_client.rs | 1,726 | LinkedIn API | 通过 HTTP 工具调用 |
| jira_tool.rs | 1,524 | Jira API | 通过 HTTP 工具调用 |
| google_workspace.rs | 1,061 | Google Workspace API | 通过 HTTP 工具 + OAuth |
| microsoft365/ | 1,519 | Microsoft 365/Graph | 通过 HTTP 工具 + OAuth |
| notion_tool.rs | 443 | Notion API | 通过 HTTP 工具调用 |
| discord_search.rs | ~300 | Discord 搜索 | 通过 HTTP 工具调用 |
| pushover.rs | ~250 | Pushover 推送 | 通过 HTTP 工具调用 |
| **合计** | **~7,765** | | |

**裁剪原因：** 每个集成都是特定 API 的客户端封装，Android 端不需要内置。

**重新实现要点：**
- 通用方案：通过 http_request 工具 + LLM 理解 API 文档 来调用任何 API
- 专用方案：作为独立工具插件 crate，按需引入
- OAuth 流程需在 Android 侧实现（通过 remote tool bridge 触发 Android 的 OAuth activity）

---

## 14. CLI 包装工具

| 工具 | 行数 | 功能 |
|------|------|------|
| claude_code_runner.rs | 527 | Claude Code 执行 |
| codex_cli.rs | ~400 | Codex CLI |
| gemini_cli.rs | ~300 | Gemini CLI |
| opencode_cli.rs | ~250 | OpenCode CLI |
| cli_discovery.rs | ~300 | CLI 自动发现 |
| **合计** | **~1,777** | |

**裁剪原因：** Android 端不运行外部 CLI 工具。

**重新实现要点：** 如需 agent-to-agent 协作，通过 delegate 或 provider API 实现。

---

## 15. 其他裁剪工具

| 工具 | 行数 | 功能 | 重新实现方式 |
|------|------|------|------------|
| weather_tool.rs | 873 | 天气查询 | 通过 web_search 或 http_request |
| cloud_ops.rs | 936 | 云操作 | 企业需求 |
| cloud_patterns.rs | ~400 | 云架构模式 | 企业需求 |
| swarm.rs | 967 | Agent 集群 | 简化为 delegate |
| pipeline.rs | 617 | 管道执行 | 简化为 cron |
| escalate.rs | 637 | 升级处理 | 通过 approval 实现 |
| image_gen.rs | 509 | 图片生成 | 通过 provider multimodal |
| image_info.rs | 494 | 图片分析 | 通过 provider multimodal |
| screenshot.rs | ~400 | 截图 | 通过 remote tool bridge |
| report_templates.rs | 602 | 报告模板 | 非 Android 需求 |
| report_template_tool.rs | ~300 | 报告生成 | 非 Android 需求 |
| project_intel.rs | 750 | 项目分析 | 非 Android 需求 |
| ask_user.rs | 503 | 用户交互 | 通过 remote tool bridge |
| sessions.rs | 1,116 | 会话管理 | gateway session 已覆盖 |
| tool_search.rs | 368 | 工具发现 | Android 工具集固定 |
| poll.rs | 473 | 轮询 | 通过 cron 实现 |
| reaction.rs | 545 | 反应工具 | 非 Android 需求 |
| workspace_tool.rs | ~300 | 工作区操作 | 非 Android 需求 |
| proxy_config.rs | 553 | 代理配置 | 简化为环境变量 |
| data_management.rs | 320 | 数据管理 | 非 Android 需求 |
| node_capabilities.rs | 266 | 节点能力 | 非 Android 需求 |
| **合计** | **~10,619** | | |

---

## 16. 裁剪的 Provider

| Provider | 行数 | 功能 | 重新实现方式 |
|----------|------|------|------------|
| openai.rs | ~800 | OpenAI 原生 API | 走 compatible/，配置 base_url + api_key |
| azure_openai.rs | ~600 | Azure OpenAI | 走 compatible/，配置 azure endpoint |
| ollama.rs | ~500 | Ollama 本地 | 走 compatible/，配置 localhost:11434 |
| openrouter.rs | ~500 | OpenRouter | 走 compatible/，配置 openrouter.ai |
| openai_codex.rs | ~400 | Codex API | 走 compatible/ |
| telnyx.rs | ~300 | Telnyx 语音 | 非 Android 需求 |
| glm.rs | ~400 | 智谱 GLM | 走 compatible/ |
| kilocli.rs | ~200 | Kilocli | 走 compatible/ |
| gemini_cli.rs | ~300 | Gemini CLI OAuth | 非 Android 需求 |
| claude_code.rs | ~300 | Claude Code provider | 非 Android 需求 |
| copilot.rs | ~400 | GitHub Copilot | 非 Android 需求 |
| models_dev.rs | ~200 | Models.dev 路由 | 走 compatible/ |
| **合计** | **~4,900** | | |

**重新实现要点：** 大部分 provider 只需在 compatible/ 的配置中注册别名和 base_url。差异化逻辑（如 OpenAI 的 reasoning_effort、Azure 的认证头）可在 compatible/ 的 provider_config 中用可选字段支持。

---

## 17. 裁剪的 Memory 后端

| 后端 | 行数 | 功能 | 重新实现方式 |
|------|------|------|------------|
| postgres.rs | 509 | PostgreSQL | Android 不需要 |
| qdrant.rs | 669 | Qdrant 向量库 | Android 不需要 |
| lucid.rs | 724 | 高性能混合后端 | Android 不需要 |
| markdown.rs | 399 | 人可读 Markdown | Android 不需要 |
| knowledge_graph.rs | 863 | 知识图谱 | 后续可作为独立 crate |
| knowledge_graph_pg.rs | 318 | PG 知识图谱 | 后续可复用 |
| consolidation.rs | 239 | 记忆合并 | 可在 SQLite 后端内实现 |
| conflict.rs | 174 | 冲突检测 | 多 agent 场景需要 |
| hygiene.rs | 586 | 记忆清理 | 可简化为定期清理 |
| audit.rs | 293 | 记忆审计 | 企业需求 |
| snapshot.rs | 470 | 记忆快照 | 备份功能可复用 |
| response_cache.rs | 526 | 响应缓存 | 可在 provider 层实现 |
| policy.rs | 198 | 记忆策略 | 企业需求 |
| **合计** | **5,868** | | |

---

## 18. 裁剪的 Gateway 模块

| 模块 | 行数 | 功能 | 重新实现方式 |
|------|------|------|------------|
| api_pairing.rs | 384 | 设备配对 API | Android 本地不需要配对 |
| api_webauthn.rs | 321 | WebAuthn API | 不需要 |
| node_tool.rs | 303 | 节点间工具调用 | 不需要多节点 |
| nodes.rs | 619 | 多节点管理 | 不需要 |
| canvas.rs | 291 | Canvas 协作 | 不需要 |
| sse.rs | 211 | Server-Sent Events | WebSocket 已覆盖 |
| voice_duplex.rs | 190 | 语音双向通信 | 后续可作为独立模块 |
| static_files.rs | 151 | 静态文件服务 | Android 不需要 Web UI |
| **合计** | **2,470** | | |

**voice_duplex 重新实现要点：** 如需语音交互，Android 有原生 SpeechRecognizer + TextToSpeech API，Rust 侧只需在 gateway 暴露音频 WebSocket 端点。

---

## 19. 裁剪的 Runtime 子系统

| 子系统 | 行数 | 功能 | 重新实现方式 |
|--------|------|------|------------|
| identity.rs | 1,488 | 身份管理（多身份、签名、验证） | Android 用系统账户 |
| onboard/mod.rs | 1,790 | 新用户引导 | Android app UI 负责 |
| service/mod.rs | 1,693 | 服务管理 | Android Service 替代 |
| migration.rs | 656 | 配置迁移 | clawseed 自带最新 schema |
| i18n.rs | 168 | 国际化 | Android 资源系统 |
| cli_input.rs | 152 | CLI 交互输入 | Android 不需要 |
| integrations/ | 1,366 | 集成注册表 | 按需引入 |
| rag/ | 393 | RAG 检索增强 | 可在 memory 中实现 |
| nodes/ | 238 | 节点传输 | 不需要多节点 |
| routines/ | 660 | 例程调度 | cron 已覆盖 |
| **合计** | **8,504** | | |

---

## 20. 裁剪的 Config 模块

| 模块 | 行数 | 功能 |
|------|------|------|
| secrets.rs | 905 | ChaCha20 加密密钥存储 |
| policy/ | 2,842 | SecurityPolicy + 执行 + 测试 |
| autonomy.rs | 16 | 自主权级别 |
| pairing.rs | 753 | 设备配对 |
| domain_matcher.rs | 259 | 域名匹配 |
| schema/channels.rs | 1,528 | Channel 配置 |
| schema/hardware.rs | 613 | 硬件配置 |
| schema/security.rs | 1,125 | 安全配置 |
| schema/security_runtime.rs | 578 | 安全运行时配置 |
| schema/sop.rs | 102 | SOP 配置 |
| schema/tunnels.rs | 171 | 隧道配置 |
| schema/multimedia.rs | 2,073 | 多媒体配置 |
| schema/enterprise.rs | 64 | 企业配置 |
| schema/tests.rs | 6,531 | 测试 |
| scattered_types.rs | 558 | 零散类型 |
| **合计** | **~18,018** | |

---

## 21. 裁剪的 CLI 命令

| 命令 | 说明 | 重新实现方式 |
|------|------|------------|
| Agent | 交互式 agent 会话 | Android 不需要 CLI |
| Acp | ACP 协议 | 不需要 |
| Daemon | 守护进程模式 | Android Service |
| Service | 服务管理 | Android Service |
| Doctor | 诊断 | /api/health |
| Estop | 紧急停止 | Android app 按钮 |
| Models | 模型列表 | /api/config |
| Channel | 渠道管理 | 不需要 |
| Integrations | 集成管理 | 不需要 |
| Skills | 技能管理 | 不需要 |
| Sop | SOP 管理 | 不需要 |
| Migrate | 迁移 | 不需要 |
| Auth | 认证管理 | Android app |
| Hardware | 硬件管理 | 不需要 |
| Peripheral | 外设管理 | 不需要 |
| Memory | 记忆管理 | /api/memory (后续) |
| Config | 配置管理 | /api/config |
| Update | 自更新 | Android app store |
| SelfTest | 自测试 | 不需要 |
| Desktop | 桌面 app | 不需要 |
| Completions | Shell 补全 | 不需要 |
| MarkdownHelp/Schema | 文档生成 | 不需要 |

---

## 22. 裁剪的 Infra 模块

| 模块 | 行数 | 处理方式 |
|------|------|---------|
| debounce.rs | ~100 | 已删除，不需要 |
| stall_watchdog.rs | ~150 | 已删除，不需要 |

注：session_backend.rs、session_sqlite.rs、session_store.rs 已迁入 clawseed-gateway。

---

## 附录：跨子系统依赖图

以下依赖关系在重新实现时需要注意——被依赖方必须先实现：

```
delegate ──→ Agent, Provider, CostTracker, SecurityPolicy, Memory
skills   ──→ prompt (SkillsSection), tools (skill_tool/skill_http), config
sop      ──→ approval (审批门), security (权限), metrics (指标)
trust    ──→ tools 执行结果 (成功/失败记录)
security ──→ config (SecurityPolicy), tools (12+ 工具引用)
mcp      ──→ tools (Tool trait), agent (activated_tools)
tunnel   ──→ gateway (端口暴露)
browser  ──→ fantoccini (WebDriver)
hardware ──→ probe-rs (固件烧录), config (HardwareConfig)
```

**实现优先级建议：**

1. **SecurityPolicy** — 最高优先级，12+ 工具依赖它。实现为 `clawseed-security` 扩展 crate，通过 Hook + ContextProvider 注册
2. **Delegate** — 多 agent 是核心架构能力。实现为 `clawseed-delegate` 扩展 crate，通过 Tool + ContextProvider 注册
3. **Skills（简化版）** — prompt 模板系统。实现为 `clawseed-skills` 扩展 crate，通过 Hook + ContextProvider + skill tools 注册
4. **MCP** — 外部工具扩展协议。实现为 `clawseed-mcp` 扩展 crate，通过 Tool + ContextProvider 注册
5. 其余按业务需求排序

**扩展 crate 的标准结构：**

```
clawseed-<name>/
├── Cargo.toml          依赖 clawseed-api, clawseed-agent (Hook/Tool/ContextProvider trait)
├── src/
│   ├── lib.rs          导出 + 注册函数
│   ├── hook.rs         impl Hook (如果需要拦截工具调用)
│   ├── context.rs      impl ContextProvider (如果需要暴露能力给工具)
│   ├── tools.rs        impl Tool (如果提供工具)
│   └── config.rs       本扩展的配置 struct (不放 clawseed-config)
```

**binary 注册示例：**
```rust
#[cfg(feature = "security")]
builder = builder
    .hook(security::SecurityHook::new(config))
    .context_provider(security::SecurityContextProvider::new(config));
```
