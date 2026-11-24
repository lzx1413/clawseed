---
title: CETP 插件开发复盘
description: 插件创建、验证和启用流程中的实际问题与修复记录。
---

# CETP 插件开发复盘

## 创建路径

插件必须生成到 `<workspace>/.clawseed/plugins/<plugin_id>/`。`plugin_create` 返回值应列出
manifest、入口和 README 的相对路径，使用工具返回的文件清单可以避免把插件误建到 workspace 根目录。

## stdio 协议

ClawSeed 的 CETP stdio binding 不是通用 JSON-RPC：

- 成功响应是 `{"id":"...","status":"success","data":...}`；
- 失败响应是 `{"id":"...","status":"error","error":{...}}`；
- `list_tools` 使用 `parameters`，不是 `inputSchema`；
- `execute_tool` 使用 `params.tool_name` 和 `params.args`；
- `negotiate` 返回 `selected_version` 和 `session_token`；
- stdout 只输出 JSONL，日志写 stderr。

## 验证环境

`plugin_validate` 必须使用和实际运行一致的 Linux runtime。Android 宿主通常没有 `python3`
或 `node`，不能把宿主命令探测结果当作插件可运行性结论。当前实现使用 active tawcroot
runtime，启动失败回退 PRoot；runtime 未就绪时直接返回错误，不回退到宿主执行。

Tawcroot/PRoot 是插件的启动器，Python 和 Node.js 是 rootfs 内的 guest 解释器。当前签名 runtime 的
Python 路径是 `/opt/python/bin/python3`，Node.js 路径是 `/opt/node/bin/node`。宿主侧不能用
`which python3` 代替 runtime 检查，也不能通过普通 exec 启动插件。

插件作者还需要明确 workspace 权限。`none` 不提供 `/workspace`，`read` 使用临时快照且修改
不会写回，`write` 才能修改真实 workspace。脚手架默认是 `none`，需要访问项目文件时应在
启用前说明权限变化。

## 配置和可观测性

插件启用配置应通过专用配置 API 或插件管理工具修改。不要让模型用宽范围的 `sed` 修改整个
`clawseed.toml`，因为这可能误改其他 section。插件管理至少需要提供插件 ID、版本、runtime、
权限、加载状态和失败原因。

Gateway 提供两个专用端点：`GET /api/plugins` 返回已发现的 manifest、校验状态和启用策略，
`PATCH /api/plugins/config` 只更新 `[plugins]` 的字段。配置修改成功后返回
`reload_required=true`，已有 Agent 需要重启 Gateway 才会重建插件工具注册表。Agent 内置的
`plugin_list` 工具提供同样的本地清单查询能力，不会启动插件进程。

## 临时文件

测试生成的 scaffold 只有在确认属于本次测试且不属于用户 workspace 项目后才能清理。未知目录
不自动删除。
