# ClawSeed External Tool Protocol (CETP) v1

定义第三方 Android 应用如何向 ClawSeed 暴露只读数据的协议。

## 1. 设计目标

Android 上大量工具类应用的核心价值是数据，但每个 App 各自实现 AI 聊天界面既重复又低质。CETP 的核心理念：**ClawSeed 做统一的 AI 交互层，其他 App 只做数据供给方。**

- v1 仅定义只读工具的发现与调用
- 第三方 App 不直接连接 gateway，由 ClawSeed Android 客户端发现并桥接
- Bridge 将外部工具包装为本地工具，通过现有 WebSocket `register_tools` 注册给 gateway
- 工具规格复用 JSON Schema，与 `Tool` trait 完全对齐
- 协议不限定 Consumer 必须是官方 ClawSeed，访问控制由 Provider 自主决定
- 最低接入门槛——实现一个 ContentProvider，约 100 行代码

### v1 范围

- 只读工具的发现（`list_tools`）与调用（`execute_tool`）
- 可选的 Provider 元信息查询（`get_provider_info`）
- Provider 自主授权模型（`AUTH_REQUIRED` + 用户完成授权后重试）
- Android ContentProvider 作为跨进程通信机制

### 非目标

- 不定义写操作协议——下单、转账、删除等留给 v2
- 不定义统一 OAuth 风格授权协议——授权策略由 Provider 自主决定
- 不保证 Consumer 身份强校验——包名识别仅用于 Provider 自主授权
- 不保证 Provider 间统一数据模型——各 App 自定义返回结构
- 不要求 Provider 实现严格幂等——只读场景不需要

## 2. 协议定义

### 2.1 角色

| 角色 | 说明 |
|---|---|
| **Tool Provider** | 第三方 App，通过 ContentProvider 暴露只读工具 |
| **Tool Consumer** | 任意兼容 CETP 的 Android App，发现并调用外部工具 |

### 2.2 发现机制

Provider 在 `AndroidManifest.xml` 中声明 Service + ContentProvider：

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
</service>

<provider
    android:name=".cetp.ToolProvider"
    android:authorities="com.example.app.clawseed.tools"
    android:exported="true"
    android:permission="com.clawseed.permission.ACCESS_TOOLS"
    android:initOrder="100" />
```

- `com.clawseed.permission.ACCESS_TOOLS`（`protectionLevel="normal"`）用于减少误调用，**不构成强安全边界**
- Consumer 需声明 `<uses-permission android:name="com.clawseed.permission.ACCESS_TOOLS" />`
- `android:initOrder="100"` 确保 Provider 优先初始化（部分厂商 ROM 需要）

Consumer 通过 `PackageManager.queryIntentServices()` 扫描 `com.clawseed.action.TOOL_PROVIDER` intent，从 meta-data 提取 authority。

### 2.3 数据交换

所有交互通过 `ContentResolver.call()` 完成，统一响应格式：

**成功：** `{ "status": "success", "data": "<JSON字符串>" }`

**失败：** `{ "status": "error", "error_code": "<错误码>", "error_message": "<描述>" }`

`AUTH_REQUIRED` 错误可附带可选字段：

```json
{
  "status": "error",
  "error_code": "AUTH_REQUIRED",
  "error_message": "用户尚未授权该数据范围",
  "resolution_hint": "请打开 App 完成授权",
  "authorize_intent": "com.example.app.ACTION_AUTHORIZE_CLAWSEED"
}
```

#### `list_tools`

| | 值 |
|---|---|
| method | `"list_tools"` |
| extras | null |

响应 `data`：

```json
{
  "tools": [
    {
      "name": "get_data",
      "description": "自然语言描述，Agent 据此决定何时调用",
      "parameters": { "<JSON Schema>" }
    }
  ]
}
```

- `name`：在单个 Provider 内唯一即可。Consumer 负责命名空间隔离（如前缀 `app__get_data`）
- `parameters`：标准 JSON Schema，与 `Tool::parameters_schema()` 对齐

#### `execute_tool`

| | 值 |
|---|---|
| method | `"execute_tool"` |
| extras | `tool_name`, `args` (JSON), `request_id` (可选 UUID) |

v1 中 **仅允许只读操作**，不应产生写入、提交、删除类副作用。

`request_id` 用于日志关联和重试识别，v1 不强制要求严格幂等。

响应 `data`：工具返回的自由结构 JSON。

#### `get_provider_info`（可选）

| | 值 |
|---|---|
| method | `"get_provider_info"` |
| extras | null |

响应 `data`：

```json
{
  "provider_name": "MyApp",
  "description": "数据说明",
  "scopes": [
    { "name": "scope_name", "description": "范围描述" }
  ]
}
```

### 2.4 错误码

| 错误码 | 含义 |
|---|---|
| `AUTH_REQUIRED` | 用户需先在 Provider App 中完成认证或授权 |
| `PERMISSION_DENIED` | 调用方未被 Provider 授权访问 |
| `TOOL_NOT_FOUND` | 请求的工具名不存在 |
| `INVALID_ARGS` | 参数不符合 schema |
| `RATE_LIMITED` | 调用频率超限 |
| `INTERNAL_ERROR` | Provider 内部错误 |

### 2.5 版本兼容

- `com.clawseed.tools.version` meta-data 标识协议版本
- Consumer 应检查版本号，对未知版本做降级处理
- 新增的 `call()` method 不影响旧版本（旧版返回 `TOOL_NOT_FOUND`）
- 工具规格中新增的字段向后兼容（Consumer 忽略未知字段）

## 3. 实现逻辑

### 3.1 Consumer 桥接架构

```
Provider App (ContentProvider)
  │  ContentResolver.call()
  ▼
ClawSeed Android (ExternalToolBridge)
  │  包装为本地工具 + 命名空间前缀
  │  WebSocket register_tools
  ▼
ClawSeed Gateway → Agent (无感知差异)
```

ExternalToolBridge 职责：
1. 扫描 PackageManager 发现 Provider
2. 调用 `list_tools` 获取规格
3. 添加命名空间前缀（`{label}__{tool_name}`）
4. 注册到 gateway（复用现有 RemoteTool 机制）
5. Agent 调用时转发回 Provider
6. 监听 `PACKAGE_ADDED` / `PACKAGE_REPLACED` / `PACKAGE_REMOVED` 广播动态刷新

### 3.2 Provider 自主授权

CETP v1 不定义统一授权协议。Provider 可通过 `Binder.getCallingUid()` + `PackageManager` 反查调用方包名，自主决定：
- 维护已授权 caller 白名单
- 首次调用时返回 `AUTH_REQUIRED`，附带 `resolution_hint` 和 `authorize_intent`
- 对不同 caller 返回不同粒度的数据（脱敏/完整）
- 设定授权有效期

### 3.3 Provider 进程保活

Android 理论上按需拉起 ContentProvider 进程，但部分厂商 ROM（MIUI、ColorOS）的后台管理可能阻止自动启动。

**Provider 建议：**
- 声明 `BOOT_COMPLETED` 广播接收器确保进程可用
- `<provider>` 设置 `android:initOrder="100"`

**Consumer 容错：**
- `acquireContentProviderClient()` 触发进程创建
- `startActivity(launchIntent)` 启动 Provider App
- 以上均失败则等待下次 Session 连接或 `PACKAGE_ADDED` 广播时重试
