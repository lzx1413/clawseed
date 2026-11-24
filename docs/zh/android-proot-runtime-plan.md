> 实现进展：已加入 Node.js/Python 离线打包、APK assets 安装及安装时 guest 执行检查。当前用法与验证边界见 [Android 离线 Linux runtime](android-linux-runtime.md)。下文保留早期设计与分阶段规划。

---
title: Android PRoot Linux 执行环境开发方案
description: 为 ClawSeed Android App 提供可下载、可更新、受约束的 Linux 用户态执行环境。
---

# Android PRoot Linux 执行环境开发方案

## 在 ClawSeed 中的定位

PRoot Runtime 是 ClawSeed 动态扩展执行层的一部分，不替代 Rust Agent 内核。Rust 进程继续负责会话、分身、记忆、工具策略和审批；Runtime 负责为 Python/Node.js 插件、Linux CLI、Git 和编译任务提供可复现的用户态环境。CETP 则把本地插件、Android 外部应用工具和其他客户端工具统一接入 Tool Registry。Runtime 未安装或不满足权限时，能力应明确不可用，不能静默绕回 Android shell。

## 1. 背景

ClawSeed 当前的 shell 类工具直接运行在 Android 应用进程所在的系统环境中。它适合执行简单的文件操作和设备命令，但 Android 用户空间与常见 Linux 发行版存在差异：命令集不完整、工具参数和路径行为不同，Python、Node.js、Git 以及编译工具链也不能假设已经存在。

本方案为 Android App 增加一个基于 PRoot 的 Linux 用户态运行时。它不需要 Root 或 Termux，通过下载并管理独立的 rootfs，为 Agent 提供更接近服务器 Linux 的命令和依赖环境。

方案参考了 DSH Go 的端侧运行时模式：应用内选择本地运行时，首次启动时准备 Ubuntu、Node.js 等依赖，并使用兼容模式启动 PRoot。另一个 dsh-mobile 实现则将最小 Debian rootfs、PRoot 和业务包放入 APK，在首次启动时解压到应用私有目录。ClawSeed 采用两者的折中方案：APK 只内置启动所需的 native bootstrap，完整运行时通过版本化资源安装到应用私有目录。bootstrap 不包含可用的 Linux 用户空间，不能替代 runtime bundle。

参考：

- [DSH Go](https://github.com/lxsfful/dsh-go/blob/main/README.en.md)
- [dsh-mobile](https://github.com/dsh-io/dsh-mobile)

### 1.1 DSH Go runtime 供应链参考

DSH Go 的 runtime 供应链主要由公开仓库中的构建脚本、锁定输入、Android 安装器和发布校验文件组成。它不是一个可以直接作为稳定 SDK 依赖的独立 runtime 服务。

**构建输入和产物**：

- `runtime/build-runtime.sh` 固定 Ubuntu Base、Node.js、Python Standalone、PRoot、Termux native libraries 和 DSH 版本；
- 上游资源使用 SHA-256 固定值，Ubuntu 软件包解析还验证发行版签名索引；
- 按 ABI 生成 rootfs、manifest、许可证和 native PRoot 组件；
- 典型产物包括 `rootfs-<abi>.tar.xz`、`manifest.json`、`libproot.so`、`libproot-loader.so`、`libtalloc.so` 和 `libandroid-shmem.so`。

参考：

- [DSH Go runtime 构建脚本](https://raw.githubusercontent.com/lxsfful/dsh-go/main/runtime/build-runtime.sh)
- [DSH Go Android runtime 架构](https://github.com/ccch1mneyyy/dsh-Go/blob/main/docs/architecture.md)
- [DSH Go Releases](https://github.com/ccch1mneyyy/dsh-Go/releases)

**公开性边界**：源码、构建脚本、版本锁定和发布校验信息公开；普通用户主要下载包含 runtime 的 APK，独立 runtime bundle 不应视为稳定的公共下载 API。ClawSeed 可以审计脚本并自行构建，但不应依赖 DSH Go APK 内部路径或未承诺稳定的下载地址。

**复用边界**：可以复用版本锁定、哈希校验、rootfs 构建和 PRoot 依赖；不能直接把 DSH Go 的完整引擎、Node/插件 bridge 和 native 启动方式当作 ClawSeed 的 `proot` 可执行文件。其 `libproot.so` 还依赖 loader、talloc 和 Android shared-memory 库，必须适配启动模板，并保留 PRoot、Termux 组件和其他第三方许可证。

ClawSeed 首版继续使用自己的最小 Linux runtime；后续可以新增 DSH-compatible 多文件 manifest，作为可选的 runtime 构建来源，而不是运行时硬依赖 DSH Go。

## 2. 目标与非目标

### 2.1 目标

- 为 Android 端 Agent 提供稳定、可复现的 Linux 用户空间。
- 支持按设备 ABI 下载并安装 PRoot、rootfs 和可选工具组件。
- 首次安装后支持离线使用已安装的运行时。
- 支持运行时版本升级、失败回滚和损坏检测。
- 让 Agent 可以在 Android 原生 shell 与 PRoot Linux 之间选择合适的执行后端。
- 将命令超时、输出大小、工作目录、环境变量和文件映射纳入统一安全策略。
- 明确 PRoot 只是用户态兼容层，不把它当作内核级安全沙箱。

### 2.2 非目标

- 不提供真正的 Linux Root 权限。
- 不实现虚拟机、容器内核或完整的 Android 隔离沙箱。
- 不保证所有依赖内核能力的程序可用，例如部分 FUSE、挂载、ptrace 和后台守护进程。
- 第一阶段不把在线 `apt install` 作为运行时安装的必要步骤。
- 不默认把整个 `/sdcard` 或 Android 文件系统暴露给 Linux 环境。
- 不承诺阻止所有利用内核、PRoot 或宿主进程信息实现的越界访问；需要强隔离时应使用独立沙箱或虚拟机。

## 3. 总体架构

```text
┌─────────────────────────────────────────────────────────────┐
│ Android App                                                  │
│                                                             │
│  MainActivity / Settings                                    │
│          │                                                  │
│          ▼                                                  │
│  RuntimeManager (Kotlin)                                    │
│  - 检测 ABI 和 Android 版本                                 │
│  - 下载、断点续传、校验、解压、切换版本                    │
│  - 管理安装状态和磁盘空间                                  │
│          │  runtime manifest / local API                    │
│          ▼                                                  │
│  ClawseedService + Rust Gateway                             │
│  - LinuxRuntime trait                                       │
│  - ProotRuntime 子进程管理                                  │
│  - shell / linux_exec 工具路由                              │
│          │                                                  │
│          ▼                                                  │
│  PRoot + Linux rootfs                                       │
│  - /workspace: ClawSeed 工作区                              │
│  - /tmp: 任务临时目录，首版按任务创建并清理                 │
│  - 可选工具组件：Python、Node.js、Git、编译器等              │
└─────────────────────────────────────────────────────────────┘
```

### 3.1 控制面与执行面

**控制面**由 Android `RuntimeManager` 负责。它是 runtime 状态的唯一来源，处理下载源、版本清单、签名验证、安装事务、更新、回滚和用户可见状态。下载失败、空间不足或校验失败时，必须保留当前 active 版本可用。

**执行面**由 Rust 侧 `LinuxRuntime` 抽象负责。它只读取 Runtime Manager 已经激活的版本，不负责下载、切换或删除 runtime。Agent 和工具只依赖抽象接口，不直接拼接 PRoot 命令行。这样未来可以替换为 Termux、远程 Linux、WASI 或其他执行后端。

推荐接口形态：

```text
LinuxRuntime
  status() -> RuntimeStatus
  check(requirements) -> RuntimeStatus
  execute(request) -> ExecutionResult
  cancel(execution_id)
  stop_all()
```

`ProotRuntime` 是第一种实现，负责将结构化请求转换为 PRoot 参数、启动子进程、收集输出、处理退出状态和取消信号。

Android 上的 PRoot executable bootstrap 必须来自 APK `nativeLibraryDir`（例如
`libproot.so` 和 `libproot-loader.so`）；rootfs 和非执行库可以位于 active generation 的
`filesDir`。执行器不能尝试执行 `filesDir` 下的 ELF，否则会被 Android SELinux 的
`untrusted_app` 域拒绝。`LD_LIBRARY_PATH` 可以指向 generation 内的 `proot-libs/`，因为
这些文件只作为动态依赖加载，不作为进程入口执行。

Runtime Manager 与 Rust 执行面的最小交互协议必须包含：当前 active 版本、ABI、rootfs 路径、组件集合和 generation 标识。Rust 只能使用一次读取到的 generation；更新时已有任务继续使用旧 generation，新任务使用新 generation。

### 3.2 安全边界

PRoot 提供的是用户态路径和身份兼容，不是内核级隔离。首版安全目标是减少误操作和控制资源消耗，而不是抵御恶意代码。具体约束如下：

- 只绑定 ClawSeed workspace 和任务临时目录；不默认绑定 `/sdcard`、应用外部存储或任意宿主路径；
- rootfs、PRoot 参数、环境变量和工作目录由执行器生成，工具调用方不能覆盖；
- 对命令设置超时、输出上限、进程组回收和并发上限；
- 网络默认采用宿主网络，不宣称 PRoot 可以提供网络隔离；需要网络控制时必须由 Android 或系统层另行实现；
- 结果中明确标注“PRoot 用户态环境”，不能向 Agent 或用户暗示获得了 Android Root 权限。

## 4. Runtime Manager

### 4.1 安装目录

运行时必须存放在应用私有目录，不依赖外部存储权限。建议目录结构如下：

```text
files/runtime/
  active.json
  state.json
  install.lock
  manifests/
    <abi>-<runtime-id>.json
  versions/
    <runtime-id>/
      proot
      rootfs/
      components/
      manifest.json
  staging/
  logs/
```

`active.json` 只指向已经完整校验的 runtime generation。新版本先写入 `staging`，完成校验和自检后，通过同一文件系统内的原子替换切换 active 指针，不能覆盖正在使用的版本。安装过程必须持有 `install.lock`；崩溃恢复时只清理未提交的 staging 目录。旧 generation 在没有执行任务引用后才能清理，并至少保留一个可回滚版本。

### 4.2 版本清单

服务端提供按 ABI 和发布渠道区分的签名清单。清单至少包含：

| 字段 | 说明 |
|------|------|
| `schema_version` | 清单格式版本 |
| `runtime_version` | 运行时整体版本 |
| `runtime_id` | 包含 ABI、发行版和构建版本的不可变标识 |
| `abi` | `arm64-v8a`、`x86_64` 等 |
| `min_android` | 最低 Android API level |
| `archive_size` | bundle 的字节数 |
| `archive_url` | PRoot + rootfs bundle 下载地址 |
| `archive_sha256` | bundle SHA-256 |
| `archive_format` | `tar.gz`（默认）或开发测试用 `zip` |
| `components` | 可选工具组件及依赖关系 |
| `publisher_key_id` | 发布公钥标识 |
| `signature` | 对规范化清单内容的签名 |

首版 Android Runtime Manager 使用单个 `archive_url`、`archive_sha256` 和可选的
`archive_format` 字段安装 bundle。默认格式为 `tar.gz`，压缩包内必须包含 `proot` 和
`rootfs/`，并保留 rootfs 的目录、权限及符号链接；`zip` 仅用于开发测试夹具。

如果采用 DSH Go 的构建产物，manifest 需要扩展为多文件 artifact 形式，分别描述
rootfs、PRoot loader、talloc 和 Android shared-memory 库。首版规范不消费这种格式，不能
直接把 `libproot.so` 重命名为 `proot` 后执行。

最小清单示例：

```json
{
  "schema_version": 1,
  "runtime_version": "debian-arm64-2026.11.20",
  "runtime_id": "debian-arm64-2026.11.20+1",
  "abi": "arm64-v8a",
  "min_android": 26,
  "archive_size": 123456789,
  "archive_format": "tar.gz",
  "archive_url": "https://example.com/clawseed/proot/debian-arm64-2026.11.20.tar.gz",
  "archive_sha256": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
  "publisher_key_id": "release-2026",
  "signature": "base64-encoded-signature"
}
```

正式发布时应用应内置发布公钥，并定义签名覆盖的规范化 JSON（签名字段本身除外）、算法、公钥轮换和撤销规则。HTTPS 只提供传输保护；哈希用于校验资源内容；签名用于确认清单的发布者，三者不能互相替代。

首版必须把签名清单作为发布前置条件。开发测试可以使用未签名夹具，但不得让未签名清单进入生产渠道。首版不支持 DSH Go 的多文件 artifact manifest；如未来支持，必须新增 schema 版本并为每个 artifact 提供独立哈希、签名覆盖范围和依赖声明。

### 4.3 Runtime 状态机

Runtime Manager 对外只暴露以下状态：

| 状态 | 含义 | 允许的下一步 |
|------|------|--------------|
| `NOT_INSTALLED` | 没有可用 active generation | 用户确认后安装 |
| `INSTALLING` | 正在下载、解压或自检 | 完成、失败或取消 |
| `READY` | active generation 通过完整校验 | 执行、升级、卸载 |
| `UPGRADE_AVAILABLE` | 当前版本可用，存在更高兼容版本 | 用户确认后升级 |
| `CORRUPTED` | active generation 自检失败 | 回滚、重新安装或清理 |
| `FAILED` | 最近一次操作失败，但旧 active 仍可能可用 | 重试或继续使用旧版本 |

`FAILED` 不等于没有 runtime；如果旧 generation 仍然完整，执行面必须继续报告 `READY`，同时把安装失败作为控制面事件展示给用户。状态迁移和 active 指针更新必须可恢复，不能依赖内存状态。

### 4.4 下载和安装流程

1. 检测设备 ABI、Android 版本、可用磁盘空间和网络状态。
2. 获取并验证对应渠道的签名清单，校验 `schema_version`、`runtime_id`、ABI、最低 Android 版本和资源大小。
3. 检查当前版本是否满足 Agent 当前请求的组件需求；不满足时返回 `UPGRADE_AVAILABLE` 或 `INSTALL_REQUIRED`，不在工具调用中隐式下载。
4. 将缺失资源下载到 `staging`，支持 HTTP Range 断点续传和有限次数重试。续传记录必须绑定 URL、大小和 SHA-256，任一变化都要重新下载。
5. 校验文件大小、SHA-256、签名和压缩包结构。
6. 解压到临时 generation 目录，检查 PRoot 可执行权限、rootfs 必要文件、组件 manifest 和启动自检命令。
7. 在持有安装锁的情况下原子提交 generation manifest，再更新 `active.json`；任一步失败都保留原 active 指针。
8. 记录新旧 generation 的引用关系，确认没有任务使用旧版本后再清理，至少保留一个可回滚版本。

runtime 安装应由设置页或首次使用向导显式触发，不能在普通工具调用中无提示下载大文件。首次安装不应因为可选组件失败而阻止最小运行时使用。最小 rootfs 安装成功后，Python、Node.js、Git 和编译器等组件可以按需安装。

### 4.5 组件策略

基础运行时建议包含：

- PRoot；
- `/bin/sh` 或 `/bin/bash`；
- `coreutils`、`findutils`、`grep`、`sed`、`awk`；
- `tar`、`gzip`、`ca-certificates`；
- DNS 和时区数据；
- ClawSeed Linux worker 所需的最小依赖。

可选组件建议单独发布，并且每个组件必须声明 ABI、最低 runtime 版本、依赖、磁盘占用、许可证和独立哈希。首版只允许官方签名组件：

- Python 运行时及常用标准库；
- Node.js；
- Git；
- Rust、C/C++ 或其他编译工具链；
- 用户或技能声明的专用 CLI（首版暂不开放任意用户上传）。

组件包应由项目构建流水线固定版本并生成 manifest。运行时安装阶段不依赖实时发行版源，以避免镜像变化导致 Agent 行为不可复现。组件安装、卸载和升级必须可单独回滚，并在低磁盘空间时拒绝操作。后续可以提供用户主动触发的包管理工具，但它不应成为基础启动路径。

## 5. Rust 执行后端

### 5.1 请求模型

`LinuxRuntime` 接收结构化请求，而不是只接收一段未经约束的 shell 字符串。请求至少包含：

```text
execution_id
argv 或 script
working_directory
environment
stdin（可选）
timeout
max_output_bytes
network_policy
```

首版默认使用明确的 shell 脚本请求，但接口必须保留 `argv` 扩展位。需要执行多步脚本时，将脚本写入任务临时目录后以固定的 shell 解释器启动，并记录脚本来源和任务 ID。以下字段为规范行为：

- `working_directory` 只能是 `/workspace` 或其子目录，默认是 `/workspace`；
- `environment` 采用白名单，不允许覆盖 rootfs、绑定目录、动态加载器或执行器控制变量；
- `timeout`、`max_output_bytes` 和并发上限由执行器设定最大值，调用方只能在范围内收紧；
- `network_policy` 首版默认使用 `host`；只有 Android 或系统层具备可靠实现时才开放 `deny` 或域名白名单，未实现的策略不得出现在接口中；
- `stdin` 默认关闭，任务临时目录在任务结束后清理；
- `ExecutionResult` 必须包含退出码、stdout、stderr、终止原因、是否截断和使用的 runtime generation。

### 5.2 PRoot 启动约束

PRoot 启动器统一设置：

- rootfs 路径和工作目录；
- `/workspace` 到 ClawSeed workspace 的映射；
- 每次任务独立的 `/tmp`，并在任务结束、超时或取消后清理；
- 最小且明确的环境变量集合；
- locale、时区、DNS 和证书路径；
- 子进程的标准输入、输出、错误和退出信号处理。

不允许调用方任意覆盖 PRoot 的 rootfs、绑定目录和宿主路径参数。任务开始时固定使用一个 generation；运行期间即使 active 指针变化，也不得改变该任务的 rootfs。取消任务时先发送终止信号，超时后强制回收整个进程组，避免留下后台子进程。PRoot 不能可靠限制恶意程序的内核行为，因此这些约束不等价于安全沙箱。

### 5.3 与现有 shell 工具的混合路由

保留现有 Android shell 后端，并新增 Linux 后端：

| 场景 | 默认后端 |
|------|----------|
| 设备信息、Android 专用能力 | Android host |
| 简单文件操作和快速查询 | Android host |
| Bash 脚本、Linux CLI 工具链 | PRoot Linux |
| Python、Node.js、Git、编译任务 | PRoot Linux |
| 需要访问 `/workspace` 的项目任务 | PRoot Linux |

路由可以由工具显式指定，也可以根据技能声明和组件需求选择。路由选择必须在执行前完成，不能在失败后静默切换后端。工具结果统一返回退出码、标准输出、标准错误、是否超时、是否截断、runtime generation 和后端标识，避免 Agent 把 Android shell 和 Linux shell 的行为混为一谈。

## 6. 工具和 Agent 集成

第一阶段建议增加独立的 `linux_exec` 工具，而不是改变现有 shell 工具的语义。runtime 安装和升级属于控制面操作，首版由设置页或首次使用向导触发，不默认暴露为 Agent 可自由调用的工具：

- `shell_exec`：调用 Android host 后端，保持现有兼容行为；
- `linux_exec`：调用 PRoot Linux 后端；
- `runtime_status`：查询运行时版本、组件和安装状态；
- `runtime_install`：控制面内部操作，在用户允许后安装指定 runtime 或组件；
- `runtime_update`：控制面内部操作，在用户允许后检查并安装兼容更新。

这些工具仍需经过现有工具注册、Hook、`SecurityPolicy`、路径防护和自治级别控制。运行时未安装时，`linux_exec` 应返回可操作的状态错误，而不是静默回退到 Android shell。

Gateway 和 Android 端共享的工具注册表应只暴露当前已安装且策略允许的能力。远程 Agent 不能绕过 Android 端的 runtime 状态和权限检查。

## 7. 安全边界

PRoot 主要提供路径和用户态兼容层，不是安全边界。安全策略必须同时在 Rust 执行器和 Android 文件访问层实现。

### 7.1 文件访问

- 默认只映射 ClawSeed workspace 和任务临时目录；
- 不映射整个 `/sdcard`、应用数据库、凭据目录或其他 App 私有目录；
- 映射路径必须经过 canonicalization 和允许列表检查；
- 执行器拒绝通过 `..` 或自定义 PRoot 参数扩大映射范围；符号链接只能在允许的 workspace 范围内按宿主文件系统规则校验，不能把这项检查宣称为对恶意 PRoot 程序的强隔离；
- 工具输出中的文件路径应经过脱敏和长度限制。

### 7.2 命令和资源

- 默认设置超时、最大输出、并发数和单任务临时空间上限；
- 限制后台进程和孤儿进程，任务结束时回收进程组；
- 默认不允许修改运行时 rootfs；需要持久化写入时写入 workspace 或组件目录；
- 网络访问首版只承诺已实现的策略。PRoot 本身不提供可靠的域名级隔离，禁止网络或域名白名单必须由 Android/系统层实现；
- 需要安装组件、访问外部目录或执行高风险命令时触发审批 Hook；
- 不将 PRoot 的伪 root 身份解释为 Android root 权限。

### 7.3 供应链

- PRoot、rootfs 和组件均固定来源、版本和哈希；
- 发布清单使用应用内置公钥验证；
- 构建产物保留许可证和第三方 NOTICE；
- 下载、校验、解压和启动日志不记录模型密钥、用户文件内容或完整命令敏感参数。

## 8. 生命周期和异常恢复

### 8.1 App 启动

`ClawseedService` 启动时读取 `active.json`，检查 manifest、文件权限和 PRoot 可执行性。检查失败时标记 runtime 为 `CORRUPTED`，尝试回滚到上一版本；启动路径只做快速检查，不同步下载大文件。需要安装或升级时由设置页、首次使用向导或前台任务显式触发。

### 8.2 Agent 请求

1. 工具声明所需 runtime 和组件。
2. Runtime Manager 返回 `READY`、`INSTALL_REQUIRED`、`UPGRADE_AVAILABLE` 或 `CORRUPTED`。
3. 需要用户确认或网络下载时，先结束当前工具调用并返回明确状态。
4. 运行时就绪后创建执行任务，并由 Rust 后端负责超时、取消和结果回传。

### 8.3 服务停止和更新

前台服务停止、App 更新或运行时切换时，先停止新任务，再等待或取消正在执行的 PRoot 进程。更新失败不影响当前 active 版本；只有新版本通过完整自检后才允许切换。正在执行的任务固定引用启动时的 generation，不能因为 active 指针变化而切换 rootfs。

Android 进入后台时，长任务应复用现有前台服务和 WakeLock 策略，不能仅依赖普通后台线程维持 PRoot 子进程。

## 9. 可观测性和用户体验

运行时设置页应显示：

- 当前状态：未安装、下载中、已就绪、更新中、损坏或回滚；
- ABI、rootfs 发行版、runtime 版本和已安装组件；
- 已占用空间和可释放空间；
- 当前下载进度、失败原因和重试入口；
- 最近一次校验、更新和启动日志的摘要。

首次安装和升级必须提供明确的用户流程：展示预计下载量和所需空间，允许用户选择网络条件，支持取消、重试和后台恢复；安装期间不能阻塞普通网关启动。无网络时已安装的 READY generation 可以继续使用；空间不足、签名失败和兼容性不满足必须给出可操作的错误。

Agent 工具结果至少要能区分：

- 命令正常退出；
- 命令非零退出；
- 超时或被取消；
- runtime 未安装或损坏；
- 权限策略拒绝；
- 输出被截断。

## 10. 分阶段实施

### Phase 0：接口和产物设计

- 定义 `LinuxRuntime`、runtime 状态和执行结果模型；
- 定义 runtime manifest、签名和组件包格式；
- 固定控制面为唯一状态源，定义 active generation、安装锁、回滚和任务引用规则；
- 固定首版的文件映射、环境变量、临时目录、超时、输出和并发上限；
- 为 Rust 执行器增加 mock backend 和单元测试；
- 决定首个 rootfs 发行版、支持的 ABI、最低 Android 版本和下载源；
- 评估 DSH Go runtime 构建脚本作为可选构建来源，不直接依赖其 APK 内部布局。

### Phase 1：Runtime Manager

- 实现 Android 端 ABI 检测、清单下载、签名和哈希校验；
- 实现断点续传、安装锁、临时目录、原子安装、崩溃恢复和旧版本回滚；
- 提供运行时状态 API 和设置页状态展示；
- 交付最小 PRoot + rootfs bundle；
- 记录 PRoot、rootfs 和第三方库的来源、版本、许可证和哈希。

### Phase 2：PRoot 执行后端

- 实现 Rust `ProotRuntime` 子进程启动、输出收集、超时和取消；
- 固定 `/workspace`、任务级 `/tmp`、环境变量和已实现的网络策略；
- 在 Android 真机上完成 arm64-v8a 启动和基础命令验收；
- 增加 `linux_exec` 和 `runtime_status` 工具；
- 验证任务取消、服务重启和 generation 切换期间的进程回收。

### Phase 3：组件和混合路由

- 增加 Python、Node.js、Git 等官方签名可选组件；
- 根据工具或技能需求触发组件检查；
- 明确 Android host 与 PRoot Linux 的工具路由；
- 增加安装确认、网络选择、磁盘空间检查和失败恢复。

### Phase 4：更新、安全和发布

- 增加签名清单轮换、版本回滚和增量资源支持；
- 完善网络、文件、进程和并发限制；
- 增加 Android 后台、进程被杀、断网和低磁盘场景测试；
- 补充第三方许可证、发布脚本和运行时镜像文档；
- 如复用 DSH Go 构建输入，固定 fork/commit、补丁版本和产物哈希。

## 11. 验收标准

- arm64-v8a 设备可以在无 Root、无 Termux 的条件下完成首次安装并启动 PRoot；
- 已安装 runtime 在无网络情况下可以继续执行基础 Linux 命令；
- 下载中断后可以续传，校验失败不会切换 active 版本；
- PRoot、rootfs 或组件损坏时可以检测并回滚；
- `linux_exec` 能正确返回退出码、stdout、stderr、超时、取消和截断状态；
- 执行器不会通过相对路径或自定义 PRoot 参数扩大宿主映射范围，并明确记录 PRoot 非强隔离边界；
- 任务取消或 Android 服务重启后不会遗留可继续运行的 PRoot 子进程；
- 运行时更新期间旧 generation 保持可用，更新失败不会影响已有任务；
- Android host 工具和 PRoot Linux 工具可以按策略并存，结果中能识别执行后端；
- 低磁盘、断网、后台、设备重启和 App 升级场景都有明确的用户可见错误；
- 无网络时已安装的 READY generation 可以继续执行基础命令，安装和升级不会阻塞普通网关启动。

当前实体设备已完成真实 PRoot 启动和 rootfs 文件存在性验收；对话层仍使用同一个已注册的 `linux_exec` 工具，后续应补充真实 guest 命令输出、workspace 映射和超时结果截图/日志。

### 11.1 对话内验收工具

首个 App 测试版本必须让用户可以在普通对话中直接验证 runtime，而不依赖调试终端。工具由 Rust 内置工具注册表提供：

1. `runtime_status`：确认 active generation、ABI、runtime 版本、rootfs 和 PRoot 路径。
2. `linux_exec`：在 PRoot 用户态环境中执行命令，支持 `command`、`timeout_secs` 和 `max_output_bytes`。

建议验收问题：

```text
检查 Linux runtime 是否就绪，并告诉我 runtime 版本和 generation。
在 Linux runtime 中执行 `printf 'clawseed-proot-ok\\n'`，只返回命令输出。
在 Linux runtime 中执行 `pwd && printf '\\n---\\n' && cat /etc/os-release`。
在 Linux runtime 中执行 `sleep 5`，设置 1 秒超时并报告超时状态。
```

对话结果必须能区分 Android host shell 和 PRoot Linux，不能在 runtime 未安装时静默回退到 host shell。开发夹具如果使用 fake launcher，只能验证下载、状态、工具路由、超时和输出边界，不得宣称提供真实 Linux 用户空间或安全隔离。

## 12. 需要在实现前确定的问题

1. 首个基础 rootfs 选择 Debian minimal 还是 Ubuntu minimal；
2. 首版是否只支持 `arm64-v8a`，最低 Android API level 是多少；
3. 运行时资源托管在 GitHub Releases、对象存储还是自有更新服务；
4. 首版组件清单和签名发布流程，以及是否完全禁止用户自定义组件；
5. 首版是否只使用宿主网络，系统层是否提供网络禁止或白名单能力；
6. Android 设置页和首次使用向导如何分工，安装是否允许后台恢复；
7. 是否采用 DSH Go 构建脚本生成 ClawSeed runtime，还是维护独立的最小 rootfs 构建链；
8. 如果采用 DSH Go 输入，是否以 fork/commit 固定来源，并由 ClawSeed 自己重新打包和发布；
9. 签名算法、公钥内置位置、密钥轮换和撤销策略是什么。

## 13. 推荐的首版范围

首版应聚焦于 `arm64-v8a`、一个固定发行版的最小 rootfs、PRoot、Bash、基础 coreutils。当前真实 bundle 构建脚本使用 Ubuntu Base 24.04.5 和 Termux PRoot 5.1.107.102，仅打包 `proot`、loader、talloc、Android shared-memory 库和 rootfs，不包含 DSH、Node.js 或 Python。Python、Node.js、Git 和编译器作为后续官方签名组件，不作为首版基础安装的硬依赖。使用签名的版本化 bundle 进行安装，不在运行时依赖实时 apt 源；只映射 ClawSeed workspace 和任务级临时目录；只提供 `linux_exec`、`runtime_status` 以及设置页触发的安装/升级；保留现有 Android shell 工具作为轻量后端。安装失败时保留旧 generation，普通网关启动不因下载大文件而阻塞。首版不直接携带完整 DSH Go 引擎；若复用其供应链，只复用可审计的构建输入和 PRoot 组件，并生成 ClawSeed 自己的 manifest、许可证清单和发布包。

这样可以先验证 Agent 在真实 Android 设备上的 Linux 工具链价值，再根据磁盘占用、启动时间和用户实际任务决定是否加入 Node.js、编译器、增量更新和更细的网络策略。
