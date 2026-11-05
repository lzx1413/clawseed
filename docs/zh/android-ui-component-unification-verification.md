# Android 基础控件统一验证记录

日期：2026-11-05。应用版本：2.1.0（versionCode 20）。验证环境：Linux / WSL2，Android compileSdk 36，项目现有 Compose BOM。

## 自动验证

- `:app:compileReleaseKotlin`：通过。
- `:app:testDebugUnitTest`：86 个测试通过，0 失败、0 跳过；其中 `AppColorsTest` 7 个测试覆盖浅色、深色、OLED、半透明材质合成、输入边界与错误信息对比度。
- `:app:assembleRelease`：通过，产物位于 `clients/android/app/build/outputs/apk/release/`。
- `:app:compileDebugAndroidTestKotlin`：通过。新增 `UnifiedComponentsTest` 的设备运行尚未执行。
- `./tools/ci-local.sh`：格式、clippy、功能组合检查、构建和 1512 个 Rust 测试通过。首次沙箱运行不允许网关测试监听端口，在沙箱外重跑后全部通过。
- `git diff --check`：通过。

构建继续报告已有的 Compose API 弃用提示；未新增 Rust、SDK 协议或 Gateway API 改动。

## 可复核入口

- `ui/components/ComponentPreviews.kt`：浅色、深色、320dp 窄屏及 2 倍字号预览，包含设置行、开关、输入错误、分段选择、禁用和加载状态。
- `ui/components/UnifiedComponentsTest.kt`（androidTest）：整行开关只触发一次、单选互斥、多选切换、加载期间阻止再次提交并保持宽度，以及只读输入的错误语义。
- `ui/theme/AppColorsTest.kt`（test）：实际材质填充的透明合成与对比度。

所有路径均相对 `clients/android/app/src/{main,androidTest,test}/kotlin/dev/clawseed/demo/`。

## 首轮设备限制（后续验证见下文）

首轮执行时，`adb devices` 没有设备；SDK 环境未安装模拟器和 AVD。本次未生成或声称已复核页面截图，也未执行 instrumentation。

连接测试设备后运行：

```bash
cd clients/android
./gradlew -PuiComponentTests=true :app:connectedDebugAndroidTest \
  -Pandroid.testInstrumentationRunnerArguments.class=dev.clawseed.demo.ui.components.UnifiedComponentsTest
```

仍需按实施方案第 8.2 节覆盖：各迁移页面浅色/深色截图、OLED、320dp/横屏、字体 1.0/1.3/2.0、中英文长文案、TalkBack 和键盘焦点、IME、多行输入、发送/停止/附件/会话切换、减少动画、Android 11 及以下与 12 以上，以及流式输出和滚动性能。

截图应记录设备、主题、页面和构建版本，并确认前台包名为 `dev.clawseed.demo`。这些项目属于待验收项，不能由编译通过或组件预览替代。


## 第二轮视觉调整与真机复核（2026-11-05 20:27–20:30）

设备：Xiaomi 23127PN0CC（houji），Android 16，1200 × 2670。包名 `dev.clawseed.demo`，版本 2.1.0（20），覆盖安装时间 20:26:45。

按用户确认追加了可见的视觉调整：顶部浮动按钮合并为统一圆角操作栏；新建按钮采用主色；建议项改为带图标的圆角卡片；输入栏统一不透明 Chrome 填充、28dp 圆角、聚焦主色描边，阴影从 4dp 调整为 2dp。设置分组增加容器、图标槽及展开态填充，分段选中项使用主色。

本轮 `:app:compileReleaseKotlin :app:testDebugUnitTest :app:assembleRelease` 通过，86 个单元测试通过；ADB 覆盖安装成功。人工检查首页、设置页及展开状态的浅色和 OLED 深色表现，检查输入框聚焦、键盘避让、抽屉、设置导航和新建分身弹层。未见上述截图中的文字遮挡或输入栏与键盘重叠；检查结束后恢复原来的浅色主题和首页，OLED 偏好保持原值。

- [首页前后对照](images/android-controls/home-comparison.png)
- [设置页前后对照](images/android-controls/settings-comparison.png)
- [设置展开态](images/android-controls/settings-expanded.png)
- [OLED 首页](images/android-controls/home-oled.png)
- [OLED 设置](images/android-controls/settings-oled.png)
- [输入法与聚焦输入栏](images/android-controls/ime.png)

上述复核不代表完成全部设备矩阵。本轮未执行 instrumentation、TalkBack、大字号/横屏/窄屏、普通深色（OLED 关闭）、其他 Android 版本和长对话性能测试，也没有发送测试消息或修改业务配置。


## 第三轮：列表与抽屉视觉统一（2026-11-05）

版本仍为 2.1.0（20），本轮正式 APK 安装时间为 20:50:36，设备沿用上述 Android 16 手机。

- 分身列表：头像和说明使用完整内容区，聊天、编辑、复制和删除移到独立操作区；操作区支持换行，保留分身配色和原回调。
- 定时任务：使用共享卡片，长标题允许换行，启用开关与下方操作区分开，运行、编辑、删除和详情操作保持原行为。本轮未触发任务执行。
- 用户资料：资料内容与操作区分开，选中项使用整卡高亮；真机当前没有画像条目，只完成筛选项和空状态复核，未插入测试数据，条目卡选中态仍待实际验收。
- 抽屉：使用可随内容增高的共享导航行，当前会话显示主色描边与浅色填充；已在真机通过切换现有会话复核。

`:app:compileReleaseKotlin :app:testDebugUnitTest :app:assembleRelease` 通过，86 个单元测试通过；`git diff --check` 通过。正式版已覆盖安装，原数据保留。

新增 7 项 Compose 测试已编译，但设备执行停在首项用例启动阶段，没有通过结果。已补充独立测试包开关 `-PuiComponentTests=true`、动态测试 Provider authority，并编译更新后的 AndroidX 测试依赖；按用户要求停止兼容性排查，没有继续运行。临时测试应用及运行器均已卸载，正式应用保留。

视觉记录：

- [分身列表前后对照](images/android-controls/persona-comparison.png)
- [默认定时任务卡前后对照（局部）](images/android-controls/tasks-comparison.png)
- [抽屉选中态（局部）](images/android-controls/drawer-selection.png)
- [资料页空状态](images/android-controls/profile-empty.png)

本轮截图均为浅色；上述页面的深色、大字号、横屏和完整无障碍矩阵仍待验收。

## 第四轮：表单与分身编辑页（2026-11-05）

版本 2.1.0（20），最终覆盖安装时间 21:16:35，沿用上述 Android 16 手机。用户数据保留。

- 新增 `ClawSeedFormDialog`：任务和资料编辑表单共用宽度、圆角、标题、分隔线与底部按钮区；内容区域独立滚动，并避让系统栏和输入法。保留原字段校验和资料保存期间的关闭保护。
- 任务表单增加字段间距，重复模式和自定义星期选项支持换行；确认操作采用主按钮。
- 新增 `ClawSeedExpandableSection`，设置页与分身编辑页共用分组标题、展开色和状态语义；分身的 Soul、工具权限、技能内容使用统一容器。记忆、工具和技能开关接入共享设置行；编辑页增加键盘避让，底部操作支持换行。
- 真机检查任务表单、资料新增表单、分身 Soul 展开/收起及底部按钮。发现全屏弹窗的左右边距未响应外部点击，已将关闭点击区域移到完整窗口并重新安装验证：内部标题点击保持打开，左右边缘点击正常关闭。
- 真机确认任务表单聚焦时取消/确认按钮在键盘上方；选择“指定星期”后，键盘打开状态仍能滚动到周一至周日并取消表单。未保存任务、资料或分身，没有发送消息或执行任务。

最终 `:app:compileReleaseKotlin :app:testDebugUnitTest :app:assembleRelease` 通过，86 个单元测试通过（0 失败、0 错误、0 跳过），`git diff --check` 通过。未恢复 instrumentation 排查；本轮新增布局的深色、横屏、大字号、TalkBack 和保存中状态仍未完成真机验收。

视觉记录（均为浅色；任务截图只保留表单和键盘，背景任务内容已遮蔽）：

- [任务表单前后对照](images/android-controls/task-form-comparison.png)
- [任务表单与键盘](images/android-controls/task-form-ime.png)
- [键盘打开时滚动到星期选项](images/android-controls/task-form-scroll.png)
- [资料新增表单](images/android-controls/profile-form.png)
- [分身 Soul 展开状态](images/android-controls/persona-editor-expanded.png)
- [分身编辑页底部操作区](images/android-controls/persona-editor-actions.png)

## 第五轮：数据管理页补齐（2026-11-05）

版本 2.1.0（20），最终覆盖安装时间 21:26:09，设备沿用上述 Android 16 手机。

- WebDAV 地址、目录、用户名、两个密码，以及自动备份时间/保留数量，共 8 个字段迁移至 `ClawSeedTextField`，保留原值、过滤逻辑和密码遮蔽。
- 排除敏感数据、启用自动备份、仅 Wi-Fi、记住 WebDAV 密码四处开关迁移至 `SettingsSwitchRow`。
- 新增 `SettingsCheckboxRow`，导入/导出分类由整行处理切换，说明文字和勾选框分列，内部勾选框不重复处理点击；组件预览同步补充。
- 从展开分组提取 `ClawSeedSectionHeader`，数据管理的五个子标题复用图标、描边和展开态配色。
- 导入/导出目标、备份目的地/频率/星期、导入策略和刷新操作支持换行。备份文件列表补齐互斥选择组、单一选择回调与 48dp 最小行高。

`:app:compileReleaseKotlin :app:testDebugUnitTest :app:assembleRelease` 通过；86 个单元测试通过，0 失败、0 错误、0 跳过；`git diff --check` 通过。ADB 覆盖安装成功。

真机浅色检查：子分组展开、导出分类与说明、排除敏感数据开关布局、WebDAV 字段。点击“会话”的说明文字后，勾选框选中且导出按钮启用；第二次点击恢复未选中。两个密码字段继续暴露为密码输入。未点击导入、导出、保存连接、测试连接、备份或删除，未修改已保存配置；检查结束后返回首页。

- [数据分组前后对照](images/android-controls/data-groups-comparison.png)
- [导出区前后对照](images/android-controls/data-export-comparison.png)
- [分类整行点击后的选中态](images/android-controls/data-category-selected.png)
- [WebDAV 字段（字段值已遮蔽，局部）](images/android-controls/data-webdav-fields.png)

本轮未执行 instrumentation，也未验证实际数据传输、备份列表有数据时的交互、自动备份展开内容、深色、大字号、横屏和 TalkBack；这些不计为通过项。

## 第六轮：聊天弹层（2026-11-05）

版本 2.1.0（20），最终覆盖安装时间 21:38:04，沿用上述 Android 16 手机。

- 新增 `ClawSeedSheetHeader`，分身选择和附件菜单共用标题、标题语义及关闭按钮。
- 分身选择使用共享卡片，头像前置，选中项主色描边，摘要允许两行；列表根据可用高度滚动，为底部操作留出空间；底部按钮使用双向间距并支持换行。
- 附件菜单内容可滚动，操作入口使用统一圆角、填充和描边；窄屏或字号达到 1.3 倍时切为纵向操作行，取消固定高度裁切；提示和操作文案补齐中英文资源。图片选择上限、权限与附件回调保留。
- 会话重命名接入 `ClawSeedFormDialog`，补充字段标签、满宽输入框和主确认按钮；未提交名称使用 `rememberSaveable`。分组标题允许多行。当前分组由分身自动生成，不存在独立分组编辑入口，本轮没有新增该业务功能。

最终 `:app:compileReleaseKotlin :app:testDebugUnitTest :app:assembleRelease` 通过；86 个单元测试通过，0 失败、0 错误、0 跳过；`git diff --check` 通过；正式 APK 已覆盖安装。

真机浅色检查：分身单选时旧项取消、新项高亮；关闭/取消能退出弹层；附件标题及三项入口完整显示；重命名输入聚焦时确认和取消仍在键盘上方。系统字号临时从 1.0 调为 2.0：分身列表可滚动到最后一项并选择，底部按钮仍可见；附件入口自动纵向排列。完成后已恢复并读取确认 `font_scale=1.0`，返回首页。

未发送消息、添加附件、保存重命名或执行分组删除。本轮没有运行 instrumentation；中英文长文案、独立 320dp 宽度、横屏、深色、TalkBack、重命名恢复和实际附件获取/发送流程尚未完成设备验收。2 倍字号通过只适用于本轮检查的两个弹层，不代表全应用大字号验收通过。

视觉记录（附件缩略图及重命名弹窗背景已遮蔽）：

- [分身选择前后对照](images/android-controls/chat-picker-comparison.png)
- [附件菜单前后对照](images/android-controls/chat-attachments-comparison.png)
- [重命名弹窗前后对照](images/android-controls/chat-rename-comparison.png)
- [重命名键盘避让](images/android-controls/chat-rename-ime.png)
- [分身选择 2 倍字号](images/android-controls/chat-picker-font2.png)
- [2 倍字号滚动至最后一项](images/android-controls/chat-picker-font2-scrolled.png)
- [附件菜单 2 倍字号](images/android-controls/chat-attachments-font2-redacted.png)

## 第七轮：详情、提示与剩余布局适配（2026-11-05）

最终版本 2.1.0（20），覆盖安装时间 22:03:48，设备沿用上述 Android 16 手机。

实现范围：

- 分身详情身份卡、功能标签及工具分组统一共享组件；取消标签 26dp 固定高度，长名称/标签可换行；头像选择按钮移至说明下方。列表底部保留浮动按钮空间。
- 定时任务详情使用统一可滚动弹窗，执行状态以成功/错误语义色表达；任务卡的重复规则、执行时间和操作区支持换行。
- 资料页筛选和批量操作移入同一滚动列表，避免短屏内容区被筛选项占满；资料内容区提供单一勾选回调；记忆卡片接入统一容器。
- 剩余确认、错误、导入结果和关于弹窗统一容器，保留原关闭保护、保存和危险操作回调。弹窗标题与底部操作在高度不足时也可滚动。
- 新增 `ClawSeedTopAppBar`，设置、分身、任务、资料页面标题允许随内容增高；首页建议在受限宽度/字号下单列显示；设置语音说明、技能标签、模型状态及询问用户操作区补齐换行和选择语义。清理未使用的导入。

验证：最终 `:app:compileReleaseKotlin :app:testDebugUnitTest :app:assembleRelease` 通过，86 个单元测试通过（0 失败、0 错误、0 跳过），`git diff --check` 通过。最初一次编译发现弹窗尺寸接收者作用域问题，修复后重新构建通过。

真机复核正常字号下的分身详情浅色/OLED、任务失败详情、普通深色设置/首页、资料页深色及横屏空状态、横屏资料新增弹窗。当前资料为空，因此没有声称资料条目及批量动作已通过真机验证；未触发任务执行、资料变更、删除或保存测试内容。

用户明确要求停止大字号测试后，立即停止，并恢复 `font_scale=1.0` 与原始 density 480。此前只做了部分 320dp/2 倍字号观察，不作为完整矩阵通过项。后续仅使用正常字号检查；结束时也恢复浅色主题、原 OLED 偏好（开启）、自动旋转（1）及 user_rotation（0），返回首页。没有改变物理屏幕尺寸。

截图：

- [分身详情前后对照](images/android-controls/detail-persona-comparison.png)
- [任务详情前后对照（背景已遮蔽）](images/android-controls/detail-task-comparison.png)
- [OLED 分身详情](images/android-controls/detail-persona-oled.png)
- [普通深色设置](images/android-controls/adapt-settings-dark.png)
- [普通深色首页](images/android-controls/adapt-home-dark.png)
- [普通深色资料空状态](images/android-controls/adapt-profile-dark.png)
- [正常字号横屏资料页](images/android-controls/adapt-profile-landscape.png)
- [正常字号横屏资料弹窗](images/android-controls/adapt-profile-dialog-landscape.png)

限制：没有运行 instrumentation、TalkBack、其他 Android 版本、长对话性能或全部页面×主题×语言组合。大字号检查按用户要求停止，不再列为继续执行的任务；现有适配代码保留。其余未覆盖项仍须与“代码已实施”区分，不能表述为全部设备验收通过。

## Git 提交前检查（2026-11-05）

按仓库约定重新运行 `./tools/ci-local.sh`，fmt、clippy、全部功能/无默认功能检查、CI profile 构建及 1512 个 Rust 测试全部通过（0 跳过）。Android 最终构建与 86 个单元测试结果沿用第七轮；没有重新运行设备测试或大字号测试。

## 第八轮：首页独立悬浮控件（2026-11-05）

按用户最新要求，移除聊天顶部整条容器，菜单、Persona、新建入口改为独立圆形按钮，会话标题改为居中胶囊。新增 `ClawSeedFloatingSurface` 与 `ClawSeedFloatingIconButton`，共用 48dp 高度、Chrome 填充/描边及 4dp 阴影；三个按钮宽度均为 48dp。Persona 保留头像和颜色，标题单行省略，原入口回调保留。

验证：`:app:compileReleaseKotlin :app:testDebugUnitTest :app:assembleRelease` 通过，86 个单元测试通过（0 失败、0 错误、0 跳过），`git diff --check` 通过。版本 2.1.0（20）已覆盖安装到上述 Android 16 手机。

正常字号、浅色真机截图确认控件独立悬浮，三个按钮边界均为 144×144 像素（48dp，density 480）。菜单打开侧边栏、Persona 打开默认分身详情、新建按钮打开分身选择弹层，取消返回首页。没有发送消息或确认新建会话。字号保持 1.0，未执行大字号测试或 instrumentation；本轮没有重新运行 Rust CI。

- [顶部前后对照（仅裁取顶部；安装前为已有会话，安装后为新会话）](images/android-controls/home-floating-header-comparison.png)
- [更新后首页完整截图](images/android-controls/home-floating-header.png)

后续按用户要求，将输入框阴影从 2dp 调整为 4dp，并与顶部控件共用 `ClawSeedElevation.FloatingControl`，防止两处样式值再次偏离。此补充调整只涉及阴影；`:app:assembleRelease` 和 `git diff --check` 通过，已覆盖安装并在正常字号下截图复核，上方首页完整截图已更新。没有重复运行单元测试或大字号测试。

用户随后指出输入框阴影仍偏重于下沿。最终改为 `ClawSeedShadows.FloatingControl`：顶部按钮、标题和输入框共用零偏移、4dp 模糊半径、22% 黑色的 `dropShadow`，同时取消这些控件的 elevation 阴影，避免两种阴影叠加。`dropShadow` 支持直接指定半径、颜色与偏移，参见 [Compose 阴影文档](https://developer.android.com/develop/ui/compose/graphics/draw/shadows)。这替代了上面的 elevation 实现，4dp 现在表示模糊半径。

最终修正重新通过 `:app:assembleRelease` 和 `git diff --check`，已覆盖安装。正常字号浅色截图确认输入框上下及两侧均有柔和阴影，下沿不再明显偏重；首页完整截图已更新，另存 [输入框阴影前后对照](images/android-controls/input-centered-shadow-comparison.png)。未重复运行单元测试或大字号测试。

## 第九轮：Ask User 细节对齐（2026-11-05）

- 提问容器改用 `GlassSurface` 的 Chrome 表面、共享卡片圆角及 `ClawSeedShadows.FloatingControl` 四周阴影，与首页悬浮控件一致；边距、内容间距改用共享 token，标题使用 `titleMedium` 并提供标题语义。
- 新增 `ClawSeedChoiceRow`，组合共享卡片和 `SettingsRow`。单选、多选均使用整行选择语义，右侧指示器不重复提供点击/朗读目标；选中项统一主题色填充和描边，禁用态沿用共享组件样式，说明可换行。
- 文本提问使用统一输入框并补齐中英文标签。提交状态使用共享按钮内置 loading，避免旁边额外的进度条挤占布局；取消/拒绝与提交/确认回调、答案格式及提交期间禁用规则保留。
- 增加正常字号浅色/深色预览样例，覆盖单选、多选、文本、确认、提交中及禁用态。这里只确认预览代码编译通过，没有声称已渲染或完成交互验收。

`:app:compileReleaseKotlin :app:testDebugUnitTest :app:assembleRelease` 通过；86 个现有单元测试通过（0 失败、0 错误、0 跳过），`git diff --check` 通过。没有运行 instrumentation 或大字号测试，也没有自动发送消息来触发真实 Ask User；实际提问场景截图和选择/提交交互尚未完成真机验收。

APK 已覆盖安装，启动后首页正常显示。最初两次非流式安装遇到 ADB 传输中断，改用流式安装后返回 Success。

本次 UI 调整提交前重新执行 `./tools/ci-local.sh`，fmt、clippy、全部功能/无默认功能检查、CI profile 构建及 1512 个 Rust 测试全部通过（0 跳过）。Android 验证沿用本轮构建和 86 个单元测试结果。
