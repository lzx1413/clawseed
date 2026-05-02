# Build and Test Workflow

## Overview

ClawSeed uses a Cargo workspace to manage a multi-crate project, supporting cross-compilation for multiple platforms (Linux, macOS, Android), with a comprehensive CI pipeline.

## Local Builds

### Basic Compilation

```bash
# Dev build (fast, no optimization)
cargo build

# Release build (maximum optimization, for distribution)
cargo build --release

# Fast release (8 codegen units, faster iteration)
cargo build --profile release-fast
```

### Build Profile Comparison

| Profile | opt-level | LTO | codegen-units | strip | panic | Use Case |
|---------|-----------|-----|---------------|-------|-------|----------|
| `dev` | 0 | — | — | — | unwind | Daily development |
| `release` | z | fat | 1 | true | abort | Production release |
| `release-fast` | z | fat | 8 | true | abort | Fast iteration |
| `ci` | z | thin | 16 | true | abort | CI pipeline |
| `dist` | z | fat | 1 | true | abort | Distribution builds |

### Running the Gateway

```bash
# Development mode
cargo run -- gateway --host 0.0.0.0 --port 3000

# Release mode
./target/release/clawseed gateway --host 0.0.0.0 --port 3000
```

## Android Cross-Compilation

### Build Script

Use `tools/build-clawseed-android.sh` for Android cross-compilation:

```bash
# Default: build aarch64 (arm64-v8a)
./tools/build-clawseed-android.sh

# Specify architecture
./tools/build-clawseed-android.sh aarch64   # arm64-v8a
./tools/build-clawseed-android.sh x86_64    # x86_64 (emulator)
./tools/build-clawseed-android.sh armv7     # armeabi-v7a

# Check only (no binary output)
./tools/build-clawseed-android.sh aarch64 check
```

### Supported Android Architectures

| Arch Argument | Rust Target | Android ABI |
|---------------|-------------|-------------|
| `aarch64` | `aarch64-linux-android` | `arm64-v8a` |
| `x86_64` | `x86_64-linux-linux` | `x86_64` |
| `armv7` | `armv7-linux-androideabi` | `armeabi-v7a` |

### NDK Configuration

The script auto-discovers the Android NDK:

1. First: `ANDROID_NDK_ROOT` environment variable
2. Then: latest version under `ANDROID_HOME/ndk/`
3. Default path: `$HOME/Android/Sdk/ndk/29.0.14206865`

Required NDK components:
- NDK clang compilers (API level 21+)
- AR archiver
- Sets `CC`, `AR`, `CARGO_TARGET_*_LINKER` environment variables

### Build Output

The binary is automatically copied to the Android project's jniLibs directory:

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

Android builds use `--no-default-features --features android`.

| Crate | Feature | Default | Description |
|-------|---------|---------|-------------|
| `clawseed` | `android` | yes | Android platform adaptation |
| `clawseed-agent` | `cron-engine` | yes | Cron scheduling engine |
| `clawseed-gateway` | `channel-nostr` | no | Nostr channel |
| `clawseed-gateway` | `observability-prometheus` | no | Prometheus metrics |
| `clawseed-gateway` | `tls-tests` | no | TLS tests |

### Building the Android Demo APK

```bash
# 1. Cross-compile Rust binary
./tools/build-clawseed-android.sh aarch64

# 2. Build Android APK
cd clients/android
./gradlew assembleDebug    # Debug build
./gradlew assembleRelease  # Release build

# APK output
# app/build/outputs/apk/debug/app-debug.apk
# app/build/outputs/apk/release/app-release.apk
```

## Other Cross-Compilation Targets

The project configures multiple cross-compilation targets in `.cargo/config.toml`:

| Target | Use Case | Linker |
|--------|----------|--------|
| `x86_64-unknown-linux-musl` | Linux static linking | musl-gcc |
| `aarch64-unknown-linux-gnu` | ARM64 Linux (Graviton) | aarch64-linux-gnu-gcc |
| `aarch64-unknown-linux-musl` | ARM64 static linking | musl-gcc |
| `x86_64-pc-windows-msvc` | Windows | MSVC |
| `aarch64-linux-android` | Android | NDK clang |

```bash
# Build Linux static binary
cargo build --release --target x86_64-unknown-linux-musl

# Build ARM64 Linux
cargo build --release --target aarch64-unknown-linux-gnu
```

## Testing

### Running Tests

```bash
# Run all tests
cargo test

# Run tests for a specific crate
cargo test -p clawseed-agent
cargo test -p clawseed-tools

# Run a specific test
cargo test -p clawseed-agent test_single_tool_call

# Using cargo-nextest (parallel execution, faster)
cargo nextest run --locked --workspace

# Show output (no truncation)
cargo test -- --nocapture

# Run ignored tests
cargo test -- --ignored
```

### Test Categories

| Type | Location | Description |
|------|----------|-------------|
| Unit tests | `#[cfg(test)]` within `src/` files | Test individual functions/structs |
| Integration tests | `tests/agent_integration.rs` | Full agent cycle E2E tests |
| Robustness tests | `tests/agent_robustness.rs` | Error handling and edge cases |
| System tests | `tests/agent_system.rs` | Real backend (SQLite, etc.) integration |

### Test Utilities

| Tool | Purpose |
|------|---------|
| `MockProvider` | Scripted LLM responses (FIFO queue) |
| `RecordingProvider` | Record requests for assertion |
| `EchoTool` | Echo tool (returns input) |
| `CountingTool` | Counting tool (tracks invocation count) |
| `RecordingTool` | Recording tool (captures arguments) |
| `FailingTool` | Failing tool (simulates errors) |
| `build_agent()` | Build test agent (NoneMemory + NoopObserver) |
| `build_agent_with_sqlite_memory()` | Build agent with SQLite memory |

### Coverage

```bash
# Generate coverage with cargo-llvm-cov
./tools/coverage.sh

# Output
# coverage/index.html  — HTML report
# lcov.info            — LCOV format
```

## CI Pipeline

### Flow Diagram

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
│  cargo-deny  │  │  llvm-cov    │  │  File size check         │
│  licenses    │  │  HTML + LCOV │  │  >1000 lines: warn       │
│  advisories  │  │              │  │  >3 files: error         │
└──────────────┘  └──────────────┘  └──────────────────────────┘
```

### Stage Details

#### Stage 1: Lint (15-minute timeout)

- Rust 1.87.0 + rustfmt + clippy
- `cargo fmt --all -- --check`
- `cargo clippy -- -D warnings` (all warnings as errors)
- Uses rust-cache for speed

#### Stage 2: Build (40-minute timeout, gated on Lint)

- **Parallel builds for two platforms**:
  - `x86_64-unknown-linux-gnu` (ubuntu-latest)
  - `aarch64-apple-darwin` (macos-14)
- Installs mold linker on Linux
- `cargo build --profile ci --locked --target`

#### Stage 2b: Check (20-minute timeout, parallel with Build)

- Feature matrix checks:
  - `--all-features`
  - `--no-default-features`
- `cargo check --locked`

#### Stage 3: Test (30-minute timeout, gated on Lint)

- Uses **cargo-nextest** for parallel execution
- `cargo nextest run --locked --workspace`
- Installs mold linker on Linux

#### Stage 4: Security (15-minute timeout, parallel with Test)

- **cargo-deny** checks licenses, sources, and advisories
- `cargo deny check`
- Configuration: `.cargo/audit.toml`

#### Stage 5: Coverage (30-minute timeout, parallel)

- **cargo-llvm-cov** generates coverage
- HTML + LCOV reports
- Uploads `lcov.info` as artifact

#### Stage 6: ADF Guard (5-minute timeout, parallel)

- Checks Rust file sizes
- >1000 lines: warning
- >3 files exceeding 1000 lines: error

### Concurrency Control

- New pushes automatically cancel in-progress builds on the same branch
- All stages must pass for success

## Code Quality Tools

```bash
# Format
cargo fmt --all

# Lint check
cargo clippy -- -D warnings

# Security audit
cargo deny check

# Dependency audit
cargo audit
```

## Profiling

```bash
# Build with release-fast profile + samply profiler
./tools/profile.sh
```
