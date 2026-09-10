# Android 对话文件附件

点击输入框的附件按钮展开两层面板：上层按新到旧横向显示最近图片，可勾选后添加；下层是「拍照」「相册」「文件」三个图标按钮。最近图片需要照片访问授权，支持 Android 14 的部分照片授权；未授权时仍可通过系统相册选择图片。相册与文件入口支持多选，拍照结果加入当前会话草稿。文件卡片显示导入、解析、可发送或失败状态，支持移除和重试。可以只发送文件，也可以与图片及问题一起发送。图片沿用原有协议及视觉模型要求；纯文件内容作为用户资料提供给文本模型。

## 格式与限额

| 格式 | 解析与读取单位 | 限制 |
| --- | --- | --- |
| MD / TXT | 保留文本；按 Unicode 码点区间 | UTF-8（可带 BOM）、带 BOM 的 UTF-16 LE/BE；其他编码或二进制明确报错 |
| CSV | 按完整记录；第 0 条包含表头 | 支持双引号、转义引号、逗号和字段内换行，不按物理行分段 |
| PDF | 文字提取；按页 | 最多 300 页；无文字扫描件提示 OCR；加密、损坏明确报错 |
| DOCX | 段落、标题、列表、制表符分隔的表格；按 Unicode 码点 | 不还原精确排版或嵌入图片；旧 DOC 须先另存为 DOCX |

首版范围已确认不包含扫描 PDF OCR 与旧 DOC 解析。

每条消息最多 4 个文件，单文件最多 20 MiB，文件合计最多 50 MiB；图片仍单独最多 4 张。解析后文字最多 200 万 UTF-16 单元；CSV 单条记录、PDF 单页最多 65536 UTF-16 单元，超限明确失败。DOCX 最多 2000 个 ZIP 条目，主 XML 解压后最多 8 MiB，拒绝 DTD 与实体声明。

单文件自动内容预算 8000 字符，整条消息合计 16000 字符（Android 按 UTF-16 保守计量）。长文件预览最多 1000 字符。CSV 和 PDF 预览只包含完整记录或完整页；首个单位超预算时只提供元数据与从 0 开始的读取方式。分段结果包含明确的 `next` 与 `eof`，不以预览代表全文。

## 持久化与客户端在线要求

原文件和解析缓存保存在 App 私有 `filesDir/chat-file-attachments`。文件名只作显示用途，文件路径只由随机附件 ID 构建。原子写入的清单按网关地址与真实会话 ID 的摘要隔离草稿，进程重启后中断导入显示可重试错误，发送失败保留草稿及文字。发送前持久化引用关系；已引用文件移除草稿后仍保留，避免删除历史会话依赖的副本。未引用副本目前也保守保留，不自动做磁盘回收。

网关只持久化正式文件元数据和有限内容，不依赖手机文件路径。后续追问可复用历史内容；进一步读取需要持有原文件的 Android App 与对应会话连接在线。换设备或客户端离线时无法读取未注入的内容。App 重连会重新注册工具；工具调用依据绑定的真实连接会话检查附件授权，不接受模型提供的会话 ID 或任意路径。

## SDK / 网关协议

`/api/status` 与 WebSocket `session_start` 增加 `file_attachments_supported`。缺少字段视为不支持，App 提示升级，SDK 阻止向旧网关发送文件。现有 `attachments` 图片数组保持兼容。文件放入独立 `files` 数组，支持空 `content`：

```json
{"type":"message","content":"","files":[{"id":"file_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","name":"notes.md","mime_type":"text/markdown","size_bytes":5,"format":"md","status":"ready","unit":"character","total":5,"excerpt":{"attachment_id":"file_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","unit":"character","start":0,"next":5,"total":5,"eof":true,"content":"hello"}}]}
```

SQLite 使用新增 `messages.files_json` 列保留元数据，迁移不删除旧会话。历史 API 返回 `files`，App 恢复文件卡片。Agent 在构造 provider 输入时将资料注入 **user** 消息，不修改原始问题，不提升至 system 指令；重生成及重启恢复保留附件关系。

App 注册 `attachment_read`，参数为 `attachment_id`、`start`、`count`，不接受其他属性。索引从 0 开始，结束位置不包含在范围内。文本和 DOCX 的位置按 Unicode 码点，CSV 按记录（含表头），PDF 按页。

```json
{"attachment_id":"file_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","start":0,"count":1000}
```

返回 `attachment_id`、`unit`、`start`、`next`、`total`、`eof`、`content`。`count` 为 1..16000；每次通常返回最多 16000 字符。单条 CSV 记录或单页可独立返回至 65536 字符加定位标签，不拆断记录。使用 `next` 继续读取，`eof=true` 表示结束。错误以工具失败返回可理解说明，包括越权、缺失文件、解析失败、无效区间。

## 依赖与验证

PDF 使用 [PdfBox-Android 2.0.27.0](https://github.com/TomRoush/PdfBox-Android)，Apache-2.0。初始化资源加载器后提取文字，无新增 native ABI 依赖；未引入可选 JPX 图像解码库。DOCX 使用平台 ZIP/SAX，CSV 为严格引号状态机。release 当前沿用项目关闭混淆的设置，不能声称已验证启用 R8 的产物。

单元测试覆盖编码、Unicode 边界、CSV 跨行引号、完整记录分段、DOCX 结构与实体拒绝、协议兼容和持久化。release 设备测试使用隔离缓存目录，不触及用户数据：

```bash
cargo test -p clawseed-api -p clawseed-agent -p clawseed-gateway --lib
cd clients/android
./gradlew -PattachmentTestBuildType=release :app:testReleaseUnitTest :app:assembleReleaseAndroidTest :sdk:core:test :sdk:android:testDebugUnitTest
```

原 App 与测试 APK 必须先验证签名后使用 `adb install -r`；测试使用 `adb shell am instrument -w dev.clawseed.demo.test/androidx.test.runner.AndroidJUnitRunner`。不要使用可能自动卸载目标 App 的测试安装任务，不卸载、不清数据。实际安装与端到端验收结果另见工作目录交付记录。

图片导入先保存本地原图并显示预览与“处理中”，再在后台编码和上传；不等待网关状态请求才显示草稿。JPEG 照片使用 JPEG 编码，PNG 截图保持无损编码。处理中断后保留本地原图供重试，上传失败可重新上传。

## 从其他 App 分享

在系统相册或文件管理器中选择图片、文件，点击分享并选择「爪籽」。支持单项、多项与图片/文件混合分享；每次分享会打开独立的新会话，附件加入草稿，附带文字填入输入框，由用户点击发送。已有会话草稿不会被覆盖。

分享入口读取 EXTRA_STREAM 与 ClipData，并去重相同 URI。临时授权内容先复制到私有目录，页面恢复使用同一次分享的标识和稳定附件 ID，避免重复导入。支持图片、PDF、MD/TXT、CSV 和 DOCX；无法读取、损坏或不支持的文件会提示错误。照片访问权限不是接收单次分享的前提，发送方需提供该内容的读取授权。

每次最多 4 张图片和 4 个文件；图片原图最多 50 MiB，单文件最多 20 MiB，文件合计最多 50 MiB，整个分享合计最多 100 MiB。图片处理后仍受 5 MiB 上传限制。连接尚未就绪时分享副本会保留，连接成功后再导入草稿，不会自动调用模型。
