# ClawSeed Android Demo

ClawSeed 的 Android 客户端，在设备本地运行 clawseed gateway（编译为 `.so` 的原生二进制），通过 WebSocket/REST 与之通信，提供 LLM 对话、工具调用、会话管理和配置编辑功能。

## 架构

```
┌─────────────────────────────────────────────────────┐
│  UI Layer (Jetpack Compose + Material 3)            │
│  ┌──────────┐  ┌──────────┐  ┌──────────────────┐  │
│  │ChatScreen│  │ Drawer   │  │SettingsScreen    │  │
│  │  + Bubble│  │(Sessions)│  │(Form / TOML编辑) │  │
│  └────┬─────┘  └────┬─────┘  └────────┬─────────┘  │
│       │              │                 │             │
│  ┌────┴─────┐  ┌─────┴────────┐  ┌────┴──────────┐ │
│  │ChatVM    │  │SessionsVM    │  │SettingsVM     │ │
│  └────┬─────┘  └─────┬────────┘  └────┬──────────┘ │
├───────┼──────────────┼─────────────────┼────────────┤
│  Data Layer           │                             │
│  ┌────┴───────────────┴────────────────┴──────────┐ │
│  │           ClawseedService (Foreground)         │ │
│  │  - 启动/管理 gateway 进程                       │ │
│  │  - WebSocket 连接 & 消息收发                    │ │
│  │  - 工具注册 & 调用分发                          │ │
│  └────────────────────┬───────────────────────────┘ │
│  ┌────────────────────┤                             │
│  │GatewayApi (REST)   │  LocalStore (DataStore)     │
│  └────────────────────┘                             │
└───────────────────────┼─────────────────────────────┘
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
| `lib` | `dev.clawseed.client` | 可复用的 WebSocket 客户端库 |

### 目录结构

```
app/src/main/kotlin/dev/clawseed/demo/
├── MainActivity.kt              # 入口 Activity，绑定 Service
├── ClawseedApp.kt               # 根 Composable，导航 + 侧边栏
├── ClawseedService.kt           # 前台服务，管理 gateway 进程和 WebSocket
├── CoordinateConverter.kt       # WGS84 → GCJ-02 坐标转换
├── data/
│   ├── ChatModels.kt            # 数据模型 (ChatEntry, ChatSession, ToolInfo 等)
│   ├── GatewayApi.kt            # REST API 客户端 (OkHttp)
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
        └── SettingsViewModel.kt # LLM 提供商配置、模型获取

lib/src/main/kotlin/dev/clawseed/client/
├── ClawseedClient.kt            # WebSocket 客户端（Builder 模式）
└── ClawseedMessages.kt          # 消息类型定义 & JSON 序列化
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
- **Gateway 内置工具**：web_fetch、http_request、web_search 等

### LLM 配置
- 11 个预设提供商（DeepSeek、Qwen、Moonshot、GLM、Doubao、百度、OpenAI、Anthropic、OpenRouter、Ollama、自定义）
- 模型列表获取（直连或通过 Gateway 代理）
- Thinking Mode 开关
- 表单编辑 或 TOML 直接编辑

### Markdown 渲染
- 标题（h1-h6）、代码块（带语言标签+复制按钮）、列表、表格
- 内联格式：**加粗**、*斜体*、`等宽`

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
- Gson (JSON)
- AndroidX DataStore (本地持久化)
- `libclawseed.so` (clawseed gateway 原生二进制，JNI legacy packaging)
