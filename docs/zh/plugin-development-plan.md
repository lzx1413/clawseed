---
title: CETP Linux 插件开发方案
description: 基于 CETP v2 和 Linux runtime 为 ClawSeed 增加本地可执行插件。
---

# CETP Linux 插件开发方案

## 1. 目标

Linux runtime 当前提供 Node.js 和 Python 等程序的运行环境。本方案在此基础上增加本地插件
能力，让第三方开发者可以把一个独立进程暴露为 ClawSeed Agent 的工具。

插件协议复用 ClawSeed External Tool Protocol（CETP）的工具语义和 JSON 数据模型。Android
第三方 App 继续使用 ContentProvider 绑定，Linux 插件使用 stdin/stdout 或 Unix socket 绑定。
两种 Provider 对 Agent 都表现为相同的 `Tool`。

这体现了 ClawSeed 的动静结合模型：Rust Agent、记忆、分身和安全策略是静态可信内核；CETP
是动态扩展的统一语义层，具体传输绑定可以是 Linux 插件的 stdio、Android 外部 App 的
ContentProvider，或其他客户端的 WebSocket。当前首期实现为 stdin/stdout JSONL；Unix socket
仍是后续可选绑定。

首期交付目标如下：

- 支持 Python 和 Node.js 插件；
- 支持插件发现、清单校验、启动、停止、崩溃检测和有限重启；
- 支持 `negotiate`、`get_provider_info`、`list_tools`、`execute_tool`；
- 将插件工具注册到 Agent 的 Tool Registry；
- 对 workspace、环境变量、网络、超时、并发和输出大小实施宿主侧限制；
- 不在普通工具调用中隐式安装或升级插件；
- 插件失败时返回结构化错误，不影响内置工具和其他插件。

首期不实现插件市场、任意远程下载、跨插件事务和内核级安全隔离。

## 2. 协议复用边界

### 2.1 CETP Core

以下内容直接复用 CETP v2：

- Provider 身份和稳定的 `provider_id`；
- 工具名称、描述和 JSON Schema；
- `negotiate`、`get_provider_info`、`list_tools`、`execute_tool`；
- `request_id`、deadline 和结构化错误；
- 工具的 effect、risk、destructive、idempotent、scope 等注解；
- `async_operations`、`cancellation`、`large_results` 能力的协商模型；
- `revision` 和工具集合缓存规则。

CETP v1 只允许只读工具。只读 Linux 插件可以实现 v1 的兼容子集，包含副作用、写入或长任务
的插件必须实现 v2。新插件默认实现 v2。

### 2.2 Transport Binding

CETP v2 当前的线传输是 Android `ContentResolver.call()`。Linux 插件需要定义独立的
transport binding，但请求字段、响应 envelope、错误代码和生命周期外的调用语义保持一致。

| 场景 | Provider | Consumer | 绑定 |
| --- | --- | --- | --- |
| Android 外部工具 | 第三方 Android App | `ExternalToolBridge` | ContentProvider |
| Linux 本地插件 | 插件进程 | `PluginManager` | stdin/stdout JSONL |
| Linux 长连接插件 | 插件进程 | `PluginManager` | Unix socket，可选 |
| Gateway 远程工具 | 远程客户端 | Gateway | WebSocket |

首期只实现 stdin/stdout JSONL。Unix socket 作为后续优化，用于需要长期运行、较大吞吐或多路
并发的插件。

### 2.3 不由 CETP 负责的内容

CETP 不负责以下宿主行为：

- 插件包安装、升级、卸载和回滚；
- 插件进程的启动、停止、健康检查和崩溃重启；
- PRoot rootfs、workspace 映射和 runtime generation 选择；
- CPU、内存、进程数和文件系统配额；
- 本地插件的信任来源和用户确认；
- 插件日志和诊断文件的持久化。

这些内容由 Rust `PluginManager` 和 Linux runtime 执行器负责。

## 3. 总体架构

```text
┌──────────────────────────────────────────────────────────────┐
│ Agent                                                         │
│  Tool Registry                                                │
│    ├─ BuiltInTool                                             │
│    ├─ RemoteTool                                              │
│    └─ PluginTool                                              │
└───────────────────────────────┬──────────────────────────────┘
                                │ Tool::execute
                                ▼
┌──────────────────────────────────────────────────────────────┐
│ PluginManager                                                 │
│  scan -> validate -> start -> negotiate -> list_tools         │
│  call -> timeout/cancel -> result -> health/restart            │
└───────────────────────────────┬──────────────────────────────┘
                                │ LinuxRuntime::execute
                                ▼
┌──────────────────────────────────────────────────────────────┐
│ PRoot Linux runtime                                           │
│  workspace / task tmp / declared environment                  │
│  Python / Node.js plugin process                              │
└──────────────────────────────────────────────────────────────┘
```

### 3.1 组件职责

| 组件 | 职责 |
| --- | --- |
| `PluginManager` | 发现、校验、加载和管理插件实例 |
| `PluginProcess` | 管理单个插件进程、stdin/stdout、请求表和退出状态 |
| `PluginTool` | 将 CETP 工具包装为 `clawseed_api::Tool` |
| `LinuxRuntime` | 生成 PRoot 请求、绑定路径、设置环境并回收进程 |
| `ToolRegistry` | 注册和过滤插件工具 |
| `RuntimeManager` | Android 侧安装和切换 Linux runtime |
| `Agent` | 按现有工具调用流程执行 `PluginTool` |

`PluginTool` 不直接拼接 PRoot 参数，也不直接管理插件进程。它只持有插件 ID、工具规格和
`PluginManager` 的调用句柄。

### 3.2 工具来源

扩展 `ToolSource`：

```rust
pub enum ToolSource {
    BuiltIn,
    Mcp { server: String },
    Remote { session: String },
    Plugin { plugin_id: String },
}
```

插件工具名使用稳定命名空间，建议格式为 `<plugin_id>__<tool_name>`。插件内部的
`tool_name` 仍然遵循 CETP v2 的名称约束。工具注册时必须拒绝与内置工具或其他插件冲突的
完整名称。

## 4. 插件包和清单

### 4.1 目录布局

首期支持 workspace 级和用户级插件目录：

```text
<workspace>/.clawseed/plugins/<plugin-id>/
  manifest.json
  bin/plugin-entry
  assets/
  README.md

~/.clawseed/plugins/<plugin-id>/
  manifest.json
  bin/plugin-entry
```

Android 应用内使用应用私有目录映射相同的结构。插件不得从 `/sdcard` 或其他任意外部路径
自动加载。

发现优先级为 workspace 级、用户级、系统级。相同 `plugin_id` 只保留最高优先级版本，
并记录被遮蔽的插件以便诊断。

### 4.2 Manifest v1

```json
{
  "schema_version": 1,
  "plugin_id": "example.weather",
  "name": "Example Weather",
  "version": "0.1.0",
  "description": "查询天气和空气质量",
  "entry": "bin/plugin-entry",
  "protocol": {
    "versions": [2],
    "transport": "stdio"
  },
  "runtime": {
    "language": "python",
    "id": "any",
    "components": ["python"]
  },
  "permissions": {
    "workspace": "read",
    "network": "allow",
    "environment": []
  },
  "limits": {
    "startup_timeout_ms": 5000,
    "request_timeout_ms": 30000,
    "max_concurrent_requests": 2,
    "max_output_bytes": 1048576
  },
  "publisher": {
    "kind": "local",
    "id": "developer"
  }
}
```

必填字段为 `schema_version`、`plugin_id`、`name`、`version`、`entry` 和 `protocol`。
`plugin_id` 必须稳定，并匹配 `[a-z][a-z0-9_.-]{0,63}`。入口必须是插件目录内的相对路径，
不能经过符号链接逃逸到插件目录外。

首期 `publisher.kind` 只允许 `local`。签名插件包使用新的 manifest schema 或新增签名字段，
不得让未验证的本地字段被误认为发布者身份。

### 4.3 权限声明

权限声明用于用户展示、配置过滤和启动时检查。它不是唯一安全边界，最终限制由执行器强制实施。

首期权限：

| 权限 | 含义 |
| --- | --- |
| `workspace=none` | 不映射 workspace |
| `workspace=read` | 将 workspace 复制到临时快照并以 `/workspace` 提供，插件修改不会写回真实 workspace |
| `workspace=write` | 允许写入 workspace |
| `network=deny` | 不提供网络能力声明 |
| `network=allow` | 使用宿主网络 |
| `environment` | 允许传入的环境变量名称白名单 |

实际网络隔离取决于宿主平台。Android PRoot 默认使用宿主网络时，`network=deny` 只能阻止
插件获得声明的网络配置，不能宣称已经完成内核级网络隔离。

`plugin_create` 默认使用 `workspace=none` 和 `network=deny`。`workspace=read` 的临时快照会在
插件进程结束后删除，适合需要读取项目文件但不应修改项目的插件。`workspace=write` 直接挂载
真实 workspace，创建、修改和删除都会保留。

## 5. CETP stdio 绑定

### 5.1 启动命令

插件进程由 Linux runtime 启动，工作目录固定为插件目录或任务临时目录。入口不通过 shell
拼接执行，参数采用结构化 argv：

```text
<runtime> launch
  --plugin-root <private-plugin-dir>
  --workspace <workspace>
  --entry <entry-relative-path>
```

实际命令由 `PluginManager` 根据 manifest 的 entry 和 runtime component 生成。插件进程
只能继承白名单环境变量，并额外获得：

```text
CLAWSEED_PLUGIN_ID
CLAWSEED_CETP_VERSION=2
CLAWSEED_REQUEST_TIMEOUT_MS
```

### 5.2 消息格式

stdin 和 stdout 均使用 UTF-8，每行一个完整 JSON 对象。插件不得向 stdout 写日志，日志必须
写入 stderr。单行大小受 manifest 和宿主上限共同限制。

请求示例：

```json
{"id":"req-1","method":"list_tools","params":{"protocol_version":2,"session_token":"..."}}
```

成功响应示例：

```json
{"id":"req-1","status":"success","protocol_version":2,"data":{"revision":"tools-1","tools":[]}}
```

错误响应示例：

```json
{"id":"req-1","status":"error","protocol_version":2,"error":{"code":"INVALID_ARGS","message":"invalid input"}}
```

传输层只负责关联 `id` 和限制消息大小。请求字段、CETP 错误代码、工具 schema 和结果 envelope
遵循 CETP v2。首期不支持 stdout 之外的二进制结果，`large_results` 能力在 stdio 绑定中不得
宣称已协商。

### 5.3 建立会话

启动后，`PluginManager` 按以下顺序建立会话：

1. 读取 manifest 并检查 runtime、权限和入口；
2. 启动插件进程并等待首个可读响应；
3. 发送 `negotiate`，选择双方共同支持的最高版本；
4. 发送 `get_provider_info`，校验 `provider_id` 与 manifest 的 `plugin_id` 一致；
5. 发送 `list_tools`，校验工具名、schema 和注解；
6. 为每个工具创建 `PluginTool` 并注册到 Agent registry；
7. 将插件状态更新为 `ready`。

任何一步失败都不会注册部分工具。进程 stderr、退出码和失败阶段写入插件诊断记录。

### 5.4 工具调用

`PluginTool::execute()` 将调用参数包装成 CETP `execute_tool` 请求，并绑定：

- Agent 回合的 deadline；
- 工具自身的 request ID；
- manifest 声明的超时和宿主更严格的超时；
- 当前 runtime generation；
- 当前插件实例 ID。

调用完成后只接受与请求 ID 匹配的响应。超时会先请求插件取消，再关闭进程输入并回收整个
进程组。插件返回的数据必须通过输出大小、JSON 解析和 CETP schema 校验后才能转换为
`ToolResult`。

插件的 `execute_tool` 结果可以包含 `data.output` 和可选的 `data.presentation`。

`presentation` 使用现有 `ToolPresentation` 格式，`blocks` 支持 `image`、`audio`、`video` 和
`link`。媒体 URL 必须使用 HTTPS，宿主会在交给 Android 客户端前执行完整校验。没有
`presentation` 的旧插件结果继续按原始 JSON 值处理。

插件生成本地媒体时可以在媒体对象中返回相对 workspace 根目录的 `path` 和 MIME 类型。
Android 客户端会直接读取这个路径并展示媒体。插件无需生成 ID，也不能返回宿主绝对路径。

当前用户回合的图片会通过 `execute_tool.params.attachments` 和
`execute_tool.params.context.attachments` 传给插件。每项包含
Linux runtime 内可访问的 `path`，以及 `id`、`mime_type`、`size_bytes`、`width` 和 `height`。
该路径只在本次工具调用期间有效，宿主会在调用结束后清理临时文件。

## 6. 生命周期

```text
discovered -> validated -> starting -> negotiating -> ready
                                      ├──────────────> unhealthy
                                      └──────────────> failed
ready -> stopping -> stopped
unhealthy -> restarting -> negotiating
```

状态含义如下：

| 状态 | 说明 |
| --- | --- |
| `discovered` | 找到插件目录和 manifest |
| `validated` | 清单、入口、runtime 和权限通过检查 |
| `starting` | 正在启动进程 |
| `negotiating` | 正在建立 CETP 会话和加载工具 |
| `ready` | 工具已注册，可被 Agent 调用 |
| `unhealthy` | 进程存在但协议调用失败 |
| `restarting` | 按策略重启进程 |
| `failed` | 启动或重启失败 |
| `stopping` | 正在取消请求并回收进程 |
| `stopped` | 已停止且工具已注销 |

默认重启策略为短时间内最多三次。连续失败后进入 `failed`，等待设置页或管理 API 显式重试。
插件重启会重新 negotiate 和 list_tools，不能复用旧 session token。

runtime generation 更新时，正在执行的插件继续使用旧 generation。新启动的插件使用新 generation。
旧 generation 在没有插件和命令引用后才允许清理。

## 7. 配置

建议增加以下配置：

```toml
[plugins]
enabled = true
workspace_dir = ".clawseed/plugins"
user_dir = "~/.clawseed/plugins"
auto_start = true
max_loaded = 16
restart_limit = 3
allowed = ["example.weather"]
denied = []
```

插件的 manifest 权限还需要经过全局策略过滤。全局 `denied` 优先于 `allowed`。未列入
`allowed` 的插件在配置启用 allowlist 时不加载。

工具级过滤继续使用现有 `allowed_tools` 和 `denied_tools`。插件工具的完整名称进入过滤器，
例如 `example.weather__get_current`。

## 8. API 和注册表集成

Gateway 增加插件状态查询接口：

```text
GET  /api/plugins
PATCH /api/plugins/config
```

当前实现返回 manifest、校验状态、语言、路径和加载策略，并通过专用 PATCH 接口更新
`[plugins]` 配置。配置变更返回 `reload_required=true`，重启 Gateway 后重建插件工具注册表。
单插件 start、stop、restart 需要 PluginManager 持久化到共享状态后再开放，暂列为后续阶段。

首期接口只管理本地已安装插件，不提供远程下载和任意路径安装。`/api/tools` 继续返回插件
工具的 `ToolSpec`，并在来源字段中标记 `plugin_id`。

网关启动时，PluginManager 先加载已启用插件，再构造 Agent。插件工具必须同时出现在共享
注册表和实际 Agent 注册表中，遵循远程工具已有的双注册表约束。插件停止或断开时，按
`ToolSource::Plugin` 批量注销。

## 9. 安全策略

PRoot 是用户态兼容层，不是内核级沙箱。插件模型必须把风险边界写入用户界面和错误信息。

执行器至少实施以下限制：

- 插件入口只能位于受信任的私有插件目录；
- workspace 映射遵循 manifest 和全局策略，默认不映射任意宿主路径；
- 环境变量使用白名单，禁止覆盖 PRoot、动态链接器和执行器控制变量；
- 每个插件限制并发调用数；
- 每个请求限制 deadline、stdout、stderr 和 JSON 行大小；
- 超时、取消、进程退出时回收整个进程组；
- 插件日志与工具结果分离；
- 插件返回的文本、schema 和错误均视为不可信数据，不自动提升为 Agent 指令；
- 高风险或副作用工具必须按 CETP v2 的确认和 scope 规则处理；
- 未验证签名的插件只允许用户主动安装，并明确显示来源和权限。

## 10. 内置插件创作 Skill

ClawSeed 内置 `plugin-authoring` Skill，帮助用户从对话创建 Python 或 Node.js 插件。
Skill 只编排创建和验证流程，实际文件写入由专用工具完成：

- `plugin_create` 生成 `manifest.json`、语言入口和 README；
- `plugin_validate` 启动入口并执行 CETP 握手、Provider 身份和 `list_tools` 校验；
- 用户确认后再启用 `[plugins].enabled` 或扩大 workspace、network 权限。

创建插件与启用插件分开处理。生成的脚手架默认使用 `workspace=none` 和 `network=deny`，
入口程序的 stdout 保留给 CETP JSONL 响应，日志写入 stderr。

## 11. 分阶段开发计划

### Phase 0 协议和数据结构

- 在 `clawseed-api` 增加 CETP transport-neutral 的请求、响应、错误和工具注解类型；
- 增加 `ToolSource::Plugin`；
- 定义 manifest v1、状态枚举和配置 schema；
- 编写协议文档中的 JSON fixture 和兼容规则。

验收条件是 Rust 能解析、校验并 round-trip manifest 和 CETP 消息，且不会改变 Android CETP
v1/v2 的现有行为。

### Phase 1 本地插件进程

- 在 `clawseed-tools` 或独立 crate 实现 `PluginProcess`；
- 实现 stdin/stdout JSONL 读写、请求 ID 路由、超时和输出限制；
- 实现 Python 和 Node.js 两个最小示例插件；
- 覆盖启动失败、协议错误、stdout 日志污染和进程退出测试。

验收条件是示例插件可以独立返回 `list_tools` 和 `execute_tool` 结果。

### Phase 2 Agent Tool 集成

- 实现 `PluginManager`；
- 扫描 workspace 和用户插件目录；
- 完成 negotiate、provider identity、tool schema 和工具注册；
- 将插件工具接入现有 Hook、权限过滤和 Tool Registry；
- 实现停止、注销、崩溃重启和失败状态。

验收条件是 Agent 可以像调用内置工具一样调用插件工具，插件异常不会污染其他工具。

### Phase 3 Linux runtime 集成

- 使用已激活的 runtime generation 启动插件；
- 实现 Node.js 和 Python 组件检查；
- 绑定 workspace、任务临时目录和环境变量白名单；
- 验证 runtime 更新期间旧插件和新插件的 generation 隔离。

验收条件是 Android 真机可以安装 runtime、启动两个示例插件并完成一次工具调用。

### Phase 4 管理界面和 API

- 增加 `/api/plugins` 查询和 `[plugins]` 专用配置接口；
- 在 Android 设置页展示插件状态、版本、权限和失败原因；
- 增加用户启用、禁用操作，重启操作在共享 PluginManager 完成后接入；
- 为插件工具补充来源和运行时状态展示。

验收条件是用户能够发现、启用、停止和恢复本地插件，不需要编辑配置文件。

### Phase 5 签名包和升级

- 定义插件包 manifest、文件哈希和签名覆盖范围；
- 实现 staging、校验、原子切换和失败回滚；
- 支持插件版本与 runtime component 依赖检查；
- 为插件安装、升级和卸载增加审计日志。

验收条件是损坏包、签名错误、空间不足和升级中断都不会覆盖当前可用插件。

## 12. 测试计划

### Rust 单元测试

- manifest 字段、入口路径和版本校验；
- 工具名称命名空间和冲突检测；
- CETP request/response 编解码；
- deadline、输出大小和并发限制；
- 状态机和重启次数；
- runtime generation 绑定；
- 工具注销和注册表来源查询。

### 协议测试

- Python 和 Node.js provider fixture 共用同一组 CETP fixture；
- `list_tools` schema 变化更新 revision；
- 无效 JSON、未知 method、错误 request ID 和过期 session token；
- `AUTH_REQUIRED`、`PERMISSION_DENIED`、`INVALID_ARGS` 和 `INTERNAL_ERROR`；
- 插件 stdout 混入日志时立即失败并记录原因；
- 同一 request ID 的超时重试不会重复产生副作用。

### Android 真机测试

- runtime 未安装时插件返回可操作错误；
- runtime 安装后自动加载插件；
- App 重启后恢复插件状态；
- 插件进程崩溃和超时后的进程组清理；
- workspace 写入和 runtime 更新期间的 generation 隔离；
- Android CETP Provider 与 Linux CETP Provider 同时注册时的命名空间隔离。

## 13. 最小插件示例

插件只需要提供一个 manifest 和一个 CETP Provider 入口。入口实现可以使用任何已经存在于
Linux runtime 的语言。

```text
example.echo/
  manifest.json
  bin/plugin-entry
```

一个最小请求序列为：

```text
PluginManager -> negotiate
PluginManager <- selected_version=2
PluginManager -> get_provider_info
PluginManager <- provider_id=example.echo
PluginManager -> list_tools
PluginManager <- echo tool schema
PluginManager -> execute_tool(args={"text":"hello"})
PluginManager <- {"text":"hello"}
```

开发者只需要遵守 CETP Core。Python 和 Node.js 插件使用相同的 JSONL 传输格式，ClawSeed
不需要为两种语言增加单独的适配器。后续增加其他语言时沿用同一 transport binding。

## 14. 完成标准

插件功能达到以下条件后才能标记为首期完成：

- CETP v1/v2 Android 外部工具回归测试继续通过；
- Python 和 Node.js 示例插件均能在 Linux runtime 中启动；
- Agent 能发现并调用插件工具；
- 工具来源、权限和 runtime 状态可查询；
- 超时、取消、崩溃、输出超限和 JSON 错误均能收敛为结构化 ToolResult；
- 插件停止后工具不会残留在共享注册表或 Agent 注册表；
- runtime 更新、App 重启和插件失败不会破坏已有 workspace；
- 文档中的示例命令和测试命令在 CI 或 Android 真机上有对应验证记录。

相关协议文档：

- [CETP v1](external-tool-protocol.md)
- [CETP v2](external-tool-protocol-v2.md)
- [Android 离线 Linux runtime](android-linux-runtime.md)
- [Android PRoot Linux 执行环境方案](android-proot-runtime-plan.md)
