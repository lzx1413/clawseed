# 语音输入功能设计文档

## 概述

为 Android 聊天 Demo 添加类似豆包的语音输入功能。用户点击麦克风图标切换到语音模式，输入框变为「按住说话」按钮，长按录音，上滑取消，松开识别并自动发送。

## 交互设计

### 模式切换

- 输入框左侧新增麦克风/键盘切换按钮
- 点击麦克风图标 → 检查 `RECORD_AUDIO` 权限 → 进入语音模式
- 点击键盘图标 → 回到文字输入模式

### 语音模式交互流程

```
[按住说话] 按钮
    │
    ├── 长按 → 开始录音，显示全屏录音浮层
    │     │
    │     ├── 手指不动/小幅移动 → 浮层显示「松开发送」
    │     │     │
    │     │     └── 松开 → 停止录音 → 识别文字 → 自动发送
    │     │
    │     └── 手指上滑超过 80dp → 浮层切换为「松开取消」(红色)
    │           │
    │           ├── 松开 → 取消录音，不发送
    │           │
    │           └── 手指滑回 → 恢复「松开发送」状态
    │
    └── 短按（非长按） → 无响应
```

### 录音浮层 UI

```
┌─────────────────────────────┐
│                             │
│     半透明遮罩 (全屏)        │
│                             │
│         ┌─────┐             │
│         │ 🎤  │  脉冲动画    │
│         └─────┘             │
│                             │
│    "你好，请问今天..."       │  ← 实时识别的部分文字
│                             │
│       松开发送               │  ← 提示文字
│    (取消状态时变为红色         │
│     "松开取消")              │
│                             │
└─────────────────────────────┘
```

## 技术方案

### 语音识别

使用 Android 内置 `android.speech.SpeechRecognizer` API：
- 无需额外依赖，属于 Android framework
- 支持实时部分识别结果（`EXTRA_PARTIAL_RESULTS`）
- 默认使用中文识别（`zh-CN`）
- 长按开始 `startListening()`，松开调用 `stopListening()` 触发最终结果，上滑调用 `cancel()` 丢弃

### 状态模型

```kotlin
// 语音输入按钮状态
enum class VoiceInputState { IDLE, LISTENING, CANCELLING }

// 语音识别结果状态
data class RecognitionState(
    val isListening: Boolean = false,
    val partialText: String = "",      // 实时部分识别文字
    val finalText: String? = null,     // 最终识别结果（非空时触发发送）
    val error: SpeechError? = null,    // 错误
)

enum class SpeechError { NO_MATCH, NETWORK, NO_SERVICE, PERMISSION_DENIED, UNKNOWN }
```

### 手势处理

使用 Compose 的 `Modifier.pointerInput` + `detectDragGesturesAfterLongPress`：
- `onDragStart` → 进入 LISTENING 状态，开始语音识别
- `onDrag` → 跟踪 Y 轴偏移，超过 -80dp 进入 CANCELLING
- `onDragEnd` → LISTENING 时提交，CANCELLING 时取消
- `onDragCancel` → 取消

`detectDragGesturesAfterLongPress` 内置长按检测，短按不会触发录音。

## 文件变更

### 新建文件

| 文件 | 说明 |
|------|------|
| `app/src/main/kotlin/dev/clawseed/demo/speech/SpeechRecognizerManager.kt` | SpeechRecognizer 封装，暴露 StateFlow |
| `app/src/main/kotlin/dev/clawseed/demo/ui/chat/components/VoiceInputButton.kt` | 「按住说话」按钮 + 录音浮层 Composable |

### 修改文件

| 文件 | 变更内容 |
|------|----------|
| `app/src/main/AndroidManifest.xml` | 添加 `RECORD_AUDIO` 权限 |
| `app/build.gradle.kts` | 添加 `material-icons-extended` 依赖（提供 Mic/Keyboard 图标） |
| `app/.../components/ChatBottomBar.kt` | 添加语音/键盘模式切换，条件渲染输入框或语音按钮 |
| `app/.../chat/ChatScreen.kt` | 串联权限请求、SpeechRecognizerManager 生命周期、状态管理、浮层渲染 |

**不需要修改 `ChatViewModel.kt`** — 语音识别出的文字直接走现有的 `sendMessage(content)` 路径。

### 详细设计

#### SpeechRecognizerManager

```kotlin
class SpeechRecognizerManager(private val context: Context) {
    private val _state = MutableStateFlow(RecognitionState())
    val state: StateFlow<RecognitionState> = _state.asStateFlow()

    fun startListening(locale: Locale = Locale.getDefault()) {
        // 1. 检查 SpeechRecognizer.isRecognitionAvailable()
        // 2. 创建 SpeechRecognizer（必须主线程）
        // 3. 设置 RecognitionListener:
        //    - onPartialResults → 更新 partialText
        //    - onResults → 设置 finalText
        //    - onError → 映射错误码到 SpeechError
        // 4. 构建 Intent: ACTION_RECOGNIZE_SPEECH, zh-CN, PARTIAL_RESULTS=true
        // 5. startListening(intent)
    }

    fun stopListening()  // → 触发最终识别结果
    fun cancel()         // → 丢弃，不返回结果
    fun destroy()        // → 释放资源
}
```

#### ChatBottomBar 改造

```kotlin
@Composable
fun ChatBottomBar(
    // 现有参数不变
    input: String, onInputChange: (String) -> Unit,
    onSend: () -> Unit, onStop: (() -> Unit)?, enabled: Boolean,
    // 新增参数
    isVoiceMode: Boolean,
    onToggleVoiceMode: () -> Unit,
    voiceState: VoiceInputState,
    partialText: String,
    onStartListening: () -> Unit,
    onCommitVoice: () -> Unit,
    onCancelVoice: () -> Unit,
    onVoiceDragStateChange: (VoiceInputState) -> Unit,
) {
    Row(...) {
        // 左侧：模式切换按钮（麦克风 ↔ 键盘）
        IconButton(onClick = onToggleVoiceMode) {
            Icon(if (isVoiceMode) Icons.Default.Keyboard else Icons.Default.Mic)
        }

        // 中间：条件渲染
        if (isVoiceMode) {
            VoiceInputButton(...)  // 按住说话按钮
        } else {
            OutlinedTextField(...)  // 现有文字输入框
        }

        // 右侧：发送/停止按钮（语音模式下隐藏发送，保留停止）
        if (onStop != null) { /* 停止按钮 */ }
        else if (!isVoiceMode) { /* 发送按钮 */ }
    }
}
```

#### ChatScreen 集成

```kotlin
// 状态
var isVoiceMode by remember { mutableStateOf(false) }
var voiceState by remember { mutableStateOf(VoiceInputState.IDLE) }
val speechManager = remember { SpeechRecognizerManager(context.applicationContext) }
val recognitionState by speechManager.state.collectAsState()

// 权限
val audioPermissionLauncher = rememberLauncherForActivityResult(
    ActivityResultContracts.RequestPermission()
) { granted -> if (granted) isVoiceMode = true }

// 自动发送识别结果
LaunchedEffect(recognitionState.finalText) {
    recognitionState.finalText?.takeIf { it.isNotBlank() }?.let {
        viewModel.sendMessage(it)
    }
}

// 错误处理
LaunchedEffect(recognitionState.error) {
    // 显示中文 Snackbar 错误提示
}

// 生命周期
DisposableEffect(Unit) { onDispose { speechManager.destroy() } }

// UI: 录音浮层渲染在 Scaffold 之上
Box(Modifier.fillMaxSize()) {
    Scaffold(...) { ... }
    if (voiceState != VoiceInputState.IDLE) {
        VoiceRecordingOverlay(state = voiceState, partialText = recognitionState.partialText)
    }
}
```

## 异常处理

| 场景 | 处理方式 |
|------|----------|
| 麦克风权限被拒绝 | Snackbar "需要麦克风权限才能使用语音输入"，保持键盘模式 |
| 设备不支持语音识别 | Snackbar "此设备不支持语音识别" |
| 未识别到语音 | Snackbar "未识别到语音，请重试" |
| 网络错误 | Snackbar "网络错误，请检查网络连接" |
| 正在流式生成中 | 禁用语音切换和按住说话按钮，仅显示停止按钮 |
| 连接断开 | `enabled` 参数同时禁用两种模式 |
| App 进入后台 | 取消录音，重置为 IDLE |

## 实现顺序

1. `AndroidManifest.xml` + `build.gradle.kts`（权限和依赖）
2. `SpeechRecognizerManager.kt`（独立可测试）
3. `VoiceInputButton.kt` + `VoiceRecordingOverlay`（独立可预览）
4. `ChatBottomBar.kt`（添加模式切换）
5. `ChatScreen.kt`（串联所有状态和逻辑）

## 测试验证

1. 构建：`cd clients/android && ./gradlew assembleDebug`
2. 真机测试（模拟器通常不支持语音识别）：
   - 点击麦克风 → 弹出权限对话框 → 授权 → 进入语音模式
   - 长按「按住说话」→ 浮层出现，麦克风脉冲动画
   - 说话 → 浮层显示实时识别文字
   - 松开 → 文字识别完成，消息自动发送
   - 长按 → 上滑 → 浮层变红显示「松开取消」→ 松开 → 取消，不发送
   - 点击键盘图标 → 回到文字输入模式
