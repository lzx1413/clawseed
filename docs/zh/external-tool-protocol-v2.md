---
description: CETP v2 实验规范，定义 Android 跨应用工具的能力协商、安全授权、可控副作用、异步执行与兼容迁移。
---

# ClawSeed External Tool Protocol (CETP) v2 实验规范

> 状态：Experimental。ClawSeed Android SDK 已实现 v2 Consumer 和 Provider 基类，但线协议在
> 稳定发布前仍可能调整。生产集成应同时保留 [CETP v1](external-tool-protocol.md) 只读兼容层。

CETP v2 在保留 Android `ContentProvider` 低接入成本的前提下，将 CETP 从“只读数据桥”扩展为
可安全承载副作用、长任务和大结果的跨应用工具协议。v2 Consumer 必须继续支持 v1；Provider
可以只实现 v1、只实现 v2，或者同时实现两者。

本文中的“必须”“不得”“应该”和“可以”分别表示规范性要求、禁止要求、推荐行为和可选行为。

## 1. 设计目标

- 为工具声明副作用、风险、幂等性、授权 scope 和确认策略。
- 让 Provider 成为授权、用户确认和写操作去重的最终执行者。
- 支持显式版本与能力协商，而不是仅比较单个整数。
- 支持超时、取消、异步操作和超出 Binder 内联容量的大结果。
- 保持 v1 Provider 和 Consumer 的行为稳定，并提供双栈迁移路径。
- 保持工具输入输出为 JSON Schema，不规定跨 Provider 的业务数据模型。

### 非目标

- 不支持 Android 之外的传输；其他平台可以复用语义，但需另行定义 transport binding。
- 不提供跨 Provider 事务，也不保证多个工具调用的原子性。
- 不把 Consumer、LLM 或 Provider 默认视为可信实体。
- 不允许 Consumer 的确认界面代替高风险操作所需的 Provider 侧确认。
- 不在 v2 中定义无人值守的高风险操作授权；定时交易等能力需要单独规范。

## 2. 兼容发现与协商

### 2.1 Manifest 声明

v2 继续使用 v1 的发现 action 和 authority。新增 `com.clawseed.tools.versions`，值为 Provider
支持的、以逗号分隔的主版本集合。

同时支持 v1 和 v2 的 Provider 必须保留旧字段 `version=1`，使 v1 Consumer 仍能发现它：

```xml
<service
    android:name=".cetp.ToolProviderService"
    android:exported="true">
    <intent-filter>
        <action android:name="com.clawseed.action.TOOL_PROVIDER" />
    </intent-filter>
    <meta-data android:name="com.clawseed.tools.authority"
        android:value="com.example.app.clawseed.tools" />
    <meta-data android:name="com.clawseed.tools.version"
        android:value="1" />
    <meta-data android:name="com.clawseed.tools.versions"
        android:value="1,2" />
</service>
```

- 只有 `version` 时，v2 Consumer 将其解释为唯一支持的版本。
- 同时存在两个字段时，`versions` 是权威集合，`version` 仅服务旧 Consumer。
- 双栈 Provider 的 v1 `list_tools` 不得返回有副作用的工具。
- v2-only Provider 使用 `version=2` 和 `versions="2"`；旧 Consumer 会忽略它。
- Consumer 必须确认发现 Service 与 authority 对应的 `ContentProvider` 属于同一个包。

### 2.2 `negotiate`

Consumer 从 manifest 判断双方可能支持 v2 后，必须先调用 `negotiate`。请求 extras：

| Key | Bundle 类型 | 要求 |
|---|---|---|
| `consumer_versions` | `int[]` | 必填，Consumer 支持的主版本，降序排列 |
| `consumer_capabilities` | `String` | 必填，JSON 字符串数组 |

成功响应沿用 Bundle 的 `status="success"` 和 `data` JSON 字符串：

```json
{
  "selected_version": 2,
  "provider_id": "com.example.app",
  "provider_name": "Example App",
  "session_token": "opaque-provider-issued-token",
  "expires_at": "2026-09-02T12:00:00Z",
  "capabilities": [
    "async_operations",
    "cancellation",
    "large_results"
  ],
  "limits": {
    "max_request_bytes": 65536,
    "max_inline_result_bytes": 262144,
    "max_concurrent_requests": 4,
    "idempotency_window_seconds": 86400
  }
}
```

Provider 选择双方共同支持的最高版本，并仅返回双方都支持的可选能力。`session_token`：

- 必须是不透明且不可预测的字符串，最长 256 字节；
- 必须绑定调用 UID、选定版本和能力集合；
- 可以有有效期；Provider 进程重启后可以失效；
- 后续 v2 调用必须携带，失效时返回 `NEGOTIATION_REQUIRED`。

没有共同版本时返回 `UNSUPPORTED_VERSION`，不得静默按其他版本解释请求。

`limits` 必须满足：`max_request_bytes >= 65536`、`max_inline_result_bytes >= 262144`、
`max_concurrent_requests >= 1`、`idempotency_window_seconds >= 86400`。双方仍应设置独立的
本地资源上限；协商值表示协议允许值，不要求 Consumer 无条件分配等量内存。

### 2.3 v2 核心与可选能力

所有 v2 实现都必须支持工具注解、输出 schema、scope 授权、结构化错误、Provider
resolution，以及有副作用调用的幂等去重。以下能力需要协商：

| Capability | 含义 |
|---|---|
| `async_operations` | `execute_tool` 可以返回后台 operation |
| `cancellation` | 支持 `cancel_operation` |
| `large_results` | 使用 `ParcelFileDescriptor` 返回大结果 |

Consumer 不得调用未协商的可选能力。Provider 不得在一次协商会话中改变能力语义；变化后应使
旧 `session_token` 失效并要求重新协商。

## 3. Provider 与工具描述

### 3.1 稳定标识

- `provider_id` 必须稳定，默认为 Android 包名；自定义值必须位于包名命名空间下。
- 工具 `name` 必须匹配 `[a-z][a-z0-9_]{0,63}`，并在一个 Provider 内稳定且唯一。
- Consumer 的本地展示名称可以变化，但路由必须使用 `(provider_id, tool_name)`，不得依赖
  扫描顺序或应用 label。

### 3.2 `get_provider_info`

v2 Provider 必须返回稳定身份、描述和 scope 目录：

```json
{
  "provider_id": "com.example.app",
  "provider_name": "Example App",
  "description": "管理价格提醒和自选列表",
  "schema_dialect": "https://json-schema.org/draft/2020-12/schema",
  "scopes": [
    {
      "name": "alerts.write",
      "title": "管理价格提醒",
      "description": "创建、修改和删除提醒",
      "access": "write",
      "sensitivity": "financial"
    }
  ]
}
```

scope `name` 必须稳定并匹配 `[a-z][a-z0-9_.-]{0,127}`。`access` 为 `read` 或 `write`；
`sensitivity` 为 `public`、`personal`、`financial`、`health`、`device` 或 `other`。这些字段帮助
Consumer 展示授权范围，但授权决定仍由 Provider 执行。

`schema_dialect` 适用于该协商会话中的全部 input/output schema。v2 默认使用 JSON Schema
Draft 2020-12。schema 不得引用网络资源；`$ref` 只能解析同一 schema 文档中的片段。双方必须
忽略描述对象中的未知字段，但不能忽略缺失的必填字段或未知的安全枚举值。

### 3.3 `list_tools`

v2 沿用 `list_tools` method，并要求 extras 包含：

| Key | Bundle 类型 | 要求 |
|---|---|---|
| `protocol_version` | `Int` | 必须为 `2` |
| `session_token` | `String` | `negotiate` 返回的 token |

响应示例：

```json
{
  "revision": "tools-42",
  "tools": [
    {
      "name": "create_alert",
      "title": "创建价格提醒",
      "description": "为指定证券创建价格提醒",
      "input_schema": {
        "type": "object",
        "properties": {
          "symbol": {"type": "string"},
          "price": {"type": "number", "exclusiveMinimum": 0}
        },
        "required": ["symbol", "price"],
        "additionalProperties": false
      },
      "output_schema": {
        "type": "object",
        "properties": {"alert_id": {"type": "string"}},
        "required": ["alert_id"]
      },
      "scopes": ["alerts.write"],
      "annotations": {
        "effect": "create",
        "risk": "moderate",
        "destructive": false,
        "idempotent": false,
        "open_world": false,
        "confirmation": "provider_policy",
        "execution": "sync_or_async"
      }
    }
  ]
}
```

`revision` 在工具集合或其 schema/安全语义变化时必须改变。Consumer 可以缓存相同 revision 的
工具清单，但每次新协商至少检查一次 revision。

### 3.4 工具注解

| 字段 | 允许值 | 语义 |
|---|---|---|
| `effect` | `read`, `create`, `update`, `delete`, `transaction` | 工具对真实世界或持久状态的主要影响 |
| `risk` | `low`, `moderate`, `high` | 执行失败或误调用的影响等级 |
| `destructive` | Boolean | 是否可能造成不可逆数据或资产损失 |
| `idempotent` | Boolean | 相同参数重复执行是否天然产生相同效果 |
| `open_world` | Boolean | 是否读取或影响 Provider 控制范围外的系统 |
| `confirmation` | `never`, `provider_policy`, `always` | Provider 侧逐次确认要求 |
| `execution` | `sync`, `sync_or_async` | 是否可能返回后台 operation |

规范性约束：

- `effect=read` 的工具不得产生调用日志、缓存和计量之外的业务状态变更。
- `effect=delete`、`effect=transaction`、`destructive=true` 或 `risk=high` 时，
  `confirmation` 必须为 `always`。
- `execution=sync_or_async` 仅能在协商了 `async_operations` 时使用。
- 注解是供 Consumer 安排 UI 和策略的声明，不是安全边界；Provider 必须在执行时重新验证。
- Provider 检测到运行时风险高于静态声明时，必须采用更严格策略并可以要求用户操作。

`scopes` 是执行工具所需权限的完整集合。缺少任一 scope 时，Provider 必须返回
`AUTH_REQUIRED` 或 `PERMISSION_DENIED`，不得返回降级数据冒充完整结果。

## 4. 调用模型

v2 定义以下 `ContentResolver.call()` method：

| Method | 能力要求 | 用途 |
|---|---|---|
| `negotiate` | 核心 | 选择版本、能力和限制 |
| `get_provider_info` | 核心 | 获取描述、scope 说明和支持信息 |
| `list_tools` | 核心 | 获取当前调用方可见的工具 |
| `execute_tool` | 核心 | 发起或恢复一次工具执行 |
| `get_operation` | `async_operations` | 查询后台执行状态 |
| `cancel_operation` | `cancellation` | 请求取消后台执行 |

未知 method 必须返回 `METHOD_NOT_FOUND`。Provider 根据请求中的 `protocol_version` 区分同名
v1/v2 method；缺少该字段的双栈调用按 v1 处理。

### 4.1 通用请求字段

除 `negotiate` 外，每个 v2 method 都必须携带：

| Key | Bundle 类型 | 要求 |
|---|---|---|
| `protocol_version` | `Int` | 固定为 `2` |
| `session_token` | `String` | 当前协商会话 |
| `request_id` | `String` | 必填 UUID；一次逻辑执行及其重试保持不变 |
| `deadline_at_ms` | `Long` | 必填，Unix epoch 毫秒 |

`execute_tool` 还包含 `tool_name`、`args` JSON 字符串和可选 `resume_token`。参数 JSON 的
UTF-8 长度不得超过协商的 `max_request_bytes`。

- 对 `execute_tool`，`request_id` 标识逻辑执行，也是幂等键。
- 对 `get_operation` 和 `cancel_operation`，extras 必须包含 `operation_id`，`request_id` 必须
  等于创建该 operation 的执行 ID。
- 对其他 method，`request_id` 标识一次可重试的读取；重试保持不变，新读取使用新 ID。

Provider 必须在开始业务副作用前检查 deadline。Consumer 超时并不代表 Provider 已停止；对于
有副作用或异步调用，Consumer 必须使用相同 `request_id` 查询或重试，不能生成新 ID 猜测结果。

### 4.2 同步成功

响应 Bundle：

```text
status = "success"
protocol_version = 2
request_id = <原请求 ID>
data = {"kind":"inline","value":<任意符合 output_schema 的 JSON>}
```

Provider 必须验证自身输出符合 `output_schema`。Consumer 仍必须把 Provider 描述、错误和输出
视为不可信数据，不得把其中的文本自动提升为 Agent 指令。

### 4.3 Provider 授权与逐次确认

需要登录、scope 授权或用户确认时，Provider 返回 `AUTH_REQUIRED` 或
`USER_ACTION_REQUIRED`。除通用错误字段外，Bundle 必须包含：

| Key | Bundle 类型 | 说明 |
|---|---|---|
| `resolution` | `PendingIntent` | Provider 创建的显式、不可变 PendingIntent |
| `resume_token` | `String` | 与调用方、工具、原始参数、request ID 和有效期绑定的不透明 token |

流程如下：

```text
Consumer             Provider                 Provider UI
   | execute_tool        |                         |
   |-------------------->|                         |
   | USER_ACTION_REQUIRED + PendingIntent         |
   |<--------------------|                         |
   | launch resolution -------------------------->|
   |                         user approves/denies  |
   | execute_tool(same request_id, resume_token)   |
   |-------------------->|                         |
   | success / error     |                         |
```

- Provider 在确认完成前不得产生目标业务副作用。
- 高风险确认必须由 Provider 自己的 UI 完成，Consumer 的“已确认”布尔值无效。
- Provider 确认 Activity 应在用户批准或拒绝后结束；Consumer 等待 Activity result，再恢复原调用。
- `resume_token` 必须单次使用并绑定原始 `args` 字节；参数变化时必须拒绝。
- 用户拒绝时返回 `USER_CANCELLED`。
- v2 不使用 v1 的隐式 `authorize_intent` action 字符串。

### 4.4 异步操作

协商 `async_operations` 后，`execute_tool` 可以返回：

```text
status = "accepted"
protocol_version = 2
request_id = <原请求 ID>
data = {"operation_id":"op_123","poll_after_ms":1000}
```

Consumer 使用 `get_operation` 查询，Provider 返回 `queued`、`running`、`succeeded`、`failed`
或 `cancelled`。成功状态携带与同步调用相同的结果 envelope；失败状态携带结构化错误。

状态查询本身成功时使用 `status="success"`，其 `data` 示例为：

```json
{
  "operation_id": "op_123",
  "state": "running",
  "progress": {"current": 40, "total": 100, "message": "正在同步"},
  "poll_after_ms": 1000
}
```

`progress` 是可选展示数据，不能用于判断业务成功。终态 `succeeded` 包含 `result`，`failed`
包含与第 5 节字段一致的 `error` 对象。`cancel_operation` 成功只表示已接受取消请求，返回
`state="cancellation_requested"`；Consumer 随后仍须查询终态。

- `operation_id` 必须绑定调用 UID，其他调用方不得读取。
- operation 必须至少保留到协商的幂等窗口结束；Provider 可以声明更长 TTL。
- 有副作用请求的去重记录和 operation 状态必须持久化，Provider 进程重启不得导致重复执行。
- `cancellation` 已协商时，Consumer 可以调用 `cancel_operation`。
- 取消是协作式请求。只有查询到 `cancelled` 才能认为没有后续效果；已提交的外部事务可能返回
  `CONFLICT`，不得伪报取消成功。

### 4.5 大结果

内联 JSON 超过 `max_inline_result_bytes` 时，Provider 不得继续放入 Bundle。协商
`large_results` 后，成功 Bundle 使用：

```text
data = {
  "kind":"file",
  "media_type":"application/json",
  "size_bytes":1048576,
  "sha256":"<lowercase hex>"
}
result_fd = <只读 ParcelFileDescriptor>
```

Consumer 必须校验声明的大小、摘要和 `output_schema`，设置本地读取上限，并关闭文件描述符。
文件内容属于一次响应，不得假设路径稳定。未协商 `large_results` 时返回 `RESULT_TOO_LARGE`。

## 5. 错误模型

错误响应必须包含 `status="error"`、`error_code`、面向用户的安全描述 `error_message`、
`retryable` Boolean，并回显 `protocol_version` 和 `request_id`。可选字段为 JSON
`error_details` 与 Long `retry_after_ms`。

| 错误码 | retryable | 含义 |
|---|---:|---|
| `UNSUPPORTED_VERSION` | 否 | 没有共同协议版本 |
| `UNSUPPORTED_CAPABILITY` | 否 | 请求使用了未协商能力 |
| `NEGOTIATION_REQUIRED` | 是 | session 缺失、过期或因 Provider 重启失效 |
| `AUTH_REQUIRED` | 条件性 | 用户需完成登录或 scope 授权 |
| `USER_ACTION_REQUIRED` | 条件性 | 本次调用需要 Provider 侧确认 |
| `USER_CANCELLED` | 否 | 用户拒绝或取消确认 |
| `PERMISSION_DENIED` | 否 | 调用方不允许访问 |
| `TOOL_NOT_FOUND` | 否 | 工具不存在或对调用方不可见 |
| `METHOD_NOT_FOUND` | 否 | method 不存在 |
| `INVALID_ARGS` | 否 | 参数不符合 input schema |
| `INVALID_REQUEST` | 否 | envelope、token 或 request ID 使用错误 |
| `CONFLICT` | 条件性 | 当前状态不允许操作或无法取消 |
| `RATE_LIMITED` | 是 | 超出 Provider 限流 |
| `DEADLINE_EXCEEDED` | 条件性 | deadline 已过；需先查询相同 request ID |
| `CANCELLED` | 否 | 操作已确认取消 |
| `RESULT_TOO_LARGE` | 否 | 未协商大结果传输且无法内联 |
| `RESULT_EXPIRED` | 否 | operation 结果已过保留期 |
| `UNAVAILABLE` | 是 | Provider 暂时不可用 |
| `INTERNAL_ERROR` | 条件性 | Provider 内部错误，不得暴露堆栈或敏感信息 |

Consumer 只能在 `retryable=true` 时自动重试，并必须遵守 `retry_after_ms`。授权、确认和参数错误
不得在没有用户动作或输入变化时循环重试。

## 6. 幂等、重试与并发

- 所有 `effect != read` 的工具必须按 `request_id` 去重，无论 `idempotent` 注解为何值。
- 去重键至少包含调用 UID、`provider_id` 和 `request_id`；记录必须绑定 tool name 和原始 args
  摘要。同一 ID 携带不同内容时返回 `INVALID_REQUEST`。
- Provider 必须在 `idempotency_window_seconds` 内返回相同的终态结果，最小窗口为 86400 秒。
- 并发到达的相同请求只能有一个执行者，其他调用返回同一 operation 或终态结果。
- Consumer 遇到 Binder 断开或超时时，只能以相同 ID 重试或查询状态。
- `idempotent=true` 表示业务操作天然可重复，不免除 request ID 去重义务。

## 7. 安全模型

### 7.1 调用方身份

Provider 必须在 `ContentProvider.call()` 入口同步捕获 `Binder.getCallingUid()`，再进行任何异步
切换。包名、`args` 中的 caller 字段和 Consumer 自报身份都不可信。

授权时 Provider 应：

1. 获取 UID 对应的全部包，而不是只取第一个包名。
2. 校验当前签名证书摘要，并按需接受 Android signing certificate rotation history。
3. 将授权绑定到 UID、包名、证书摘要、scopes 和有效期。
4. 对共享 UID、多包映射或签名变化采用拒绝或重新授权策略。

`com.clawseed.permission.ACCESS_TOOLS` 仍只是发现入口和纵深防御；`normal` 权限不是身份或授权
边界。

### 7.2 最小权限与确认

- `list_tools` 应只返回当前调用方可以发现的工具；敏感工具可以完全隐藏。
- Provider 必须逐次检查 scopes，不能依赖先前 `list_tools` 的结果。
- `PendingIntent` 必须显式指向 Provider 自身组件并使用 immutable flag。
- 高风险 UI 必须显示具体对象、动作、关键参数和不可逆后果，不能只显示工具描述。
- Provider 应记录不含敏感参数的审计事件：调用方、工具、request ID、授权/确认结果和终态。

### 7.3 不可信内容

Provider 名称、工具描述、schema、错误和结果都可能包含恶意内容。Consumer 必须限制长度、验证
JSON/schema，并将工具输出标记为数据。Provider 也必须把 Agent 生成的参数视为不可信输入，
完成业务校验，不能只依赖 JSON Schema。

## 8. v1 共存与迁移

| 阶段 | Provider | Consumer |
|---|---|---|
| 0：现状 | 继续提供 v1 只读工具 | 保持当前 v1 行为 |
| 1：双栈发现 | 增加 `versions="1,2"`，v1 清单只保留只读子集 | 识别 `versions`，实现 `negotiate` |
| 2：v2 只读 | 为现有工具补 schema、scope、注解和结构化错误 | Provider 声明 v2 时必须协商成功，否则禁用该 Provider |
| 3：受控写入 | 仅通过 v2 暴露写工具，实现确认和去重 | 按风险展示状态，正确恢复确认流程 |
| 4：异步/大结果 | 按需启用可选能力 | 仅在协商后使用对应 method |

回退规则：

- Consumer 仅在 Provider manifest 包含 v1 且未声明 v2 时使用 v1；声明 v2 后协商失败不得降级。
- v2 协商、授权或安全校验失败不得降级到 v1 绕过限制。
- v1 Consumer 永远只能看到双栈 Provider 的只读子集。
- 已经通过 v1 暴露的有副作用扩展应迁移到 v2；迁移完成前 Consumer 应默认禁用或要求本地
  人工审批，且不得宣称其符合 v1。

## 9. 一致性测试要求

正式发布 v2 前应提供共享测试向量和 Android Provider/Consumer 测试套件，至少覆盖：

- manifest 版本集合、无共同版本和双栈回退；
- 未知字段忽略、必填字段缺失、无效 JSON 与 schema 不匹配；
- 调用方 UID/签名变化、scope 过期、PendingIntent 绑定与 token 重放；
- 同 ID 并发、超时后重试、不同参数复用 ID 和幂等窗口；
- 用户批准、拒绝、token 过期以及确认前无副作用；
- operation 成功、失败、取消竞争和 Provider 重启恢复；
- Binder 内联边界、文件摘要错误、超限读取和文件描述符关闭；
- 恶意 Provider 的超长描述、伪造指令、未知错误码和敏感错误内容。

只有核心测试全部通过的实现才能声明 CETP v2；可选能力必须分别声明并通过对应测试。

## 10. Android SDK 实现

SDK 提供 `CetpClient`、`ExternalToolBridge` 和可继承的 `CetpV2ContentProvider`。Provider 的
最小结构如下：

```kotlin
class AlertToolProvider : CetpV2ContentProvider() {
    override val provider = CetpProviderDescriptor(
        providerId = "com.example.app",
        providerName = "Example App",
        description = "价格提醒",
        scopes = listOf(
            ProviderScope(
                name = "alerts.write",
                description = "管理价格提醒",
                access = ScopeAccess.WRITE,
                sensitivity = ScopeSensitivity.FINANCIAL,
            ),
        ),
    )

    override val providerTools = listOf(
        CetpProviderTool(
            name = "create_alert",
            title = "创建价格提醒",
            description = "为指定证券创建价格提醒",
            inputSchema = createAlertInputSchema,
            outputSchema = createAlertOutputSchema,
            scopes = listOf("alerts.write"),
            annotations = ToolAnnotations(
                effect = ToolEffect.CREATE,
                risk = ToolRisk.MODERATE,
                idempotent = false,
                confirmation = ConfirmationPolicy.PROVIDER_POLICY,
            ),
        ),
    )

    override val mutationExecutor = appDurableMutationExecutor

    override fun authorize(request: CetpProviderRequest): CetpAccessDecision =
        appAuthorizationPolicy.authorize(request)

    override fun executeTool(request: CetpProviderRequest): CetpProviderResult =
        alertRepository.execute(request)
}
```

`CetpV2ContentProvider` 默认拒绝授权，并在高风险声明不一致、deadline 过期、会话无效或写工具
没有 `CetpMutationExecutor` 时拒绝执行。应用提供的 mutation executor 必须把请求预留、业务变更
和结果持久化纳入同一可靠边界；SDK 不可能替业务数据库替代这一原子性。

SDK 负责协议 envelope、协商 token、调用方 UID/签名收集、结构化错误、异步轮询/取消、文件
结果校验和 Provider UI 恢复。Provider 应用仍负责 JSON Schema 与业务规则验证、scope 策略、
确认 token 状态、operation 持久化和审计记录。
