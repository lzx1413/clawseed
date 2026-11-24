# 多媒体展示

当用户要求展示、分析或处理图片、音频、视频、缩略图或媒体链接时使用这个 Skill。

当用户要求展示当前回合上传的图片时，调用 `media_present`。省略
`attachment_ids` 可以展示当前回合的全部图片。这个工具返回结构化媒体，客户端会通过
当前会话的附件接口读取图片。

## 当前回合的图片

用户上传的图片已经由 Gateway 验证并解析。需要通过 Linux 插件处理图片时，插件的
`execute_tool` 请求会收到：

```json
{
  "params": {
    "attachments": [
      {
        "id": "att_...",
        "path": "/plugin/.clawseed-input/images-.../att_...",
        "mime_type": "image/png",
        "size_bytes": 12345,
        "width": 800,
        "height": 600
      }
    ]
  }
}
```

直接读取 `params.attachments[*].path`。当前回合已经提供图片时，不要使用文件搜索工具在
workspace 中寻找同一张图片，也不要要求用户再次提供宿主文件路径。这个路径只在本次插件
调用期间有效，插件不得保存或返回宿主绝对路径。

## 返回媒体

插件或工具需要同时返回文本和媒体时，在 CETP `execute_tool` 的 `data` 中使用
`output` 和 `presentation`：

```json
{
  "output": "这是图片中的天气图。",
  "presentation": {
    "version": 1,
    "blocks": [
      {
        "type": "image",
        "media": {
          "url": "https://cdn.example.com/weather.png",
          "mime_type": "image/png"
        },
        "alt": "天气图"
      }
    ]
  }
}
```

规则如下：

- `output` 用简短文字说明结果，供 Agent 继续推理。
- `presentation` 只放客户端需要渲染的图片、音频、视频或链接。
- 远程媒体 URL 必须使用 HTTPS。受宿主管理的媒体可以使用 `asset_id`。
- 媒体 block 只包含与用户请求相关的内容，避免重复展示候选媒体。
- 用户只要求一张图片时返回一个 `image` block。用户要求比较时再返回多个 block。
- 图表、指标和多图切换使用 `visualize-dashboard` Skill。
