# ClawSeed Android UI 局部与风格优化方案

**版本：** 0.5  
**状态：** 开发评审稿  
**目标平台：** Android / Jetpack Compose  
**范围：** 现有页面和交互中的视觉、局部组件、动效、性能与可访问性优化

## 1. 目标和边界

ClawSeed 的视觉目标是“安静、可靠、可控制的个人 Agent 工作界面”。整体使用雾蓝、深色工作台、低饱和强调色、细边框和有限玻璃材质，状态始终同时使用图标、文字和颜色表达。

本方案保留现有导航、页面位置、Persona、Memory、Skill、Tool、会话数据结构、定时任务流程和 Gateway 协议。只改主题、表面、间距、圆角、局部卡片、输入框、时间线节点、指标弹层、Ask User 卡片、定时任务卡片和动效。

本期不新增底部导航、工作台、设置页、任务中心或新的业务流程，不修改 Agent、Provider、Memory、Tool 和定时任务逻辑，也不要求 Gateway 增加字段。

## 2. 现状和数据依赖

当前 Android 客户端已经使用 Material 3 主题，并设置了 `minSdk = 26`。会话模型已有以下可直接用于 UI 的能力：

- `ResponseMetrics`：输入 token、输出 token、缓存输入 token、缓存命中率、输出速度和回合耗时；
- `ChatEntry.AssistantMessage.metrics`：保存已完成回答的用量数据；
- `Thinking`、`ToolInvocations` 和工具结果节点；
- `ChatEvent.AskUserRequested` / `AskUserAcknowledged`：支持局部 Ask User 卡片；
- `ScheduledTask` 和任务状态：支持现有定时任务列表与详情视觉统一。

首期不假设已有首 token 延迟、思考 token、cache-write、上下文窗口、费用或精确 Provider 计费数据。指标组件必须遵守以下显示规则：

| 数据状态 | UI 显示 |
|---|---|
| Provider 返回的精确值 | 正常数值 |
| 客户端估算值 | 数值旁显示“估算” |
| 当前模型没有提供 | “暂无数据” |
| 流式输出尚未结束 | 显示当前值或“统计中” |

UI 可以增加内部展示适配器，但不得在 UI 层伪造精确 token、费用或缓存数据。

### 2.1 DSH Go 参考截图结论

最新浅色 DSH Go 首页截图（`/tmp/dsh-go-reference-light.png`）用于校准比例和层级，不直接复制其品牌图标、吉祥物或文案。截图呈现出以下可复用规律：

- 页面背景是浅灰雾面 Canvas，顶部状态栏与内容区域连续，不使用大面积渐变；
- 菜单、新会话/本机状态、Provider/Model 选择器使用同一组高圆角胶囊，左右边距和高度保持一致；
- 空会话欢迎语位于页面中部，建议操作使用低对比度双列胶囊，避免每个入口变成厚重卡片；
- 底部输入栏是单层白色浮动胶囊，左右端点接近完整半圆，视觉高度约 48–56dp；
- 输入文字和占位提示在胶囊内垂直居中，图标的视觉尺寸约 32dp，触控区域仍至少 48dp；
- 输入栏与页面底部保持约 16dp 安全间距，键盘出现时只随 IME 上移，不改变胶囊比例；
- 阴影和边框都很弱，层级主要由填充、留白和圆角表达。

ClawSeed 采用这些比例和层级关系，保留自己的雾蓝色、Persona 图标、附件入口、Skill 入口和状态语义。空会话建议入口只映射现有功能，例如新会话、Persona、附件、定时任务或工具，不新增业务流程。

## 3. 设计令牌

```kotlin
object ClawSeedColors {
    val Ink = Color(0xFF101820)
    val Mist = Color(0xFFE9F1F5)
    val GlassLight = Color.White.copy(alpha = 0.62f)
    val GlassDark = Color(0xFF18242C).copy(alpha = 0.72f)
    val HairlineLight = Color.White.copy(alpha = 0.34f)
    val HairlineDark = Color.White.copy(alpha = 0.12f)
    val Accent = Color(0xFF76AFC4)
    val Positive = Color(0xFF65B89A)
    val Warning = Color(0xFFE0AA63)
    val Danger = Color(0xFFD87878)
}

object ClawSeedShapes {
    val Small = RoundedCornerShape(10.dp)
    val Medium = RoundedCornerShape(16.dp)
    val Large = RoundedCornerShape(24.dp)
    val Sheet = RoundedCornerShape(topStart = 28.dp, topEnd = 28.dp)
}
```

使用 4dp 间距基线，页面主要边距为 16dp，顶部胶囊和建议入口的间距为 12–16dp。输入栏外边距为水平 12–16dp、底部 12–16dp，单行输入栏高度为 56dp 左右，多行内容再向上扩展。主要操作使用 `Accent`，成功、警告、危险和等待状态分别使用 `Positive`、`Warning`、`Danger` 与图标和文字组合。

这些颜色先映射到 Material 3 的 `ColorScheme`，再由组件读取语义角色。不要在页面中直接散落颜色值。Persona 自定义颜色继续保留，但必须经过现有的可读性处理，不能覆盖主要文字和错误状态颜色。

验收时以最终合成后的背景计算文字对比度，普通正文达到 WCAG AA；透明表面、代码块、错误提示和禁用状态都要单独检查。

## 4. 玻璃表面

只提供三种表面层级：

- `Canvas`：页面背景，不做动态模糊；
- `Surface`：普通内容区域，使用低透明度填充和细边框；
- `ElevatedSurface`：输入框、确认卡和弹层，允许局部模糊和顶部高光。

统一组件接口如下，具体实现放在 `ui/theme` 或 `ui/components`：

```kotlin
enum class GlassLevel { Surface, Elevated }
enum class BlurPolicy { Never, WhenSupported, AlwaysLocal }

@Composable
fun GlassSurface(
    modifier: Modifier = Modifier,
    level: GlassLevel = GlassLevel.Surface,
    shape: Shape = ClawSeedShapes.Medium,
    interactive: Boolean = false,
    blurPolicy: BlurPolicy = BlurPolicy.WhenSupported,
    content: @Composable ColumnScope.() -> Unit,
)
```

绘制顺序固定为背景、可选局部模糊、半透明填充、细边框、顶部高光和内容。`AlwaysLocal` 仍然只能作用于组件边界，禁止对整页或长列表持续 Blur。

Blur 策略如下：

- Android 12 及以上：只对 Elevated Surface 的有限区域启用；
- Android 11 及以下：使用不透明或半透明填充，不依赖 Blur 才能读懂内容；
- 系统启用减少动态效果、设备低电量或性能降级时：关闭动态 Blur 和非必要阴影；
- 同一屏最多两个动态 Blur 区域。

## 5. 组件规范

### 5.1 输入栏

保留输入栏位置和发送流程，改为单层浮动 `ElevatedSurface`。使用 `RoundedCornerShape(percent = 50)` 或等效的胶囊形状，禁止外层 Surface 与内部 TextField 同时绘制边框。浅色模式使用接近白色的填充，深色模式使用深灰蓝填充；边框保持低对比度，获得焦点时只提高外层边框亮度。

输入栏单行高度控制在 56dp 左右，左右端点必须保持半圆。输入文字、占位文字和光标在垂直方向居中，多行文本达到上限后才增加高度。附件、语音、Skill、发送和停止按钮均提供至少 48dp 触控区域，视觉图标约 24–32dp。生成期间发送按钮变为停止按钮，并同时改变图标、文字语义和状态颜色。

有排队消息时，在输入栏上方显示一行紧凑队列条。输入栏获得焦点时只提高边框亮度，不缩放或移动整个组件。Token 按钮只显示上下文占用比例或简写值，点击后打开 `UsagePopover`。

### 5.7 空会话首页

空会话状态沿用现有页面位置和导航，在时间线为空时显示轻量欢迎区域。顶部可将菜单、Persona/本机状态和 Provider/Model 信息统一为同高度胶囊；中部显示简短问候和当前 Persona 相关辅助文案；下方使用两列低对比度建议胶囊。建议项必须调用已有发送或导航动作，点击后进入原有会话流程。

空会话区域不使用持续动画、不引入大插画，也不让建议入口抢过输入栏的主要操作层级。深色模式和浅色模式只改变表面与文字语义色，布局、间距和点击区域保持一致。

### 5.2 UsagePopover

`UsagePopover` 使用独立状态和独立重组范围，不得因为指标更新而重组整条消息列表。展示字段按可用性分组：

1. 回合耗时、首 token 延迟、生成速度；
2. 输入、输出、思考 token；
3. cache-read、cache-write、缓存命中率；
4. 会话累计输入/输出 token；
5. 当前上下文、窗口上限和使用比例；
6. Provider、Model 和可选费用。

当前模型不提供的字段显示“暂无数据”，客户端估算值带“估算”标记。流式期间更新当前回合数据，结束后保留最终值。弹层需要支持点击外部关闭、返回键关闭、TalkBack 标题和键盘焦点顺序。

### 5.3 对话时间线

保留现有数据顺序，用低对比度纵线连接同一回合。节点类型继续包括用户消息、思考、工具调用、工具结果、Ask User、用户插话和最终回答。

- 思考默认折叠；当前节点和最近失败节点默认展开；
- 工具调用使用轻量执行卡，显示名称、参数摘要、状态、结果预览和操作；
- 长结果、长思考和日志默认显示不超过 4 行摘要；
- 展开后保持在原节点位置，不跳转新页面；
- 已完成节点减少阴影和表面透明度；
- 回合末尾显示紧凑用量摘要，点击后打开 `UsagePopover`；
- `LazyColumn` 使用稳定 `key`，流式内容只更新当前节点。

### 5.4 Ask User 卡片

Ask User 保持原时间线位置，作为局部阻塞卡片。顶部显示“需要你的回答”和等待图标，内容区域根据现有事件渲染单选、多选、确认/拒绝或自由文本控件。

推荐选项使用强调色，危险选项使用警示色和明确动词。提交后卡片变成只读摘要，后续事件继续显示在下方。超时、取消和稍后处理使用次要按钮，后台任务只优化通知和入口样式，不改变任务逻辑。

### 5.5 定时任务卡片

现有任务列表和详情统一使用 `Surface`。卡片显示任务名称、绑定 Persona、重复规则、下次运行时间、最近状态和权限摘要。状态必须同时显示图标、文字和颜色：运行中、已暂停、等待用户、成功、失败。

任务详情中的执行记录复用对话时间线节点样式，继续支持现有的启用、暂停、立即运行、编辑和删除操作。危险删除使用具体动词确认，并保留现有撤销或二次确认流程。

### 5.6 通用状态组件

统一封装状态胶囊、确认卡、Bottom Sheet、Snackbar 和错误提示。确认文案使用“允许读取文件”“拒绝执行命令”等具体动词。所有可点击控件支持至少 48dp 触控区域、TalkBack 描述和键盘焦点。

## 6. 动效

| 用途 | 时长 | 曲线 |
|---|---:|---|
| 点击反馈 | 90–120ms | FastOutLinearIn |
| 节点展开/折叠 | 160–220ms | FastOutSlowIn |
| Bottom Sheet | 280–340ms | Spring 或 EaseOut |
| 工具状态变化 | 220–300ms | EaseInOut |
| 指标弹层出现 | 160–220ms | EaseOut |

展开、指标更新、Ask User 提交和工具状态变化使用可中断动画。用户滚动、低电量或系统启用减少动态效果时，关闭非必要动画、Blur 和阴影。动画不得修改业务状态，也不得在每一帧触发列表数据重建。

## 7. Compose 实现约束

- 使用 Material 3 基础控件和无障碍语义，由 ClawSeed Theme 覆盖颜色、形状和表面；
- 将 Markdown、工具卡、Ask User 卡、指标弹层、附件预览和状态组件拆成独立 Composable；
- 使用 `Animatable`、`updateTransition` 或 `AnimatedVisibility`，动画状态与业务状态分离；
- Blur 放在独立图层并限制区域，旧系统使用填充降级；
- 通过内部 `UsagePresentation` 适配现有 `ResponseMetrics`，不在 UI 层扩展 Gateway 协议；
- 为输入框、时间线展开、工具操作、Ask User 提交、UsagePopover 和任务操作添加稳定 `testTag`；
- 流式输出、节点展开和指标刷新不能导致整棵 `LazyColumn` 重组。

## 8. 实施阶段和验收

### Phase 1：主题和基础组件

建立语义色、形状、间距、动效、`GlassSurface`、状态胶囊和通用确认/提示组件。先完成浅色/深色 Canvas、顶部胶囊、空会话欢迎区和单层输入胶囊，再替换一个代表性列表页面。

验收：输入栏单行高度约 56dp、端点为半圆、文字垂直居中；浅色输入栏接近白色、深色输入栏为深灰蓝；Android 11 以下无 Blur 也能正常显示；主要控件达到 48dp；现有主题和 Persona 颜色不影响错误状态。

### Phase 2：对话局部

在 Phase 1 的输入胶囊基础上，优化队列条、时间线节点、工具卡、Ask User 卡和 `UsagePopover`。指标仅消费现有 `ResponseMetrics`，缺失字段按规则显示。

验收：流式输出不重建整条列表；展开后保持原位置；Ask User 提交、取消、超时状态可读；指标弹层独立打开和关闭。

### Phase 3：定时任务和通用状态

替换任务列表、任务详情、执行记录、Snackbar、Bottom Sheet 和错误提示的表面及状态样式，不改变任务流程。

验收：运行中、暂停、等待、成功、失败同时具备图标、文字和颜色；启用、暂停、立即运行、编辑、删除操作保持原行为。

### Phase 4：性能和可访问性

在中端 Android 设备验证长对话滚动、流式输出、键盘弹出、时间线展开、指标弹层、Ask User 输入、任务状态切换和弱网恢复。

验收目标：普通页面滚动接近 60 FPS；流式和展开期间无持续掉帧；同屏动态 Blur 不超过两个；TalkBack、键盘焦点、返回键和触控区域通过检查；减少动态效果模式下无非必要动效。

## 9. 不纳入本期的事项

以下需求只有在数据层已有稳定来源后再单独评估：首 token 延迟、思考 token、cache-write、上下文窗口上限、精确费用、多 Provider 计费换算和完整回合级性能追踪。它们不能因为新增 UI 卡片而直接扩展 Gateway 协议或会话数据结构。

## 10. 评审输出

每个 Phase 需要提供：一组浅色/深色截图、与 DSH Go 参考图并排的输入栏比例截图、Android 11 和 Android 12 以上的表面降级截图、TalkBack 焦点顺序记录、长对话滚动录屏，以及对应的 Compose UI 测试结果。输入栏评审重点检查胶囊端点、单行文字垂直居中、浅色填充、底部安全间距和键盘上移行为。视觉评审只确认颜色、间距、状态层级和动效；业务行为继续使用现有测试覆盖。

## 11. 首页实现细节和已验证规则

### 11.1 顶部信息层级

顶部不使用一个承载所有操作的完整导航块。当前首页使用独立的浮动元素：

- 侧边栏：独立圆形按钮，至少 52dp；
- 会话标题：单独的半圆胶囊；短标题按文字宽度收缩，长标题使用最大宽度和省略号；
- Persona：独立圆形按钮，只显示 Persona 图标；Persona 颜色作为整个按钮底色，图标接近填满按钮；
- 新会话：独立圆形按钮，和侧边栏按钮使用相同尺寸、Surface、边框和阴影；
- 声音控制：不放在首页顶部主操作区。

有 Persona 时，标题胶囊必须为右侧 Persona 和新会话按钮预留间距，不能通过固定最小宽度把短标题撑大。顶部元素之间使用 8–12dp 间距，状态栏通过 `statusBarsPadding()` 处理，内部组件不得重复添加系统 inset。

### 11.2 Surface、透明度和阴影

Material `Surface` 的 `shadowElevation` 和 `Modifier.shadow` 的扩散结果不同。顶部浮动控件和输入栏统一使用 Material `Surface` 绘制阴影，默认 4dp；圆形按钮和标题胶囊使用同一套边框颜色和阴影参数。

半透明只用于需要透出 Canvas 的标题胶囊、输入栏等 Elevated Surface。菜单和新会话按钮使用不透明 Surface，避免后方 backdrop 透过按钮形成内外两种颜色或多边形叠色。Persona 按钮使用单一 Persona 主色，不叠加白色内圆。

### 11.3 Backdrop 过渡和 Blur 边界

普通 Compose `Modifier.blur()` 只会模糊当前 Composable 自身，不能直接模糊后方页面内容。没有独立背景采样层时，不得把它标记为真正的 Backdrop Blur。

当前首页使用以下可控降级：

- 顶部保留低透明度的局部过渡层，禁止形成横跨页面的不透明灰条；
- 输入栏半透明背景只存在于输入栏自身边界内，输入栏上沿之外不再额外绘制扩散层；
- 需要真正背景模糊时，另行实现 Android 12+ 的局部背景采样/RenderEffect，并为 Android 11 以下提供填充降级；
- 任何 Blur 都必须限制在组件边界或明确的短过渡区域，同屏不超过两个动态区域。

### 11.4 输入栏边界

输入栏使用单层 `RoundedCornerShape(percent = 50)` 胶囊。内部文本由自定义 `BasicTextField` 居中布局，避免 Material `TextField` 默认内边距造成文字偏上。输入栏外部不叠加第二个 TextField 边框，也不绘制超过输入框上沿的 backdrop。

验收时分别检查未聚焦、聚焦、键盘打开、输入多行和生成中五种状态：单行高度约 56dp，文字垂直居中，边框聚焦只轻微提亮，停止按钮和发送按钮不改变胶囊端点。

### 11.5 空会话首页

空会话欢迎区使用低对比度双列建议胶囊，建议入口调用现有发送流程。欢迎区不使用持续动画或大面积插画；顶部浮动元素与输入栏保持主要操作层级。建议入口和输入栏都必须保留至少 48dp 触控区域，并提供稳定 `testTag`。

## 12. 当前实现状态

已完成：

- ClawSeed 颜色、形状和 `GlassSurface` 基础令牌；
- 单层浮动输入栏、半圆端点、垂直居中文本和发送/停止状态；
- 独立菜单、标题、Persona 和新会话顶部元素；
- Persona 图标铺满圆形按钮，菜单和新会话按钮统一为纯圆 Surface；
- 空会话欢迎区和双列建议入口；
- 浅色/深色主题映射、状态栏 inset 和 release 设备验证流程。

下一步按以下顺序推进：

1. 为 `ResponseMetrics` 增加独立 `UsagePopover`，不触发时间线整棵列表重组；
2. 统一时间线节点、工具执行卡和 Ask User 卡；
3. 统一定时任务卡片和执行记录节点；
4. 补齐 Android 11 降级、TalkBack、键盘焦点和长对话性能验收。

每次视觉改动必须经过 `compileReleaseKotlin`、`testDebugUnitTest` 或对应测试、`assembleRelease`，并在真实设备上覆盖安装后截图复核。截图必须确认当前前台确实是 ClawSeed，避免把其他应用画面作为 UI 验收依据。
