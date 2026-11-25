# 编译与测试流程

## 概述

ClawSeed 使用 Cargo workspace 管理多 crate 项目，支持多平台交叉编译（Linux、macOS、Android），并配有完整的 CI 流水线。

## 本地编译

### 基础编译

```bash
# 开发编译（快速，无优化）
cargo build

# Release 编译（最大优化，用于发布）
cargo build --release

# 快速 Release（8 个 codegen units，迭代更快）
cargo build --profile release-fast
```

### 编译 Profile 对比

| Profile | opt-level | LTO | codegen-units | strip | panic | 用途 |
|---------|-----------|-----|---------------|-------|-------|------|
| `dev` | 0 | — | — | — | unwind | 日常开发 |
| `release` | z | fat | 1 | true | abort | 生产发布 |
| `release-fast` | z | fat | 8 | true | abort | 快速迭代 |
| `ci` | z | thin | 16 | true | abort | CI 流水线 |
| `dist` | z | fat | 1 | true | abort | 分发构建 |

### 运行 Gateway

```bash
# 开发模式
cargo run -- gateway --host 0.0.0.0 --port 3000

# Release 模式
./target/release/clawseed gateway --host 0.0.0.0 --port 3000
```

## Android 交叉编译

### 构建脚本

使用 `tools/build-clawseed-android.sh` 进行 Android 交叉编译：

```bash
# 默认编译 aarch64 (arm64-v8a)
./tools/build-clawseed-android.sh

# 指定架构
./tools/build-clawseed-android.sh aarch64   # arm64-v8a
./tools/build-clawseed-android.sh x86_64    # x86_64 (模拟器)
./tools/build-clawseed-android.sh armv7     # armeabi-v7a

# 仅检查（不生成二进制）
./tools/build-clawseed-android.sh aarch64 check
```

### 支持的 Android 架构

| 架构参数 | Rust Target | Android ABI |
|----------|-------------|-------------|
| `aarch64` | `aarch64-linux-android` | `arm64-v8a` |
| `x86_64` | `x86_64-linux-linux` | `x86_64` |
| `armv7` | `armv7-linux-androideabi` | `armeabi-v7a` |

### NDK 配置

脚本自动查找 Android NDK：

1. 优先使用 `ANDROID_NDK_ROOT` 环境变量
2. 其次使用 `ANDROID_HOME/ndk/` 下的最新版本
3. 默认路径：`$HOME/Android/Sdk/ndk/29.0.14206865`

NDK 需要的组件：
- NDK clang 编译器（API level 21+）
- AR 归档工具
- 设置 `CC`、`AR`、`CARGO_TARGET_*_LINKER` 环境变量

### 编译输出

二进制文件自动复制到 Android 项目的 jniLibs 目录：

```
clients/android/app/src/main/jniLibs/
├── arm64-v8a/
│   └── libclawseed.so
├── armeabi-v7a/
│   └── libclawseed.so
└── x86_64/
    └── libclawseed.so
```

### Feature Flags

Android 构建使用 `--no-default-features --features android`。

| Crate | Feature | 默认 | 说明 |
|-------|---------|------|------|
| `clawseed` | `android` | yes | Android 平台适配 |
| `clawseed-agent` | `cron-engine` | yes | 定时任务引擎 |
| `clawseed-gateway` | `channel-nostr` | no | Nostr 通道 |
| `clawseed-gateway` | `observability-prometheus` | no | Prometheus 指标 |
| `clawseed-gateway` | `tls-tests` | no | TLS 测试 |

### 编译 Android Demo APK

```bash
# 1. 交叉编译 Rust 二进制
./tools/build-clawseed-android.sh aarch64

# 2. 编译 Android APK
cd clients/android
./gradlew assembleDebug    # Debug 版本
./gradlew assembleRelease  # Release 版本

# APK 输出
# app/build/outputs/apk/debug/app-debug.apk
# app/build/outputs/apk/release/app-release.apk
```

如果要为 Android App 配置在线 PRoot runtime 清单，在构建时传入 HTTPS 地址：

```bash
./gradlew \
  -PprootRuntimeManifestUrl=https://example.com/clawseed/proot/manifest.json \
  -PprootRuntimePublisherKeyId=release-2026 \
  -PprootRuntimePublisherKey='<base64-ed25519-public-key>' \
  assembleDebug
```

未配置 manifest 时不会自动下载 runtime；已安装的本地 runtime 仍可由执行后端使用。
开发测试可以显式加入 `-PprootRuntimeAllowUnsigned=true` 使用未签名夹具；该模式也允许通过本地 HTTP manifest/archive 验证安装流程。生产构建不得启用该选项，生产 runtime 必须使用 HTTPS 和签名清单。

### PRoot App 开发夹具（adb 预置）

首个 App 测试版本不依赖下载服务器。生成 fixture 后，脚本会安装 Debug APK，并通过 `run-as` 将 runtime generation 写入应用私有目录：

```bash
./tools/install-proot-runtime-fixture-adb.sh \
  clients/android/app/build/outputs/apk/debug/app-debug.apk \
  <adb-serial>
```

如果 `adb` 不在 PATH，可以设置：

```bash
ADB_BIN=/root/Android/Sdk/platform-tools/adb \
  ./tools/install-proot-runtime-fixture-adb.sh \
  clients/android/app/build/outputs/apk/debug/app-debug.apk \
  <adb-serial>
```

该 fixture 的 `proot` 是 host-shell launcher，只验证 generation、状态、超时、输出限制和工具路由，不提供真实 Linux 用户空间或安全隔离。安装后在对话中调用 `runtime_status` 和 `linux_exec`；真实 PRoot 验收必须替换为官方或自建的 arm64 runtime bundle。下载、断点续传和正式签名发布暂不属于首测路径。

### 真实 PRoot bundle

生成包含 Ubuntu Base、Node.js/npm、Python/pip 的离线 arm64 bundle（完整流程见 [Android 离线 Linux runtime](android-linux-runtime.md)）：

```bash
./tools/build-proot-runtime.sh arm64-v8a build/proot-runtime-arm64
python3 tools/build-tawcroot.py --ndk "$ANDROID_HOME/ndk/28.2.13676358"
./tools/package-proot-bootstrap.sh
```

产物包含 `proot`、`proot-loader`、`proot-libs/` 和含解释器的 `rootfs/`，不包含 DSH。Gradle 将 packaged 目录中的资源打入 APK，优先在设置中离线安装。旧版 tar.gz bundle 也可通过 ADB 预置：

```bash
ADB_BIN=/mnt/c/Users/Lenovo/AppData/Local/Android/Sdk/platform-tools/adb.exe \
./tools/install-proot-runtime-bundle-adb.sh \
  /tmp/clawseed-real-proot/clawseed-proot-runtime-arm64-v8a.tar.gz \
  /tmp/clawseed-real-proot/manifest.json \
  clients/android/app/build/outputs/apk/debug/app-debug.apk \
  <adb-serial>
```

真实 bundle 的执行仍然是 PRoot 用户态兼容层，不提供 Android Root 或内核级隔离。Node.js 和 Python 已纳入离线包；Git 和编译工具链尚未纳入。

已在实体 `arm64-v8a` 设备（Android API 36）完成真实启动验收：

- PRoot ELF 成功从 App 私有目录执行；
- `PROOT_LOADER`、`LD_LIBRARY_PATH`、`PROOT_TMP_DIR` 和 `PROOT_L2S_DIR` 生效；
- Ubuntu Base rootfs 中的 `/etc/lsb-release` 可被真实 guest shell 读取；
- Gateway 日志报告真实 runtime generation 为 `ubuntu-24.04.5-arm64-v8a-proot-5.1.107.102-adb`；
- `/api/tools` 注册了 `runtime_status` 和 `linux_exec`。

本次测试 bundle SHA-256：

```text
2806f30a2aeef86be418e66adaf2457208b2c6516cfb4997dd625836cab96e61
```

## 其他交叉编译目标

项目在 `.cargo/config.toml` 中配置了多个交叉编译目标：

| Target | 用途 | 链接器 |
|--------|------|--------|
| `x86_64-unknown-linux-musl` | Linux 静态链接 | musl-gcc |
| `aarch64-unknown-linux-gnu` | ARM64 Linux (Graviton) | aarch64-linux-gnu-gcc |
| `aarch64-unknown-linux-musl` | ARM64 静态链接 | musl-gcc |
| `x86_64-pc-windows-msvc` | Windows | MSVC |
| `aarch64-linux-android` | Android | NDK clang |

```bash
# 编译 Linux 静态二进制
cargo build --release --target x86_64-unknown-linux-musl

# 编译 ARM64 Linux
cargo build --release --target aarch64-unknown-linux-gnu
```

## 测试

### 运行测试

```bash
# 运行所有测试
cargo test

# 运行特定 crate 的测试
cargo test -p clawseed-agent
cargo test -p clawseed-tools

# 运行特定测试
cargo test -p clawseed-agent test_single_tool_call

# 使用 cargo-nextest（并行执行，更快）
cargo nextest run --locked --workspace

# 显示输出（不截断）
cargo test -- --nocapture

# 运行被忽略的测试
cargo test -- --ignored
```

### 测试分类

| 类型 | 位置 | 说明 |
|------|------|------|
| 单元测试 | `src/` 文件内 `#[cfg(test)]` | 测试单个函数/结构体 |
| 集成测试 | `tests/agent_integration.rs` | 完整 Agent 循环 E2E 测试 |
| 健壮性测试 | `tests/agent_robustness.rs` | 错误处理和边界情况 |
| 系统测试 | `tests/agent_system.rs` | 真实后端（SQLite 等）集成 |

### 测试工具

| 工具 | 用途 |
|------|------|
| `MockProvider` | 脚本化 LLM 响应（FIFO 队列） |
| `RecordingProvider` | 记录请求用于断言 |
| `EchoTool` | 回显工具（返回输入） |
| `CountingTool` | 计数工具（追踪调用次数） |
| `RecordingTool` | 记录工具（捕获参数） |
| `FailingTool` | 失败工具（模拟错误） |
| `build_agent()` | 构建测试 Agent（NoneMemory + NoopObserver） |
| `build_agent_with_sqlite_memory()` | 构建带 SQLite 的 Agent |

### 覆盖率

```bash
# 使用 cargo-llvm-cov 生成覆盖率
./tools/coverage.sh

# 输出
# coverage/index.html  — HTML 报告
# lcov.info            — LCOV 格式
```

## CI 流水线

### 流程图

```
┌─────────┐     ┌──────────────────────────────────┐     ┌──────────────┐
│  Lint    │────→│           Build + Check           │────→│    Test      │
│ (15 min) │     │  ┌─────────┐    ┌──────────────┐ │     │  (30 min)    │
│          │     │  │  Build  │    │    Check     │ │     │  nextest     │
│  fmt     │     │  │ Linux   │    │ all-features │ │     │  --locked    │
│  clippy  │     │  │ macOS   │    │ no-default   │ │     │  --workspace │
└─────────┘     │  └─────────┘    └──────────────┘ │     └──────┬───────┘
                └──────────────────────────────────┘            │
                                                                ↓
┌──────────────┐  ┌──────────────┐  ┌──────────────────────────┐
│  Security    │  │  Coverage    │  │      ADF Guard           │
│  (15 min)    │  │  (30 min)    │  │      (5 min)             │
│  cargo-deny  │  │  llvm-cov    │  │  文件大小检查             │
│  licenses    │  │  HTML + LCOV │  │  >1000 行警告            │
│  advisories  │  │              │  │  >3 个文件 error          │
└──────────────┘  └──────────────┘  └──────────────────────────┘
```

### 阶段详情

#### Stage 1: Lint（15 分钟超时）

- Rust 1.95.0 + rustfmt + clippy
- `cargo fmt --all -- --check`
- `cargo clippy -- -D warnings`（所有警告视为错误）
- 使用 rust-cache 加速

#### Stage 2: Build（40 分钟超时，依赖 Lint）

- **并行构建两个平台**：
  - `x86_64-unknown-linux-gnu`（ubuntu-latest）
  - `aarch64-apple-darwin`（macos-14）
- Linux 安装 mold 链接器
- `cargo build --profile ci --locked --target`

#### Stage 2b: Check（20 分钟超时，与 Build 并行）

- Feature 矩阵检查：
  - `--all-features`
  - `--no-default-features`
- `cargo check --locked`

#### Stage 3: Test（30 分钟超时，依赖 Lint）

- 使用 **cargo-nextest** 并行执行
- `cargo nextest run --locked --workspace`
- Linux 安装 mold 链接器

#### Stage 4: Security（15 分钟超时，与 Test 并行）

- **cargo-deny** 检查许可证、来源和安全公告
- `cargo deny check`
- 配置：`.cargo/audit.toml`

#### Stage 5: Coverage（30 分钟超时，并行）

- **cargo-llvm-cov** 生成覆盖率
- HTML + LCOV 报告
- 上传 `lcov.info` 为 artifact

#### Stage 6: ADF Guard（5 分钟超时，并行）

- 检查 Rust 文件大小
- >1000 行：警告
- >3 个文件超过 1000 行：错误

### 并发控制

- 新的推送自动取消进行中的同分支构建
- 所有阶段必须通过才算成功

## 代码质量工具

```bash
# 格式化
cargo fmt --all

# Lint 检查
cargo clippy -- -D warnings

# 安全审计
cargo deny check

# 依赖检查
cargo audit
```

## 性能分析

```bash
# 使用 release-fast profile 编译 + samply 分析
./tools/profile.sh
```
