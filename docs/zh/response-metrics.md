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

- `input_tokens`、`cached_input_tokens`：最近一次模型请求的真实用量，不再把工具
  循环中的多次 input 相加。这样它表示当前实际发送给 Provider 的这一份 prompt。
- `output_tokens`：本轮所有模型调用的真实输出用量总和，包括工具循环。
  Gemini 的输出计入已上报的思考 token。
- Anthropic 和 Bedrock 的输入总量会合并未缓存输入、缓存写入与缓存读取。
- `cache_hit_ratio`：最近一次请求的缓存读取 token / 最近一次请求 input token，
  范围为 `[0, 1]`。缓存写入不算命中。
- `output_tokens_per_second`：输出总量 / 各模型调用的流式生成时长之和。
  每次调用从第一个正文、思考或工具生成事件计时，至流结束；不包含首 token
  等待和工具执行。该值受网络缓冲影响。非流式调用无法分离生成时间，因此
  包含此类调用的回复显示速度未提供。
- `elapsed_ms`：Agent 整轮处理耗时，包括准备、模型等待与工具执行；不含客户端
  传输、Agent 开始前的排队，以及后台标题生成或学习。

缺失数据显示 **未提供**。最近一次请求缺少 input 或 cache 字段时，对应字段显示未知；
任一次模型调用缺少 output 时，整轮 output 总量显示未知，避免将部分统计当作完整统计。
不以 prompt 估算值代替真实用量。

官方字段依据见[英文文档](../en/response-metrics.md)中的链接。

## 请求前缀与 Debug

工具注册表按名称排序，系统提示与原生 tools 使用同一顺序。原生模式只在 tools 字段提交完整 schema，XML 模式仍在系统提示中提供工具说明。技能目录只展示有长度上限的首句用途摘要，完整规则必须用 Skill 工具加载。

Debug 的 `estimated_tokens` 在请求发出前是最近一次模型调用的完整 input 估算，包含原生
工具定义；Provider 返回后会用本次真实 input 回写同一张 Debug 卡。`estimated_tool_tokens`
作为工具定义明细单独展示。若已有上一次 Provider 的真实 input，下一次请求前的估算会以
它为基准，再按 prompt 内容变化量校准。消息快照去掉重复的内部 `stable_prefix` 字段。
估算不用于计算命中率；回复底部的 input/cache 是最近一次请求，output 和输出速度仍覆盖
整轮工具循环。
