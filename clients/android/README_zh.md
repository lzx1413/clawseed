# ClawSeed Android Demo

ClawSeed 的 Android 客户端，在设备本地运行 clawseed gateway（编译为 `.so` 的原生二进制），通过 WebSocket/REST 与之通信，提供 LLM 对话、工具调用、会话管理和配置编辑功能。

## 架构

```
┌──────────────────────────────────────────────────────────┐
│  UI Layer (Jetpack Compose + Material 3)                 │
│  ┌──────────┐  ┌──────────┐  ┌───────────────────────┐  │
│  │ChatScreen│  │ Drawer   │  │SettingsScreen         │  │
│  │  + Bubble│  │(Sessions)│  │(Form / TOML编辑)      │  │
│  └────┬─────┘  └────┬─────┘  └──────────┬────────────┘  │
│       │              │                    │               │
│  ┌────┴─────┐  ┌─────┴────────┐  ┌──────┴────────────┐  │
│  │ChatVM    │  │SessionsVM    │  │SettingsVM         │  │
│  └────┬─────┘  └─────┬────────┘  └──────┬────────────┘  │
├───────┼──────────────┼──────────────────┼────────────────┤
│  SDK 层                                                   │
│  ┌────┴────────────────┴─────────────────┴──────────────┐ │
│  │  sdk:android (ClawSeedAndroid, SessionManager,       │ │
│  │              ChatAccumulator, ClawSeedViewModel)      │ │
│  ├───────────────────────────────────────────────────────┤ │
│  │  sdk:embedded (EmbeddedGateway, GatewayService,      │ │
│  │                 GatewayConfigManager)                 │ │
│  ├───────────────────────────────────────────────────────┤ │
│  │  sdk:core (ClawSeedSession, ChatClient, GatewayClient│ │
│  │            ToolRegistry, models)                      │ │
│  └───────────────────────────┬───────────────────────────┘ │
│  ┌────────────────────┐  ┌──┴──────────────────┐          │
│  │LocalStore (DataStore)│  │GatewayApi (REST)   │          │
│  └────────────────────┘  └─────────────────────┘          │
└───────────────────────────┼───────────────────────────────┘
                            │ localhost:42617
                    ┌───────┴───────┐
                    │ clawseed      │
                    │ gateway       │
                    │ (native .so)  │
                    └───────────────┘
```

### 模块

| 模块 | 包名 | 说明 |
|------|------|------|
| `app` | `dev.clawseed.demo` | 主应用：UI、Service、ViewModel、数据层 |
| `sdk:core` | `dev.clawseed.sdk.core` | 核心抽象：会话、聊天/WebSocket 客户端、工具注册、模型定义 |
| `sdk:android` | `dev.clawseed.sdk.android` | Android 特有：ClawSeedAndroid 单例、SessionManager、ChatAccumulator、CETP 外部工具桥接 |
| `sdk:embedded` | `dev.clawseed.sdk.embedded` | 嵌入式 Gateway：进程管理、配置、前台服务 |

### 目录结构

```
app/src/main/kotlin/dev/clawseed/demo/
├── MainActivity.kt              # 入口 Activity，绑定 Service
├── ClawseedApp.kt               # 根 Composable，导航 + 侧边栏
├── CoordinateConverter.kt       # WGS84 → GCJ-02 坐标转换
├── data/
│   ├── ChatModels.kt            # 数据模型 (ChatEntry, ChatSession, ToolInfo 等)
│   └── LocalStore.kt            # DataStore 本地持久化
└── ui/
    ├── navigation/
    │   └── ClawseedNavHost.kt   # 导航路由 (Chat / Settings)
    ├── chat/
    │   ├── ChatScreen.kt        # 聊天主界面
    │   ├── ChatViewModel.kt     # 聊天逻辑 + 工具注册
    │   └── components/
    │       ├── ChatBottomBar.kt  # 输入框 + 发送/停止按钮
    │       ├── MessageBubble.kt  # 消息气泡（用户/助手/工具/思考）
    │       └── MarkdownContent.kt# Markdown 渲染（标题/代码/表格/内联格式）
    ├── drawer/
    │   ├── SessionDrawer.kt     # 会话历史侧边栏
    │   └── SessionsViewModel.kt # 会话 CRUD
    └── settings/
        ├── SettingsScreen.kt    # 配置界面（表单 + TOML 两种模式）
        └── SettingsViewModel.kt # LLM 提供商配置、搜索引擎、模型获取

sdk/core/src/main/kotlin/dev/clawseed/sdk/core/
├── ClawSeed.kt                  # 会话工厂接口
├── ClawSeedConfig.kt            # SDK 配置
├── ClawSeedSession.kt           # 会话接口
├── DefaultClawSeedSession.kt    # 默认会话实现
├── client/
│   ├── ChatClient.kt            # WebSocket 聊天客户端（连接、发送、工具分发）
│   ├── GatewayClient.kt         # REST API 客户端（会话、配置、工具、状态）
│   └── ReconnectPolicy.kt       # 自动重连策略
├── model/
│   ├── ChatEvent.kt             # 聊天事件类型（chunk, thinking, tool_call 等）
│   ├── ConnectionState.kt       # WebSocket 连接状态
│   ├── Gateway.kt               # Gateway 状态模型
│   └── Session.kt               # 会话模型
└── tool/
    ├── ClawSeedTool.kt          # 工具接口
    ├── ToolRegistry.kt          # 客户端工具注册表
    ├── ToolResult.kt            # 工具执行结果
    └── ToolSpec.kt              # 工具规格定义

sdk/android/src/main/kotlin/dev/clawseed/sdk/android/
├── ClawSeedAndroid.kt           # 单例：SDK 初始化 + gateway 客户端访问
├── SessionManager.kt            # 会话生命周期管理
├── ChatAccumulator.kt           # 流式片段累积为消息
├── AccumulatedMessage.kt        # 累积消息模型
├── ClawSeedViewModel.kt         # 聊天 ViewModel 基类
└── cetp/
    ├── CetpConstants.kt         # CETP v1 协议常量
    ├── CetpModels.kt            # 数据类（DiscoveredProvider, AuthRequiredEvent 等）
    ├── CetpClient.kt            # ContentResolver.call() 封装
    ├── ExternalToolBridge.kt    # 发现 Provider，桥接工具到 ToolRegistry
    └── PackageChangeReceiver.kt # 监听应用安装/更新/卸载的广播接收器

sdk/embedded/src/main/kotlin/dev/clawseed/sdk/embedded/
├── EmbeddedGateway.kt           # Gateway 进程生命周期管理
├── EmbeddedGatewayConfig.kt     # Gateway 启动配置（端口、二进制名、超时）
├── GatewayConfigManager.kt      # TOML 配置创建、修补、web_search 默认值
├── GatewayService.kt            # Android 前台服务，管理 gateway 进程
└── GatewayState.kt              # Gateway 状态模型
```

## 功能

### 对话
- WebSocket 实时流式输出
- 按会话保存完整消息历史
- Extended Thinking 展示（折叠/展开）
- Debug 模式查看完整 prompt 和 token 估算

### 会话管理
- 创建 / 恢复 / 重命名 / 删除会话
- 首条消息自动命名
- 侧边栏显示历史会话列表

### 工具调用
- **设备端工具**（通过 WebSocket 注册）：
  - `device_info` — 设备型号、厂商、Android 版本
  - `get_location` — GPS 定位（WGS84→GCJ-02）+ 逆地理编码
- **CETP 外部工具**（自动发现第三方 App）：
  - 自动发现实现了 [CETP v1 协议](../../docs/zh/external-tool-protocol.md) 的第三方应用
  - Provider 工具自动添加命名空间前缀（如 `finance__get_portfolio_holdings`），注册为 RemoteTool
  - 支持 `AUTH_REQUIRED` 授权流程，提供提示和授权引导
  - 监听应用安装/更新/卸载广播动态刷新
- **Gateway 内置工具**：web_fetch、http_request、web_search 等

### LLM 配置
- 11 个预设提供商（DeepSeek、Qwen、Moonshot、GLM、Doubao、百度、OpenAI、Anthropic、OpenRouter、Ollama、自定义）
- 模型列表获取（直连或通过 Gateway 代理）
- Thinking Mode 开关
- 表单编辑 或 TOML 直接编辑

### 搜索引擎配置
- 搜索引擎选择器（Bing / Tavily）
- Tavily API Key 输入框，带密码显示/隐藏切换
- 免费获取 Tavily API Key 链接（每月 1,000 次调用）
- 配置写入 `[web_search]` TOML 段

### Gateway 状态与工具
- 状态卡片显示当前 Provider、Model、Memory 后端
- 可展开的已注册工具列表，显示来源类型（Built-in / Remote / MCP）

### Markdown 渲染
- 标题（h1-h6）、代码块（带语言标签+复制按钮）、列表、表格
- 内联格式：**加粗**、*斜体*、`等宽`

### 开发者选项
- Debug Query Message 开关 — 每条消息显示完整 LLM prompt 和 token 估算

## 默认 Gateway 配置

首次启动时，App 自动生成 TOML 配置，默认启用网络功能：

```toml
workspace_dir = "{WORKSPACE_DIR}"

[gateway]
session_persistence = true

[web_fetch]
enabled = true
allowed_domains = ["*"]

[http_request]
enabled = true
allowed_domains = ["*"]

[web_search]
enabled = true
provider = "bing"
```

如需使用 Tavily 替代 Bing，通过设置页面或直接编辑 TOML：

```toml
[web_search]
enabled = true
provider = "tavily"
tavily_api_key = "tvly-..."
```

## 通信协议

### WebSocket

地址：`ws://127.0.0.1:42617/ws/chat?session_id={id}`

| 方向 | 类型 | 说明 |
|------|------|------|
| → | `message` | 发送用户消息 |
| → | `register_tools` | 注册设备端工具 |
| → | `tool_result` | 返回工具执行结果 |
| ← | `chunk` | 流式文本片段 |
| ← | `thinking` | 思考过程片段 |
| ← | `tool_call_request` | 请求执行工具 |
| ← | `done` | 回复完成 |
| ← | `aborted` | 回复被中止 |

### REST API

基础地址：`http://127.0.0.1:42617`

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/api/sessions` | 会话列表 |
| GET | `/api/sessions/{id}/messages` | 会话消息历史 |
| PUT | `/api/sessions/{id}` | 重命名会话 |
| DELETE | `/api/sessions/{id}` | 删除会话 |
| POST | `/api/sessions/{id}/abort` | 中止生成 |
| GET | `/api/config` | 获取 TOML 配置 |
| PUT | `/api/config` | 更新配置 |
| GET | `/api/tools` | 已注册工具列表 |
| GET | `/api/status` | Gateway 状态 |
| GET | `/api/provider/models` | 通过 Gateway 代理获取模型列表 |

## 构建

```bash
# 先编译 clawseed native binary (需要 NDK)
cd /path/to/claw-seed
./tools/build-clawseed-android.sh aarch64 build

# 构建 APK
cd clients/android
./gradlew assembleDebug

# 安装
adb install -r app/build/outputs/apk/debug/app-debug.apk
```

### 依赖

- Android SDK 36, minSdk 26
- Kotlin + Jetpack Compose (Material 3)
- OkHttp 4.12 (HTTP + WebSocket)
- kotlinx-serialization-json (JSON)
- AndroidX DataStore (本地持久化)
- `libclawseed.so` (clawseed gateway 原生二进制，JNI legacy packaging)
