# 多媒体展示

当用户要求展示、分析或处理图片、音频、视频、缩略图或媒体链接时使用这个 Skill。

多媒体显示由 Android 客户端提供的 `media_present` 工具完成。这个工具只负责把媒体
描述交给客户端，实际读取和渲染由客户端完成。显示当前回合上传的图片时传入
`attachment_ids`，显示插件生成的文件时传入 workspace 根目录下的相对 `path`。

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

插件进程的当前目录是 `/plugin`。这个目录用于插件代码，不能作为 App 媒体目录。
需要返回本地媒体时，插件必须声明 `workspace=write`，把文件写入 `/workspace` 下，
再把同一个文件转换为 workspace 相对路径返回。例如插件 `weather` 应写入
`/workspace/.clawseed/plugins/weather/media/weather.png`，返回
`.clawseed/plugins/weather/media/weather.png`。

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

插件生成本地媒体时，把文件写入 workspace 下的插件目录后，在工具结果的
`presentation` 中直接返回相对路径：

```json
{
  "type": "image",
  "media": {
    "path": ".clawseed/plugins/weather/data/weather.png",
    "mime_type": "image/png"
  }
}
```

Android 客户端会按 workspace 根目录解析这个相对路径并展示文件。已经有
`presentation` 时不要再次调用 `media_present`。
插件不要返回宿主绝对路径，也不要生成 `asset_id`。

规则如下：

- `output` 用简短文字说明结果，供 Agent 继续推理。
- `presentation` 只放客户端需要渲染的图片、音频、视频或链接。
- 远程媒体 URL 必须使用 HTTPS。本地插件媒体使用相对 `path`，由 Android 客户端读取并展示。
- 媒体 block 只包含与用户请求相关的内容，避免重复展示候选媒体。
- 用户只要求一张图片时返回一个 `image` block。用户要求比较时再返回多个 block。
- 图表、指标和多图切换使用 `visualize-dashboard` Skill。
