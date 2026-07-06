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
