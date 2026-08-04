---
title: 九财花 Android App 联动
description: 将 ClawSeed 与九财花 Android 投资管理 App 连接，通过 CETP 调用持仓、行情、资讯、交易与收益分析工具。
---

# 九财花 Android App 联动

[九财花](https://github.com/lzx1413/jiucaihua-android)是一款个人投资管理 App，也是
ClawSeed External Tool Protocol（CETP）的真实 Provider。两个 App 安装在同一台设备后，
ClawSeed 会发现九财花的 Provider，并将其工具注册到当前 Agent 会话。

## 工作方式

```text
ClawSeed Android App
  -> 发现 com.clawseed.action.TOOL_PROVIDER
  -> 通过 ContentProvider 读取九财花工具定义
  -> 注册 jiucaihua__get_portfolio_analysis 等带命名空间的工具
  -> 在九财花进程中执行选定工具
  -> 将结构化结果返回 ClawSeed Agent 循环
```

投资数据和业务逻辑仍由九财花管理。ClawSeed 负责对话、模型选择、分身、记忆、任务规划和
多轮工具调用。

## 可用能力

该联动提供 19 个带命名空间的工具，主要包括：

- 投资组合与单标的持仓分析
- K 线数据与技术指标快照
- 市场资讯与个股资讯
- 市场指数、交易状态、资金流向和证券搜索
- 自选列表、价格预警、交易流水、已实现收益和组合表现
- 目标价或涨跌幅假设推演

完整工具定义和参数参见
[九财花 Tool Provider 文档](https://github.com/lzx1413/jiucaihua-android/blob/main/docs/tool-provider.md)。

## 安装与连接

1. 安装较新的 [ClawSeed Android 版本](https://github.com/lzx1413/clawseed/releases/latest)。
2. 从 [GitHub 仓库](https://github.com/lzx1413/jiucaihua-android)构建并安装九财花。
3. 至少打开一次九财花，确保本地投资数据与 Provider 已就绪。
4. 在 ClawSeed 中新建会话。会话连接时会重新扫描 CETP Provider，九财花工具以
   `jiucaihua__` 前缀出现在工具注册表中。

## 示例提问

```text
分析我当前的投资组合，总结最大的三个集中度风险。

结合近期 K 线指标和相关资讯，分析我持仓金额最大的标的。

计算最近 90 天的已实现收益和现金流，并解释结果。
```

这些请求可以连续触发多个九财花工具。最终回复由当前 ClawSeed 分身生成，并可结合用户画像、
记忆和语音播报设置。

## 只读核心与副作用边界

CETP v1 定义的是只读互操作核心。九财花的持仓、行情、资讯、自选、交易查询和分析工具符合
这一约束。九财花当前还暴露了 `jiucaihua__create_alert` 和
`jiucaihua__delete_alert`；它们会修改应用状态，应视为有副作用的扩展，而不是只读 CETP v1
操作。

Provider 使用显式的外部工具白名单，并要求 Android 权限
`com.clawseed.permission.ACCESS_TOOLS`。由于该权限的保护级别为 `normal`，外部白名单和
Provider 自身的授权策略仍是重要安全边界。

## 相关文档

- [CETP v1 协议](../external-tool-protocol.md)
- [CETP Provider 接入教程](../cetp-provider-tutorial.md)
- [Android App 架构](../android-demo.md)
