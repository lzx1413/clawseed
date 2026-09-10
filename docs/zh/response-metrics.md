# Provider 余额与回复统计

Android 的 LLM 配置页面会在进入页面、修改 URL 或密钥后自动查询余额
（600 毫秒防抖），页面打开期间每 60 秒刷新。切换 provider 时立即清除上一个
账户的余额。当前支持 DeepSeek、Moonshot 国内站和 OpenRouter；OpenRouter
使用已购额度减去已消费额度，且需要管理密钥。其他地址显示不支持。
缺少密钥、密钥无效、权限不足和临时查询失败分别提示，未知余额不会显示成零。

`POST /api/provider/balance` 接收 `base_url` 和可选 `api_key`，需要网关认证。
省略密钥时仅复用相同 Base URL 的已保存密钥；显式传入空字符串不会复用。
余额请求只发送到已识别的官方 HTTPS 端点，不返回凭据或上游错误正文。
响应使用 `Cache-Control: no-store`。

在 Android 设置中开启 **Debug 查询**，新回复下方会显示统计。WebSocket 的
`done` 事件及会话历史包含可选 `metrics` 对象；SQLite 单独保存统计，不将其
发送回模型上下文。关闭 debug 后隐藏统计，新回复也不再记录。历史消息没有
记录过的统计不进行补算。

- `input_tokens`、`output_tokens`：本轮所有模型调用的真实用量总和，包括工具
  循环和自动续接。Gemini 的输出计入已上报的思考 token。
- `cached_input_tokens`：缓存读取 token。Anthropic 和 Bedrock 的输入总量会
  合并未缓存输入、缓存写入与缓存读取。
- `cache_hit_ratio`：缓存读取总量 / 输入总量，范围为 `[0, 1]`。缓存写入不算
  命中，不对每次调用的百分比取平均。
- `output_tokens_per_second`：输出总量 / 各模型调用的流式生成时长之和。
  每次调用从第一个正文、思考或工具生成事件计时，至流结束；不包含首 token
  等待和工具执行。该值受网络缓冲影响。非流式调用无法分离生成时间，因此
  包含此类调用的回复显示速度未提供。
- `elapsed_ms`：Agent 整轮处理耗时，包括准备、模型等待与工具执行；不含客户端
  传输、Agent 开始前的排队，以及后台标题生成或学习。

缺失数据显示 **未提供**。若任一次模型调用未提供某项 token 数，本轮对应总量
也显示未知，避免将部分统计当作完整统计。不以 prompt 估算值代替真实用量。

官方字段依据见[英文文档](../en/response-metrics.md)中的链接。

## 请求前缀与 Debug

工具注册表按名称排序，系统提示与原生 tools 使用同一顺序。原生模式只在 tools 字段提交完整 schema，XML 模式仍在系统提示中提供工具说明。技能目录只展示有长度上限的首句用途摘要，完整规则必须用 Skill 工具加载。

Debug 的 `estimated_tokens` 是首次模型调用的消息粗估；新增可选 `tools`（JSON 字符串）和 `estimated_tool_tokens` 单独展示原生工具定义和粗估。消息快照去掉重复的内部 `stable_prefix` 字段。粗估不等于供应商 token 用量，不用于计算命中率；回复底部 metrics 仍是整轮所有调用的合计。
